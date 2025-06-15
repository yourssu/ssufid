use std::sync::LazyLock;

use futures::{StreamExt as _, stream::FuturesUnordered};
use reqwest::Url;
use scraper::{Html, Selector};
use thiserror::Error;
use time::{
    OffsetDateTime, PrimitiveDateTime,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};

// Corrected import path for ssufid types
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};

static SELECTORS: LazyLock<Selectors> = LazyLock::new(Selectors::new);

struct Selectors {
    post_list_item: Selector,
    post_link: Selector,
    post_title_in_list: Selector,
    post_author_in_list: Selector,
    notice_indicator: Selector,
    post_title: Selector,
    post_author: Selector,
    post_date: Selector,
    post_content: Selector,
    post_attachments: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_list_item: Selector::parse("table.board_list tr[class^=\"bg\"]").unwrap(),
            post_link: Selector::parse("td.subject a").unwrap(),
            post_title_in_list: Selector::parse("td.subject a").unwrap(),
            post_author_in_list: Selector::parse("td.name span.member").unwrap(),
            notice_indicator: Selector::parse("td.num b").unwrap(),
            post_title: Selector::parse("div[style*=\"font-size:13px; font-weight:bold\"]")
                .unwrap(),
            post_author: Selector::parse(
                "tr:first-child > td[style*=\"color:#888\"] > div[style*=\"float:left\"] .member",
            )
            .unwrap(),
            post_date: Selector::parse(
                "div[style*=\"margin-top:6px\"] > span[style*=\"color:#888\"]",
            )
            .unwrap(),
            post_content: Selector::parse("#writeContents").unwrap(),
            post_attachments: Selector::parse(
                "img[src=\"../skin/board/basic/img/icon_file.gif\"] ~ a",
            )
            .unwrap(), // Attachment links - assuming this is standard
        }
    }
}

const DATE_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]");

#[derive(Debug)]
struct LifelongEduMetadata {
    id: String, // wr_id
    url: String,
    title: String,  // Title from the list page
    author: String, // Author from the list page
    is_notice: bool,
}

#[derive(Debug, Error)]
enum LifelongEduError {
    #[error("URL not found for post item")]
    UrlNotFound,
    #[error("wr_id (post ID) not found in URL: {0}")]
    IdNotFoundInUrl(String),
    #[error("Title not found for post item")]
    TitleNotFound,
    // #[error("Author not found for post item")] // UNUSED
    // AuthorNotFound,
    #[error("Date not found for post item")]
    DateNotFound,
    #[error("Date parsing error: {0}")]
    DateParse(#[from] time::error::Parse),
    #[error("Content not found on post page: {0}")]
    ContentNotFound(String),
}

impl From<LifelongEduError> for PluginError {
    fn from(err: LifelongEduError) -> Self {
        PluginError::parse::<LifelongEduPlugin>(err.to_string())
    }
}

pub struct LifelongEduPlugin;

impl LifelongEduPlugin {
    pub fn new() -> Self {
        Self
    }

    fn parse_wr_id_from_url(url_str: &str) -> Result<String, LifelongEduError> {
        let parsed_url = Url::parse(url_str).map_err(|_| LifelongEduError::UrlNotFound)?; // Should be a valid URL
        parsed_url
            .query_pairs()
            .find_map(|(key, value)| {
                if key == "wr_id" {
                    Some(value.into_owned())
                } else {
                    None
                }
            })
            .ok_or_else(|| LifelongEduError::IdNotFoundInUrl(url_str.to_string()))
    }

    fn parse_datetime_from_post(datetime_str: &str) -> Result<OffsetDateTime, LifelongEduError> {
        // Expected input like "작성일 : YY-MM-DD HH:MM" or just "YY-MM-DD HH:MM"
        let date_time_part_to_parse = if let Some(part) = datetime_str.split(" : ").nth(1) {
            part.trim().to_string()
        } else {
            datetime_str.trim().to_string()
        };

        Ok(
            PrimitiveDateTime::parse(&format!("20{}", date_time_part_to_parse), DATE_FORMAT)
                .map_err(|e| {
                    tracing::error!(
                        "PrimitiveDateTime::parse failed for '{}': {:?}",
                        date_time_part_to_parse,
                        e
                    );
                    LifelongEduError::DateParse(e)
                })?
                .assume_offset(offset!(+9)),
        )
    }

