// Removed: use std::future::Future;
// Removed: use std::pin::Pin;
use futures::{StreamExt, stream::FuturesOrdered}; // Removed FutureExt
use scraper::{Html, Selector}; // Added back missing import
// Removed: use thiserror::Error;
use time::Date;
use url::Url;

use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};

struct Selectors {
    post_item: Selector,
    post_title: Selector,
    post_date: Selector,
    detail_title: Selector,
    detail_date: Selector,
    detail_content: Selector,
    detail_attachments: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_item: Selector::parse("div.board-list-body > div.col")
                .expect("Failed to parse post_item selector"),
            post_title: Selector::parse("p.b-title > a")
                .expect("Failed to parse post_title selector"),
            post_date: Selector::parse("p.b-date").expect("Failed to parse post_date selector"),
            detail_title: Selector::parse("div.titlearea > h4")
                .expect("Failed to parse detail_title selector"),
            detail_date: Selector::parse("ul.date-view > li:nth-child(1)")
                .expect("Failed to parse detail_date selector"),
            detail_content: Selector::parse("div.board-content")
                .expect("Failed to parse detail_content selector"),
            detail_attachments: Selector::parse("div.board-attach a")
                .expect("Failed to parse detail_attachments selector"),
        }
    }
}

pub struct LawyerPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl Default for LawyerPlugin {
    fn default() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug)]
struct PostMetadata {
    id: String,
    url: Url,
    title: String,
    date_str: String,
}

impl LawyerPlugin {
    const LIST_PAGE_URL_BASE: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_list.do"; // Renamed for clarity
    const VIEW_PAGE_URL_BASE: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_view.do";
    const ATTACHMENT_BASE_URL: &'static str = "http://lawyer.ssu.ac.kr"; // Added for attachments
    const DATE_FORMAT_STR: &'static str = "[year]-[month]-[day]";

