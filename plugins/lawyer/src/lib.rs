use std::collections::HashMap;

// Removed: use std::future::Future;
// Removed: use std::pin::Pin;
use futures::{StreamExt, stream::FuturesOrdered}; // Removed FutureExt
use scraper::{Html, Selector}; // Added back missing import
// Removed: use thiserror::Error;
use time::{Date, macros::offset};

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
            post_item: Selector::parse("div.board-list-body > div.col:not(.noti)")
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
            detail_attachments: Selector::parse("div.board-attach file")
                .expect("Failed to parse detail_attachments selector"),
        }
    }
}

pub struct LawyerPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl LawyerPlugin {
    pub fn new() -> Self {
        Self::default()
    }
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
    title: String,
    date_str: String,
}

impl PartialEq for PostMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl SsufidPlugin for LawyerPlugin {
    const IDENTIFIER: &'static str = "lawyer.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 국제법무학과";
    const DESCRIPTION: &'static str = "숭실대학교 국제법무학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_list.do";

    // Applied clippy's suggestion for needless_lifetimes (removed 'a)
    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
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
                        tracing::warn!(
                            "Failed to fetch metadata from page {}: {:?}. Assuming end of posts.",
                            current_page_num,
                            e
                        );
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

            all_posts_metadata.dedup();

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
                    tracing::debug!("Successfully fetched details for post ID: {}", post.id);
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
    }
}

impl LawyerPlugin {
    const POST_VIEW_URL: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_view.do";
    const DATE_FORMAT_STR: &'static str = "[year]-[month]-[day]";

    async fn request_page(&self, page_no: u32) -> Result<reqwest::Response, PluginError> {
        let mut params = HashMap::new();
        params.insert("stype".to_string(), "".to_string());
        params.insert("stxt".to_string(), "".to_string());
        params.insert("pdsid".to_string(), "".to_string());
        params.insert("menuid".to_string(), "1003".to_string());
        params.insert("pageno".to_string(), page_no.to_string());
        tracing::debug!(page = page_no, "Fetching notice list page for metadata");
        self.http_client
            .post(Self::BASE_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to send request for page {}", page_no);
                PluginError::request::<Self>(e.to_string())
            })
    }

    async fn request_post(&self, pdsid: String) -> Result<reqwest::Response, PluginError> {
        tracing::debug!(post = &pdsid, "Fetching notice post");
        let mut params = HashMap::new();
        params.insert("stype".to_string(), "".to_string());
        params.insert("stxt".to_string(), "".to_string());
        params.insert("pdsid".to_string(), pdsid.clone());
        params.insert("menuid".to_string(), "1003".to_string());
        params.insert("pageno".to_string(), "1".to_string());
        self.http_client
            .post(Self::POST_VIEW_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to send request for post {}", pdsid);
                PluginError::request::<Self>(e.to_string())
            })
    }

    async fn fetch_page_posts_metadata(
        &self,
        page_num: u32,
    ) -> Result<Vec<PostMetadata>, PluginError> {
        let response = self.request_page(page_num).await?;

        if !response.status().is_success() {
            tracing::error!(
                "Failed to fetch list page: HTTP Status {}",
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
                        PluginError::parse::<Self>(format!("Failed to find title for post ID {id}"))
                    })?;
                let title = title_element.text().collect::<String>().trim().to_string();

                let date_str = element
                    .select(&self.selectors.post_date)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                tracing::trace!(
                    id,
                    title,
                    date_str,
                    "Found post metadata on page {}",
                    page_num,
                );

                Ok(PostMetadata {
                    id,
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
        tracing::debug!(post_id = %metadata.id, "Fetching post details");
        let response = self.request_post(metadata.id.clone()).await?;
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
            .assume_offset(offset!(+9));

        let content_element = document.select(&self.selectors.detail_content).next();
        let content = content_element
            .ok_or(PluginError::parse::<Self>(format!(
                "Failed to find content element for post ID {} - {}",
                metadata.id, metadata.title
            )))?
            .inner_html();
        if content.trim().is_empty() && content_element.is_some() {
            tracing::warn!(post_id = %metadata.id, "Parsed content is empty or whitespace only.");
        }

        let attachments = document
            .select(&self.selectors.detail_attachments)
            .map(|el| {
                let name = el.text().collect::<String>().trim().to_string();
                Attachment {
                    name: Some(name),
                    url: Self::BASE_URL.to_string(), // NOTE: No valid URL for individual attachments; every request is a POST
                    mime_type: None,                 // Content type is not provided in the HTML
                }
            })
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: Self::BASE_URL.to_string(), // NOTE: No valid URL for individual posts; every request is a POST
            title,
            author: None,
            description: None,
            category: vec![],
            created_at,
            updated_at: None,
            thumbnail: None,
            content,
            attachments,
            metadata: None,
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
                "Expected to fetch at least one post with limit {posts_limit}"
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