    fn as_attachment(element: scraper::ElementRef) -> Option<Attachment> {
        let href = element.value().attr("href")?;
        let mut args = href
            .split("download(")
            .nth(1)?
            .strip_suffix(");")?
            .split(", ")
            .map(|s| s.trim_matches('\''));
        let url = Url::parse(Self::BASE_URL)
            .ok()?
            .join(args.next()?.strip_prefix("./")?)
            .ok()?
            .to_string();
        let name = element
            .child_elements()
            .next()?
            .text()
            .collect::<String>()
            .trim()
            .to_string();
        Some(Attachment {
            url,
            name: Some(name),
            mime_type: None,
        })
    }

    async fn fetch_page_posts_metadata(
        &self,
        page: u32,
    ) -> Result<Vec<LifelongEduMetadata>, PluginError> {
        let page_url = format!(
            "{}&page={}",
            LifelongEduPlugin::BASE_URL, // The main board URL
            page
        );
        tracing::info!("Fetching metadata from: {}", page_url);

        let response_text = reqwest::get(&page_url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);
        let mut posts_metadata = Vec::new();

        for element in document.select(&SELECTORS.post_list_item) {
            // Skip header rows if any (GNUBoard usually doesn't have a th in tbody for posts)
            // Check if it's a notice post (these often lack a proper wr_id or are duplicated)
            let is_notice = element.select(&SELECTORS.notice_indicator).next().is_some();

            let link_element = element.select(&SELECTORS.post_link).next();

            if link_element.is_none() && is_notice {
                // Some notices might not have a direct link in the same way, or are just text.
                // Or they might be linked differently. For now, we skip if no link.
                tracing::warn!(
                    "Skipping potential notice row due to missing link element. HTML: {:?}",
                    element.html()
                );
                continue;
            }

            let link_element = link_element.ok_or(LifelongEduError::UrlNotFound)?;

            let partial_url = link_element
                .value()
                .attr("href")
                .ok_or(LifelongEduError::UrlNotFound)?
                .to_string();

            // Construct absolute URL
            let base_url_obj = Url::parse(LifelongEduPlugin::BASE_URL).unwrap(); // Base URL of the board
            let url = base_url_obj
                .join(&partial_url)
                .map_err(|_| LifelongEduError::UrlNotFound)? // Handle malformed partial_url
                .to_string();

            let id = Self::parse_wr_id_from_url(&url)?;

            let title = element
                .select(&SELECTORS.post_title_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .ok_or(LifelongEduError::TitleNotFound)?;

            let author = element
                .select(&SELECTORS.post_author_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            posts_metadata.push(LifelongEduMetadata {
                id,
                url,
                title,
                author,
                is_notice,
            });
        }
        Ok(posts_metadata)
    }

    async fn fetch_post_details(
        &self,
        metadata: &LifelongEduMetadata,
    ) -> Result<SsufidPost, PluginError> {
        tracing::info!(
            "Fetching post details for ID {}: {}",
            metadata.id,
            metadata.url
        );
        let response_text = reqwest::get(&metadata.url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);

        let title = document
            .select(&SELECTORS.post_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            // Use title from metadata if not found on page, or error out
            .unwrap_or_else(|| metadata.title.clone());

        // Author on post page might be different or more detailed
        let author_on_page = document
            .select(&SELECTORS.post_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.author.clone()); // Fallback to list author

        let date_str = document
            .select(&SELECTORS.post_date)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .ok_or(LifelongEduError::DateNotFound)?;
        let created_at = Self::parse_datetime_from_post(&date_str)?;

        let content_html = document
            .select(&SELECTORS.post_content)
            .next()
            .map(|el| el.html())
            .ok_or_else(|| LifelongEduError::ContentNotFound(metadata.url.clone()))?;

        let attachments = document
            .select(&SELECTORS.post_attachments)
            .filter_map(Self::as_attachment)
            .collect::<Vec<_>>();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            title,
            author: Some(author_on_page),
            description: None,
            category: if metadata.is_notice {
                vec!["공지".to_string()]
            } else {
                vec![]
            },
            created_at,
            updated_at: None,
            thumbnail: None,
            content: content_html,
            attachments,
            metadata: None,
        })
    }
}

impl Default for LifelongEduPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SsufidPlugin for LifelongEduPlugin {
    const IDENTIFIER: &'static str = "lifelongedu.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 평생교육학과";
    const DESCRIPTION: &'static str = "숭실대학교 평생교육학과 공지사항을 제공합니다.";
    // Base URL for the board itself
    const BASE_URL: &'static str = "http://lifelongedu.ssu.ac.kr/bbs/board.php?bo_table=univ";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        tracing::info!(
            "Starting crawl for SsuLifelongEduPlugin with limit: {}",
            posts_limit
        );
        let mut all_metadata = Vec::new();
        let mut current_page = 1;
        let max_pages_to_check = (posts_limit / 15) + 2; // Heuristic: check a couple more pages