    async fn fetch_page_posts_metadata(
        &self,
        page_num: u32,
    ) -> Result<Vec<PostMetadata>, PluginError> {
        let url_str = format!("{}?pageno={}", Self::LIST_PAGE_URL_BASE, page_num);
        tracing::debug!(url = %url_str, page = page_num, "Fetching notice list page for metadata");

        let response = self
            .http_client
            .get(&url_str)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        if !response.status().is_success() {
            tracing::error!(
                "Failed to fetch list page {}: HTTP Status {}",
                url_str,
                response.status()
            );
            return Err(PluginError::request::<Self>(format!(
                "HTTP Error: {} for page {}",
                response.status(),
                page_num
            )));
        }

        let html_content = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        let document = Html::parse_document(&html_content);

        let posts_on_page = document
            .select(&self.selectors.post_item)
            .map(|element| {
                let id = element
                    .value()
                    .attr("id")
                    .map(str::to_string)
                    .ok_or_else(|| {
                        PluginError::parse::<Self>(
                            "Failed to extract post ID from div.col".to_string(),
                        )
                    })?;

                let title_element = element
                    .select(&self.selectors.post_title)
                    .next()
                    .ok_or_else(|| {
                        PluginError::parse::<Self>(format!("Failed to find title for post ID {}", id))
                    })?;
                let title = title_element.text().collect::<String>().trim().to_string();

                let date_str = element
                    .select(&self.selectors.post_date)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let detail_url_str = format!("{}?pdsid={}", Self::VIEW_PAGE_URL_BASE, id);
                let url = Url::parse(&detail_url_str).map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Failed to parse post URL {}: {}",
                        detail_url_str, e
                    ))
                })?;
                tracing::trace!(
                    "Found post metadata on page {}: id={}, title='{}', date='{}'",
                    page_num,
                    id,
                    title,
                    date_str
                );

                Ok(PostMetadata {
                    id,
                    url,
                    title,
                    date_str,
                })
            })
            .collect::<Result<Vec<PostMetadata>, PluginError>>()?;

        Ok(posts_on_page)
    }

    async fn fetch_post_details(
        &self,
        metadata: Box<PostMetadata>,
    ) -> Result<SsufidPost, PluginError> {
        tracing::debug!(post_id = %metadata.id, post_url = %metadata.url, "Fetching post details");
        let response = self
            .http_client
            .get(metadata.url.clone())
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let html_content = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        let document = Html::parse_document(&html_content);

        let title = document
            .select(&self.selectors.detail_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.title.clone());

        let author_from_detail = Some("국제법무학과".to_string());
        let mut date_str_from_detail = metadata.date_str.clone();

        if let Some(date_el_container) = document.select(&self.selectors.detail_date).next() {
            let full_text = date_el_container.text().collect::<String>();
            if let Some(date_part) = full_text.split("등록일").nth(1) {
                date_str_from_detail = date_part
                    .split_whitespace()
                    .next()
                    .unwrap_or(&metadata.date_str)
                    .to_string();
            }
        }

        let date_format = time::format_description::parse(Self::DATE_FORMAT_STR).map_err(|e| {
            PluginError::parse::<Self>(format!(
                "Failed to parse date format string '{}': {}",
                Self::DATE_FORMAT_STR,
                e
            ))
        })?;

        let created_at = Date::parse(&date_str_from_detail, &date_format)
            .map_err(|e| {
                PluginError::parse::<Self>(format!(
                    "Failed to parse date string '{}' with format '{}': {}",
                    date_str_from_detail,
                    Self::DATE_FORMAT_STR,
                    e
                ))
            })?
            .midnight()
            .assume_utc()
            .to_offset(time::macros::offset!(+9));

        let content_element = document.select(&self.selectors.detail_content).next();
        let content = content_element
            .map(|el| el.inner_html())
            .unwrap_or_default();
        if content.trim().is_empty() && content_element.is_some() {
            tracing::warn!(post_id = %metadata.id, "Parsed content is empty or whitespace only.");
        }

        let attachment_base_url = Url::parse(Self::ATTACHMENT_BASE_URL)
            .map_err(|e| PluginError::Configuration(format!("Invalid attachment base URL: {}", e)))?;

        let attachments = document
            .select(&self.selectors.detail_attachments)
            .filter_map(|el| {
                let name = el.text().collect::<String>().trim().to_string();
                el.value().attr("href").and_then(|href_val| {
                    let attachment_url_str = if href_val.starts_with("javascript:downpage(") {
                        let params_part = href_val
                            .trim_start_matches("javascript:downpage(")
                            .trim_end_matches(")");
                        let params: Vec<&str> = params_part
                            .split(',')
                            .map(|s| s.trim().trim_matches('\''))
                            .collect();
                        if params.len() == 3 {
                            format!(
                                "javascript_download:original_name={},server_file={},folder={}",
                                params[0], params[1], params[2]
                            )
                        } else {
                            tracing::warn!(post_id = %metadata.id, href = href_val, "Unexpected number of params in javascript:downpage");
                            // Keep the original href_val as a fallback, or decide to skip
                            // For now, let's skip if params are not as expected to avoid malformed URLs
                            return None;
                        }
                    } else {
                        match attachment_base_url.join(href_val.trim()) {
                            Ok(full_url) => full_url.to_string(),
                            Err(e) => {
                                tracing::warn!(post_id = %metadata.id, href = href_val, error = %e, "Failed to join attachment URL with base");
                                // Fallback to an invalid URL or skip
                                return None; // Skip this attachment
                            }
                        }
                    };
                    Some(Attachment {
                        name: Some(name),
                        url: attachment_url_str,
                        mime_type: None,
                    })
                })
            })
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.to_string(),
            title,
            author: author_from_detail,
            description: None,
            category: vec!["공지사항".to_string()],
            created_at,
            updated_at: None,
            thumbnail: None,
            content,
            attachments,
            metadata: None,
        })
    }
}

impl SsufidPlugin for LawyerPlugin {
    const IDENTIFIER: &'static str = "lawyer.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 법과대학";
    const DESCRIPTION: &'static str = "숭실대학교 법과대학 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_list.do";

