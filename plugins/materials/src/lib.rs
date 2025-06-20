use futures::TryStreamExt;
use futures::stream::FuturesOrdered;
use reqwest::Client;
use scraper::{Html, Selector};
use ssufid::core::{Attachment, SsufidPlugin, SsufidPost};
use ssufid::error::PluginError;
use time::Date;
use time::format_description::BorrowedFormatItem;
use time::macros::{format_description, offset};
use url::Url;

const BASE_URL_HOST_ONLY: &str = "https://materials.ssu.ac.kr";

#[derive(Debug, Clone)]
struct Selectors {
    // For listing page
    list_item_selector: Selector,
    post_link_selector: Selector,
    title_selector: Selector,
    notice_icon_selector: Selector,

    // For individual post page (detail view)
    post_title_selector_detail: Selector,
    post_date_selector_detail: Selector,
    post_content_selector: Selector,
    attachment_item_selector: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            list_item_selector: Selector::parse("div.news-list > ul > li")
                .expect("Failed to parse list_item_selector"),
            post_link_selector: Selector::parse("a").expect("Failed to parse post_link_selector"),
            title_selector: Selector::parse("div.txt_box > div.tit_box > strong")
                .expect("Failed to parse title_selector"),
            notice_icon_selector: Selector::parse(
                "div.txt_box > div.tit_box > strong > span.tag01",
            )
            .expect("Failed to parse notice_icon_selector"),

            post_title_selector_detail: Selector::parse(".basic_bd01_view > .inner > .tit_box > p")
                .expect("Failed to parse post_title_selector_detail"),
            post_date_selector_detail: Selector::parse(
                ".basic_bd01_view > .inner > .tit_box > span",
            )
            .expect("Failed to parse post_date_selector_detail"),
            post_content_selector: Selector::parse(".basic_bd01_view > .inner > .view_box")
                .expect("Failed to parse post_content_selector"),
            attachment_item_selector: Selector::parse(
                ".basic_bd01_view > .inner > .view_box > .file_box > ul > li > a",
            )
            .expect("Failed to parse attachment_item_selector"),
        }
    }
}

#[derive(Debug, Clone)]
struct PostMetadata {
    id: String,
    url: String,
    title: String,
    is_notice: bool,
}

#[derive(Debug, Clone)]
struct MaterialsPost {
    id: String,
    url: String,
    title: String,
    is_notice: bool,
    created_at: Date,
    content: String,
    attachments: Vec<Attachment>,
}

impl From<MaterialsPost> for SsufidPost {
    fn from(post: MaterialsPost) -> Self {
        SsufidPost {
            id: post.id,
            url: post.url,
            title: post.title,
            author: None,
            description: None,
            category: post
                .is_notice
                .then_some(vec!["공지".to_string()])
                .unwrap_or_default(),
            created_at: post.created_at.midnight().assume_offset(offset!(+9)),
            updated_at: None,
            thumbnail: None,
            content: post.content,
            attachments: post.attachments,
            metadata: None,
        }
    }
}

pub struct MaterialsPlugin {
    selectors: Selectors,
    client: Client,
}

impl Default for MaterialsPlugin {
    fn default() -> Self {
        Self {
            selectors: Selectors::new(),
            client: Client::builder()
                        .danger_accept_invalid_certs(true) // No trailing whitespace
                        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.0.0 Safari/537.36")
                        .build()
                        .unwrap(),
        }
    }
}

impl SsufidPlugin for MaterialsPlugin {
    const IDENTIFIER: &'static str = "materials.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 신소재공학과 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 신소재공학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://materials.ssu.ac.kr/bbs/board.php?tbl=bbs51";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        tracing::info!("Crawling started. Limit: {}", posts_limit);

        let mut collected_metadata: Vec<PostMetadata> = Vec::new();
        let mut page_count = 0;