        loop {
            if current_page > max_pages_to_check {
                tracing::info!(
                    "Reached max pages to check ({}), stopping metadata fetch.",
                    max_pages_to_check
                );
                break;
            }
            if all_metadata.len() >= posts_limit as usize {
                tracing::info!(
                    "Collected enough metadata ({}) for posts_limit ({}), stopping metadata fetch.",
                    all_metadata.len(),
                    posts_limit
                );
                break;
            }

            tracing::info!("Fetching metadata from page: {}", current_page);
            let page_metadata = self.fetch_page_posts_metadata(current_page).await?;

            if page_metadata.is_empty() {
                tracing::info!(
                    "No metadata found on page {}, assuming end of posts.",
                    current_page
                );
                break; // No more posts on this page, assume end
            }

            for meta in page_metadata {
                if !all_metadata
                    .iter()
                    .any(|m: &LifelongEduMetadata| m.id == meta.id)
                {
                    // Avoid duplicates
                    all_metadata.push(meta);
                }
            }
            current_page += 1;
        }

        // Take only up to posts_limit
        all_metadata.truncate(posts_limit as usize);

        let post_futures = all_metadata
            .iter()
            .map(|metadata| self.fetch_post_details(metadata))
            .collect::<FuturesUnordered<_>>();

        let all_posts = post_futures
            .collect::<Vec<Result<SsufidPost, PluginError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<SsufidPost>, PluginError>>()?;