    // Applied clippy's suggestion for needless_lifetimes (removed 'a)
    fn crawl(
        &self,
        posts_limit: u32,
    ) -> impl futures::Future<
        Output = std::result::Result<
            std::vec::Vec<ssufid::core::SsufidPost>,
            ssufid::error::PluginError,
        >,
    > + std::marker::Send {
        Box::pin(async move {
            let mut all_posts_metadata: Vec<PostMetadata> = Vec::new();
            let mut current_page_num = 1;
            const MAX_PAGES_TO_CRAWL: u32 = 500; // Safety break

            tracing::debug!(
                "Starting crawl for plugin: {}, posts_limit: {}",
                Self::IDENTIFIER,
                posts_limit
            );

            loop {
                tracing::debug!(
                    "Attempting to fetch metadata from page {}",
                    current_page_num
                );
                let metadata_on_page = match self.fetch_page_posts_metadata(current_page_num).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        // If a page fetch fails, decide if it's critical or skippable
                        // For now, let's assume it's critical for a specific page,
                        // but if it's a request error for a non-first page, maybe log and break.
                        if current_page_num == 1 {
                            tracing::error!("Failed to fetch metadata from first page: {:?}", e);
                            return Err(e);
                        } else {
                            tracing::warn!("Failed to fetch metadata from page {}: {:?}. Assuming end of posts.", current_page_num, e);
                            break; // Stop if a subsequent page fails
                        }
                    }
                };

                if metadata_on_page.is_empty() {
                    tracing::debug!(
                        "No more posts found on page {}. Assuming it's the last page or page is empty.",
                        current_page_num
                    );
                    break;
                }

                all_posts_metadata.extend(metadata_on_page);

                if posts_limit > 0 && all_posts_metadata.len() >= posts_limit as usize {
                    tracing::debug!(
                        "Reached or exceeded posts_limit ({}) with {} posts. Truncating.",
                        posts_limit,
                        all_posts_metadata.len()
                    );
                    all_posts_metadata.truncate(posts_limit as usize);
                    break;
                }

                if current_page_num >= MAX_PAGES_TO_CRAWL {
                    tracing::warn!(
                        "Reached maximum page limit ({}) for crawling. Stopping.",
                        MAX_PAGES_TO_CRAWL
                    );
                    break;
                }

                current_page_num += 1;
                tracing::trace!("Advanced to page {}", current_page_num);
            }

            tracing::debug!(
                "Collected {} post metadata items in total. Fetching details...",
                all_posts_metadata.len()
            );

            let mut futures_ordered = FuturesOrdered::new();
            for metadata in all_posts_metadata {
                futures_ordered.push_back(self.fetch_post_details(Box::new(metadata)));
            }

            let mut posts: Vec<SsufidPost> = Vec::new();
            while let Some(result) = futures_ordered.next().await {
                match result {
                    Ok(post) => {
                        tracing::trace!("Successfully fetched details for post ID: {}", post.id);
                        posts.push(post)
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to fetch or parse a post. Skipping.");
                    }
                }
            }

            posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            tracing::info!("Crawl finished. Total posts fetched: {}", posts.len());
            Ok(posts)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use tracing_subscriber::EnvFilter;

    fn setup_tracing_for_tests() {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init();
    }

    #[tokio::test]
    async fn test_selectors_initialization() {
        setup_tracing_for_tests();
        let _selectors = Selectors::new();
        assert!(true, "Selectors::new() should complete without panic.");
        tracing::info!("Selectors initialized successfully (test_selectors_initialization).");
    }

    #[tokio::test]
    async fn test_crawl_integration() {
        setup_tracing_for_tests();
        tracing::info!("Starting test_crawl_integration...");

        let plugin = LawyerPlugin::default();
        let posts_limit = 3;

        tracing::info!("Calling crawl with posts_limit = {}", posts_limit);
        let result = plugin.crawl(posts_limit).await;

        assert!(result.is_ok(), "Crawl failed: {:?}", result.err());
        let posts = result.unwrap();

        if posts_limit > 0 {
            assert!(
                !posts.is_empty(),
                "Expected to fetch at least one post with limit {}",
                posts_limit
            );
        }
        tracing::info!("Fetched {} posts.", posts.len());

        for (i, post) in posts.iter().enumerate().take(1) {
            tracing::info!(
                "Inspecting post #{}: ID={}, Title='{}'",
                i,
                post.id,
                post.title
            );
            assert!(!post.id.is_empty(), "Post ID is empty");
            assert!(
                post.url.starts_with("http://lawyer.ssu.ac.kr"),
                "Post URL is invalid: {}",
                post.url
            );
            assert!(!post.title.is_empty(), "Post title is empty");

            let current_year = OffsetDateTime::now_utc().year();
            assert!(
                post.created_at.year() >= 2020 && post.created_at.year() <= current_year,
                "Post year seems incorrect: {}",
                post.created_at.year()
            );

            assert!(
                !post.content.is_empty(),
                "Post content is empty for post ID {}",
                post.id
            );
            tracing::info!(
                "Post content (first 100 chars for ID {}): '{}'",
                post.id,
                post.content.chars().take(100).collect::<String>()
            );
        }
        tracing::info!("test_crawl_integration finished successfully.");
    }
}