        loop {
            page_count += 1;
            tracing::debug!("Fetching metadata page {}: {}", page_count, page_count);
            let page_meta = self.fetch_post_metadata(page_count).await?;
            tracing::info!(
                "Metadata Page {} yielded {} items.",
                page_count,
                page_meta.len()
            );

            for meta in page_meta {
                if collected_metadata.len() < posts_limit as usize {
                    collected_metadata.push(meta);
                } else {
                    break;
                }
            }

            if collected_metadata.len() >= posts_limit as usize {
                tracing::info!(
                    "Reached posts_limit for metadata ({}) with {} items.",
                    posts_limit,
                    collected_metadata.len()
                );
                break;
            }

            if page_count > 20 {
                tracing::warn!("Reached metadata page limit of 20. Stopping.");
                break;
            }
        }
        tracing::info!(
            "Collected {} post metadata items in total.",
            collected_metadata.len()
        );

        Ok(collected_metadata
            .into_iter()
            .map(|meta| {
                tracing::debug!(
                    "Fetching full details for post ID {}: {}",
                    meta.id,
                    meta.url
                );
                self.post_details(meta, &self.client)
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<MaterialsPost>>()
            .await?
            .into_iter()
            .map(SsufidPost::from)
            .collect())
    }
}

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year].[month].[day]");

impl MaterialsPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    async fn fetch_post_metadata(&self, page: u32) -> Result<Vec<PostMetadata>, PluginError> {
        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching metadata from page: {}", page);

        let response_text = self
            .client
            .get(format!("{}&page={}", MaterialsPlugin::BASE_URL, page))
            .send()
            .await
            .map_err(|e| PluginError::request::<MaterialsPlugin>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::request::<MaterialsPlugin>(e.to_string()))?;

        let document = Html::parse_document(&response_text);