        tracing::info!(
            "Successfully crawled {} posts from SsuLifelongEduPlugin.",
            all_posts.len()
        );
        Ok(all_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssufid::core::SsufidPlugin; // Ensure SsufidPlugin trait is in scope for tests
    use tracing_subscriber::{EnvFilter, fmt};

    fn setup_tracing() {
        let _ = fmt()
            .with_env_filter(
                EnvFilter::from_default_env()
                    .add_directive("ssufid_lifelongedu=info".parse().unwrap()),
            )
            .try_init();
    }

    #[test]
    fn test_parse_wr_id() {
        setup_tracing();
        let url1 = "http://lifelongedu.ssu.ac.kr/bbs/board.php?bo_table=univ&wr_id=710";
        assert_eq!(
            LifelongEduPlugin::parse_wr_id_from_url(url1).unwrap(),
            "710"
        );

        let url2 =
            "http://lifelongedu.ssu.ac.kr/bbs/board.php?bo_table=univ&wr_id=710&page=2&test=true";
        assert_eq!(
            LifelongEduPlugin::parse_wr_id_from_url(url2).unwrap(),
            "710"
        );

        let url3 = "http://lifelongedu.ssu.ac.kr/bbs/board.php?bo_table=univ";
        assert!(LifelongEduPlugin::parse_wr_id_from_url(url3).is_err());
    }

    #[tokio::test]
    async fn test_fetch_page_metadata_real() {
        setup_tracing();
        let plugin = LifelongEduPlugin::new();
        let metadata = plugin.fetch_page_posts_metadata(1).await.unwrap();
        assert!(
            !metadata.is_empty(),
            "Should fetch some metadata from page 1"
        );
        tracing::info!(
            "Fetched metadata (first 5): {:?}",
            &metadata[..std::cmp::min(5, metadata.len())]
        );
        // Check if a known wr_id exists (e.g. 710 was used before)
        // This might fail if post 710 is not on page 1 anymore
        // assert!(metadata.iter().any(|m| m.id == "710"));
    }

    #[test]
    fn test_parse_attachment() {
        setup_tracing();
        let html = r#"<a href="javascript:file_download('./download.php?bo_table=univ&amp;wr_id=709&amp;no=0', '%EA%B5%90%EB%82%B4%EC%9E%A5%ED%95%99%EA%B8%88+%EC%8B%A0%EC%B2%AD+%EB%A7%A4%EB%89%B4%EC%96%BC.pdf');" title="">&nbsp;<span style="color:#888;">교내장학금 신청 매뉴얼.pdf (374.8K)</span>&nbsp;<span style="color:#ff6600; font-size:11px;">[4]</span>&nbsp;<span style="color:#d3d3d3; font-size:11px;">DATE : 2025-06-04 11:58:11</span></a>"#;
        let document = Html::parse_document(html);
        let attachment_element = document
            .select(&Selector::parse("a").unwrap())
            .next()
            .unwrap();
        let attachment = LifelongEduPlugin::as_attachment(attachment_element);
        assert!(attachment.is_some());
        let attachment = attachment.unwrap();
        assert_eq!(
            attachment.url,
            "http://lifelongedu.ssu.ac.kr/bbs/download.php?bo_table=univ&wr_id=709&no=0"
        );
    }

    #[tokio::test]
    async fn test_fetch_single_post_real() {
        setup_tracing();
        // This wr_id might change, pick one from the latest page 1 listing if test fails
        let sample_wr_id = "709";
        let sample_url = format!(
            "http://lifelongedu.ssu.ac.kr/bbs/board.php?bo_table=univ&wr_id={}",
            sample_wr_id
        );
        let metadata = LifelongEduMetadata {
            id: sample_wr_id.to_string(),
            url: sample_url,
            title: "Test Title (from metadata)".to_string(), // This will be overridden by page title
            author: "Test Author (from metadata)".to_string(), // This will be overridden
            is_notice: false,
        };
        let plugin = LifelongEduPlugin::new();
        let post = plugin.fetch_post_details(&metadata).await.unwrap();

        assert_eq!(post.id, sample_wr_id);
        assert!(!post.title.is_empty());
        assert!(post.title.contains("2025학년도 2학기 교내장학금"));
        assert_eq!(post.author, Some("관리자".to_string()));
        assert!(!post.content.is_empty());
        tracing::info!("Fetched post: {:?}", post);
    }

    #[tokio::test]
    async fn test_crawl_integration_limited() {
        setup_tracing();
        let plugin = LifelongEduPlugin::new();
        let posts_limit = 5; // Test with a small limit
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert!(posts.len() <= posts_limit as usize);
        assert!(
            !posts.is_empty() || posts_limit == 0,
            "Should crawl at least one post if limit > 0 and posts exist"
        );
        if !posts.is_empty() {
            tracing::info!("Crawled post (first one): {:?}", posts[0]);
            assert!(!posts[0].id.is_empty());
            assert!(!posts[0].url.is_empty());
            assert!(!posts[0].title.is_empty());
        }
    }
}
