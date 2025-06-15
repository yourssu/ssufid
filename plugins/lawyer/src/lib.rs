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
    const LIST_PAGE_URL: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_list.do";
    const VIEW_PAGE_URL_BASE: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_view.do";
    const DATE_FORMAT_STR: &'static str = "[year]-[month]-[day]";

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

        let attachments = document
            .select(&self.selectors.detail_attachments)
            .filter_map(|el| {
                let name = el.text().collect::<String>().trim().to_string();
                el.value().attr("href").map(|href_val| {
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
                            href_val.to_string()
                        }
                    } else {
                        Url::parse("http://lawyer.ssu.ac.kr")
                            .expect("Failed to parse base URL for attachments")
                            .join(href_val.trim())
                            .unwrap_or_else(|_| Url::parse("http://invalid.url/").unwrap())
                            .to_string()
                    };

                    Attachment {
                        name: Some(name),
                        url: attachment_url_str,
                        mime_type: None,
                    }
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
            let mut posts_collected = 0;

            tracing::debug!(
                "Starting crawl for plugin: {}, posts_limit: {}",
                Self::IDENTIFIER,
                posts_limit
            );

            loop {
                if posts_limit > 0 && posts_collected >= posts_limit {
                    tracing::debug!("Reached posts_limit ({})", posts_limit);
                    break;
                }

                let list_page_url = format!("{}?pageno={}", Self::LIST_PAGE_URL, current_page_num);
                tracing::debug!(url = %list_page_url, page = current_page_num, "Fetching notice list page");

                let response = self
                    .http_client
                    .get(&list_page_url)
                    .send()
                    .await
                    .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

                if !response.status().is_success() {
                    tracing::error!(
                        "Failed to fetch list page {}: HTTP Status {}",
                        list_page_url,
                        response.status()
                    );
                    return Err(PluginError::request::<Self>(format!(
                        "HTTP Error: {}",
                        response.status()
                    )));
                }

                let html_content = response
                    .text()
                    .await
                    .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
                let document = Html::parse_document(&html_content);

                let mut new_metadata_on_page: Vec<PostMetadata> = Vec::new();
                let items_on_page = document.select(&self.selectors.post_item);

                for element in items_on_page {
                    if posts_limit > 0 && posts_collected >= posts_limit {
                        break;
                    }

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
                            PluginError::parse::<Self>(format!(
                                "Failed to find title for post ID {}",
                                id
                            ))
                        })?;
                    let title = title_element.text().collect::<String>().trim().to_string();

                    let date_str = element
                        .select(&self.selectors.post_date)
                        .next()
                        .map(|el| el.text().collect::<String>().trim().to_string())
                        .unwrap_or_default();

                    let post_url_str = format!("{}?pdsid={}", Self::VIEW_PAGE_URL_BASE, id);
                    let url = Url::parse(&post_url_str).map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to parse post URL {}: {}",
                            post_url_str, e
                        ))
                    })?;

                    tracing::trace!(
                        "Found post metadata on list page: id={}, title='{}', date='{}'",
                        id,
                        title,
                        date_str
                    );
                    new_metadata_on_page.push(PostMetadata {
                        id,
                        url,
                        title,
                        date_str,
                    });
                    posts_collected += 1;
                }

                if new_metadata_on_page.is_empty() {
                    tracing::debug!(
                        "No more posts found on page {} or page is empty.",
                        current_page_num
                    );
                    break;
                }

                all_posts_metadata.extend(new_metadata_on_page);

                let has_next_page = document
                    .select(&Selector::parse("div.board-pagination a").unwrap())
                    .any(|link| {
                        link.text().collect::<String>().contains(">")
                            || link.value().attr("href").is_some_and(|h| {
                                h.contains(&format!("gotoPage({})", current_page_num + 1))
                            })
                    });

                if !has_next_page {
                    tracing::debug!(
                        "No 'next page' link found or inferred on page {}. Assuming it's the last page.",
                        current_page_num
                    );
                    break;
                }

                current_page_num += 1;
                tracing::trace!("Moving to page {}", current_page_num);
            }

            if posts_limit > 0 {
                all_posts_metadata.truncate(posts_limit as usize);
            }
            tracing::debug!(
                "Collected {} post metadata items. Fetching details...",
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