        document
            .select(&self.selectors.list_item_selector)
            .map(|e| {
                let post_link_element = e
                    .select(&self.selectors.post_link_selector)
                    .next()
                    .ok_or_else(|| {
                        PluginError::parse::<MaterialsPlugin>(format!(
                            "No post link found in item. HTML: {}",
                            e.html()
                        ))
                    })?;

                let relative_url_str = post_link_element
                    .value()
                    .attr("href")
                    .ok_or(PluginError::parse::<MaterialsPlugin>(format!(
                        "No 'href' attribute found in post link element for item. HTML: {}",
                        post_link_element.html()
                    )))?
                    .trim();

                let absolute_url = Url::parse(BASE_URL_HOST_ONLY)
                    .expect("BASE_URL_HOST_ONLY should be a valid URL")
                    .join(relative_url_str)
                    .map_err(|e| {
                        PluginError::parse::<MaterialsPlugin>(format!(
                            "Failed to join post URL '{}' with base '{}': {}",
                            relative_url_str, BASE_URL_HOST_ONLY, e
                        ))
                    })?;

                let post_id = absolute_url
                    .query_pairs()
                    .find(|(key, _)| key == "num")
                    .map(|(_, value)| value.into_owned())
                    .ok_or_else(|| {
                        PluginError::parse::<MaterialsPlugin>(format!(
                            "Could not extract post ID ('num') from URL: {}",
                            absolute_url
                        ))
                    })?;

                let is_notice = post_link_element
                    .select(&self.selectors.notice_icon_selector)
                    .next()
                    .is_some();

                let title = post_link_element
                    .select(&self.selectors.title_selector)
                    .next()
                    .ok_or(PluginError::parse::<MaterialsPlugin>(format!(
                        "No title found in post link element for item {}. HTML: {}",
                        &post_id,
                        post_link_element.html()
                    )))?
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();
                tracing::info!(
                    "Successfully extracted PostMetadata for post ID '{}'",
                    &post_id
                );
                Ok(PostMetadata {
                    id: post_id,
                    url: absolute_url.to_string(),
                    title,
                    is_notice,
                })
            })
            .collect::<Result<Vec<_>, PluginError>>()
    }

    async fn post_details(
        &self,
        meta: PostMetadata,
        client: &Client,
    ) -> Result<MaterialsPost, PluginError> {
        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching full post details for ID {}: {}", meta.id, meta.url);
        let response_text = client
            .get(&meta.url)
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<MaterialsPlugin>(format!(
                    "Failed to fetch post page {}: {}",
                    meta.url, e
                ))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::request::<MaterialsPlugin>(format!(
                    "Failed to read post page text {}: {}",
                    meta.url, e
                ))
            })?;

        let document = Html::parse_document(&response_text);

        let title = document.select(&self.selectors.post_title_selector_detail).next()
            .map_or_else(
                || {
                    tracing::warn!(target: MaterialsPlugin::IDENTIFIER, "Title not found on detail page for {}. Using title from metadata: '{}'", meta.url, meta.title);
                    meta.title.clone()
                },
                |el| el.text().collect::<String>().trim().to_string()
            );

        let date_text = document
            .select(&self.selectors.post_date_selector_detail)
            .next()
            .ok_or(PluginError::parse::<MaterialsPlugin>(format!(
                "No date found in post detail page for {}",
                meta.url
            )))?
            .text()
            .collect::<String>();

        let created_at = Date::parse(date_text.trim(), DATE_FORMAT).map_err(|e| {
            PluginError::parse::<MaterialsPlugin>(format!(
                "Failed to parse date '{}' for post {}: {}",
                date_text, meta.url, e
            ))
        })?;

        let content_html = document
            .select(&self.selectors.post_content_selector)
            .next()
            .map_or_else(String::new, |el| el.inner_html());

        let attachments = document
            .select(&self.selectors.attachment_item_selector)
            .map(|e| {
                let rel_url = e.value().attr("href").ok_or_else(|| {
                    PluginError::parse::<MaterialsPlugin>(format!(
                        "No 'href' attribute found in attachment element: {}",
                        e.html()
                    ))
                })?;

                let url = format!("{}{}", BASE_URL_HOST_ONLY, rel_url);

                let name = e.text().collect::<String>().trim().to_string();

                Ok(Attachment {
                    url,
                    name: Some(name),
                    mime_type: None,
                })
            })
            .collect::<Result<Vec<Attachment>, _>>()?;

        Ok(MaterialsPost {
            id: meta.id.clone(),
            url: meta.url.clone(),
            title,
            is_notice: meta.is_notice,
            created_at,
            content: content_html,
            attachments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{error, info, warn};

    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(format!("{}=trace", MaterialsPlugin::IDENTIFIER))
            .try_init();
    }

    #[tokio::test]
    async fn test_crawl_collects_full_posts() {
        init_tracing();
        let plugin = MaterialsPlugin::new();
        let posts_limit = 3;

        match plugin.crawl(posts_limit).await {
            Ok(posts_vec) => {
                assert!(!posts_vec.is_empty(), "Crawl should return some posts.");
                assert!(
                    posts_vec.len() <= posts_limit as usize,
                    "Should not exceed posts_limit."
                );
                for post in posts_vec {
                    assert!(!post.id.is_empty(), "Post ID should not be empty");
                    assert!(
                        !post.title.is_empty(),
                        "Post title should not be empty for ID {}",
                        post.id
                    );
                    info!(target: MaterialsPlugin::IDENTIFIER, "Tested Post: ID={}, Title='{}', HasContent={}", post.id, post.title, !post.content.is_empty());
                }
            }
            Err(e) => {
                panic!("Crawl failed: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_first_page_metadata_directly() {
        init_tracing();
        let plugin = MaterialsPlugin::new();

        tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Testing fetch_page_post_metadata_helper with BASE_URL: {}", MaterialsPlugin::BASE_URL);

        let result = plugin.fetch_post_metadata(MaterialsPlugin::BASE_URL).await;

        if let Err(e) = &result {
            error!(target: MaterialsPlugin::IDENTIFIER, "fetch_page_post_metadata_helper failed: {:?}", e);
        }
        let (metadata, next_page_url_opt) =
            result.expect("Fetching metadata from first page should succeed");

        if metadata.is_empty() {
            let debug_client = reqwest::Client::new();
            let page_html_for_debug = debug_client
                .get(MaterialsPlugin::BASE_URL)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            warn!(target: MaterialsPlugin::IDENTIFIER,
                "No metadata collected from the first page. This might be due to website changes or incorrect selectors. \
                Current list_item_selector: '{}'. Page HTML for debug:\n{}",
                plugin.selectors.list_item_selector_str,
                page_html_for_debug
            );
        }

        assert!(
            !metadata.is_empty(),
            "Should collect some metadata from the first page."
        );

        for item in &metadata {
            assert!(
                !item.id.is_empty(),
                "Post ID should not be empty: {:?}",
                item
            );
            assert!(
                item.url.starts_with(BASE_URL_HOST_ONLY),
                "Post URL ('{}') should be absolute, starting with '{}': {:?}",
                item.url,
                BASE_URL_HOST_ONLY,
                item
            );
            assert!(
                !item.title.is_empty(),
                "Post title should not be empty: {:?}",
                item
            );
            // assert!(!item.date_str.is_empty(), "Post date string should not be empty: {:?}", item); // date_str removed from PostMetadata
        }

        if let Some(url) = &next_page_url_opt {
            info!(target: MaterialsPlugin::IDENTIFIER, "Next page URL from metadata helper: {}", url);
            assert!(
                url.starts_with(BASE_URL_HOST_ONLY),
                "Next page URL ('{}') should be absolute if present.",
                url
            );
            assert!(
                url.contains("page="),
                "Next page URL ('{}') should typically contain 'page='.",
                url
            );
        } else {
            info!(target: MaterialsPlugin::IDENTIFIER, "No next page URL found from the first page, this might be normal if there's only one page of results.");
        }
        info!(target: MaterialsPlugin::IDENTIFIER, "First page metadata count: {}. Next page URL: {:?}", metadata.len(), next_page_url_opt);
    }

    #[tokio::test]
    async fn test_fetch_single_post_detail() {
        init_tracing();
        let plugin = MaterialsPlugin::new();

        let metadata_items_result = plugin.fetch_post_metadata(MaterialsPlugin::BASE_URL).await;
        assert!(
            metadata_items_result.is_ok(),
            "Fetching metadata for single post detail test failed: {:?}",
            metadata_items_result.err()
        );
        let metadata_items = metadata_items_result.unwrap().0;

        assert!(
            !metadata_items.is_empty(),
            "Need at least one post from listing to test detail fetching."
        );

        let test_meta = metadata_items[0].clone();
        info!(target: MaterialsPlugin::IDENTIFIER, "Testing full detail fetch for: ID={}, URL={}", test_meta.id, test_meta.url);

        match plugin.post_details(test_meta.clone(), &plugin.client).await {
            Ok(full_data) => {
                assert_eq!(full_data.id, test_meta.id);
                assert_eq!(full_data.url, test_meta.url);
                assert!(
                    !full_data.title.is_empty(),
                    "Full post title should not be empty"
                );
                info!(target: MaterialsPlugin::IDENTIFIER, "Fetched Full Data: Title='{}', Author='{:?}', ContentNotEmpty={}", full_data.title, full_data.author, !full_data.content.is_empty());
                if full_data.author.is_none() {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "Author was None for post ID {}", full_data.id);
                }
                assert!(
                    full_data.created_at.year() > 2000,
                    "Parsed year seems too old, check date parsing. Year: {}",
                    full_data.created_at.year()
                );
            }
            Err(e) => {
                panic!(
                    "fetch_full_post_details failed for URL {}: {:?}",
                    test_meta.url, e
                );
            }
        }
    }
}
