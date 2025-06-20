#![allow(dead_code)]
use encoding_rs::EUC_KR;
use futures::TryStreamExt as _;
use futures::stream::FuturesOrdered;
use scraper::{Html, Selector};
use ssufid::core::{SsufidPlugin, SsufidPost};
use ssufid::error::PluginError;
use thiserror::Error;
use time::format_description::BorrowedFormatItem;
use time::macros::offset;
use time::{PrimitiveDateTime, macros::format_description};

struct Selectors {
    list_item_selector: Selector,
    title_in_list_selector: Selector,
    date_in_list_selector: Selector,

    // Post page selectors
    title_selector: Selector,
    metadata_selector: Selector,
    content_selector: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // List page selectors
            list_item_selector: Selector::parse(
                "table[bordercolor=\"#CCCCCC\"][frame=\"hsides\"] > tbody > tr",
            )
            .unwrap(),
            title_in_list_selector: Selector::parse(
                "td:nth-child(2) > a[href*='javascript:viewContent' i]",
            )
            .unwrap(),
            date_in_list_selector: Selector::parse("td:nth-child(5)").unwrap(),

            // Post page selectors
            title_selector: Selector::parse("td[bgcolor=\"#edf8fc\"]").unwrap(),
            metadata_selector: Selector::parse("td[height=\"38\"] > table > tbody > tr > td")
                .unwrap(),
            content_selector: Selector::parse("td.descript").unwrap(),
        }
    }
}

#[derive(Debug, Error)]
enum SsuDormError {
    #[error("Failed to parse post ID from onclick attribute: {0}")]
    PostIdParse(String),
    #[error("Post ID not found in onclick attribute: {0}")]
    PostIdNotFound(String),
    #[error("Title not found for post: {0}")]
    TitleNotFound(String),
    #[error("Content not found for post: {0}")]
    ContentNotFound(String),
    #[error("Date string parsing error: {0}")]
    DateParse(String),
    #[error("Author/Date string parsing error: {0}")]
    AuthorDateStringParse(String),
}

// Removed unused: use std::error::Error as StdError;

impl From<SsuDormError> for PluginError {
    fn from(error: SsuDormError) -> Self {
        let error_name = match error {
            SsuDormError::PostIdParse(_) => "PostIdParse",
            SsuDormError::PostIdNotFound(_) => "PostIdNotFound",
            SsuDormError::TitleNotFound(_) => "TitleNotFound",
            SsuDormError::ContentNotFound(_) => "ContentNotFound",
            SsuDormError::DateParse(_) => "DateParse",
            SsuDormError::AuthorDateStringParse(_) => "AuthorDateStringParse",
        };
        PluginError::custom::<SsuDormPlugin>(
            error_name.to_string(), // This will be the 'name' in PluginErrorKind::Custom
            error.to_string(),      // This will be the detailed message
        )
    }
}

pub struct SsuDormPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl SsuDormPlugin {
    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::new(),
        }
    }

    const LIST_PAGE_URL: &'static str = "https://ssudorm.ssu.ac.kr:444/SShostel/mall_main.php?viewform=B0001_noticeboard_list&board_no=1";
    const POST_VIEW_URL_BASE: &'static str = "https://ssudorm.ssu.ac.kr:444/SShostel/mall_main.php?viewform=B0001_noticeboard_view&board_no=1";

    const DATETIME_FORMAT: &[BorrowedFormatItem<'_>] =
        format_description!("[year]-[month]-[day] [hour]:[minute]");

    // Function to decode EUC-KR bytes to String
    fn decode_euc_kr(bytes: &[u8]) -> String {
        EUC_KR.decode(bytes).0.into_owned()
    }

    async fn fetch_html_content(&self, url: &str) -> Result<String, PluginError> {
        let response_bytes = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        Ok(Self::decode_euc_kr(&response_bytes))
    }

    async fn fetch_page_posts_metadata(
        &self,
        page: u32, // page is 1-indexed
    ) -> Result<Vec<SsuDormPostMetadata>, PluginError> {
        // Dormitory website uses 'next' for pagination, which seems to be an offset (multiples of 15)
        // page 1: next=0 (or not present)
        // page 2: next=15
        // page 3: next=30
        let offset = if page > 0 { (page - 1) * 15 } else { 0 }; // Ensure page is positive
        let page_url = format!("{}&next={}", Self::LIST_PAGE_URL, offset);

        tracing::info!("Fetching metadata from URL: {}", page_url);

        let html_content = self.fetch_html_content(&page_url).await?;
        let document = Html::parse_document(&html_content);
        let mut metadata_list = Vec::new();
        tracing::debug!("Using list_item_selector for actual post rows.");

        let post_rows = document
            .select(&self.selectors.list_item_selector)
            .collect::<Vec<_>>();

        for row_element in post_rows.into_iter().skip(2) {
            // The title_in_list_selector is now more specific: "td:nth-child(2) > a[onclick*='viewContent' i]"
            // It's applied to the current row_element.
            let title_element_opt = row_element
                .select(&self.selectors.title_in_list_selector)
                .next();

            if let Some(title_element) = title_element_opt {
                // Now we are looking at the 'href' attribute
                let href_attr = title_element.value().attr("href").unwrap_or_default();
                // Parts are extracted from the href attribute now
                let parts: Vec<&str> = href_attr.split(['\'', ',']).collect();
                let id = if parts.len() > 2 {
                    parts[parts.len() - 2].to_string()
                } else {
                    tracing::warn!(
                        "Could not parse ID from href parts: {:?} (original: {})",
                        parts,
                        href_attr
                    );
                    continue;
                };

                if id.is_empty() || id == "null" {
                    tracing::warn!("Empty or null ID parsed: {} from href: {}", id, href_attr);
                    continue;
                }

                let post_url = format!("{}&idx={}", Self::POST_VIEW_URL_BASE, id);
                let title = title_element.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    tracing::warn!("Empty title for ID {}: {}", id, title_element.html());
                    // Potentially skip if title is mandatory, or use a placeholder
                }

                // Date is likely in a sibling td of the title_element's parent td, or a td in the same row_element
                // The current date_in_list_selector assumes a fixed position (e.g., 5th td in the row)
                let date_str = row_element
                    .select(&self.selectors.date_in_list_selector)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_else(|| {
                        tracing::warn!("Date not found for ID {} in URL {}", id, post_url);
                        String::new() // Default to empty string if not found
                    });
                tracing::info!(
                    "Successfully extracted metadata: ID={}, Title='{}', Date='{}', URL='{}'",
                    id,
                    title,
                    date_str,
                    post_url
                );
                metadata_list.push(SsuDormPostMetadata {
                    id,
                    url: post_url,
                    title_from_list: title,
                    date_str_from_list: date_str,
                });
            }
        }
        Ok(metadata_list)
    }

    async fn all_posts_metadata(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<SsuDormPostMetadata>, PluginError> {
        tracing::info!("Fetching all posts metadata with limit: {}", posts_limit);
        let mut all_metadata = Vec::new();
        let mut current_page = 1;
        const MAX_PAGES_TO_TRY: u32 = 50; // Safety break for pagination

        while all_metadata.len() < posts_limit as usize && current_page <= MAX_PAGES_TO_TRY {
            tracing::debug!("Fetching metadata for page: {}", current_page);
            let metadata_list = self.fetch_page_posts_metadata(current_page).await?;
            if metadata_list.is_empty() {
                tracing::info!("No more metadata found on page {}. Stopping.", current_page);
                break; // No more posts found on this page
            }
            all_metadata.extend(metadata_list);
            current_page += 1;
        }

        if all_metadata.len() > posts_limit as usize {
            all_metadata.truncate(posts_limit as usize); // Ensure we don't exceed the limit
        }
        Ok(all_metadata)
    }

    async fn fetch_post_data(
        &self,
        metadata: SsuDormPostMetadata,
    ) -> Result<SsufidPost, PluginError> {
        tracing::debug!("Fetching post data for URL: {}", metadata.url);
        let html_content = self.fetch_html_content(&metadata.url).await?;
        let document = Html::parse_document(&html_content);

        let title = document
            .select(&self.selectors.title_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .ok_or_else(|| SsuDormError::TitleNotFound(metadata.url.clone()))?;

        let mut metadata_elements = document.select(&self.selectors.metadata_selector);

        let author_str = metadata_elements
            .next()
            .and_then(|el| el.text().next().and_then(|s| s.split(':').nth(1)))
            .ok_or_else(|| SsuDormError::AuthorDateStringParse(metadata.url.clone()))?
            .trim()
            .to_string();

        let date_str = metadata_elements
            .nth(1)
            .and_then(|el| {
                el.text()
                    .next()
                    .and_then(|s| s.split_once(':').map(|(_, v)| v))
            })
            .ok_or_else(|| SsuDormError::AuthorDateStringParse(metadata.url.clone()))?
            .trim()
            .to_string();

        let created_at = PrimitiveDateTime::parse(&date_str, Self::DATETIME_FORMAT)
            .map_err(|_| SsuDormError::DateParse(date_str.clone()))?
            .assume_offset(offset!(+9));

        let content_element = document
            .select(&self.selectors.content_selector)
            .next()
            .ok_or_else(|| SsuDormError::ContentNotFound(metadata.url.clone()))?;
        let content = content_element.html(); // Get inner HTML to preserve formatting

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            author: Some(author_str),
            title,
            description: None,
            category: vec![],
            created_at,
            updated_at: None,
            thumbnail: None,
            content,
            attachments: vec![],
            metadata: None,
        })
    }
}

impl Default for SsuDormPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct SsuDormPostMetadata {
    id: String,
    url: String,
    title_from_list: String,
    date_str_from_list: String,
}

impl SsufidPlugin for SsuDormPlugin {
    const IDENTIFIER: &'static str = "ssudorm.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 기숙사";
    const DESCRIPTION: &'static str = "숭실대학교 기숙사 홈페이지의 공지사항을 제공합니다.";
    // Base URL for resolving relative links if necessary
    const BASE_URL: &'static str = "https://ssudorm.ssu.ac.kr:444/SShostel/mall_main.php?viewform=B0001_noticeboard_list&board_no=1";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        tracing::info!(message = "Crawling started", posts_limit);
        self.all_posts_metadata(posts_limit)
            .await?
            .into_iter()
            .map(|metadata| {
                tracing::debug!("Fetching post data for metadata: {:?}", metadata);
                self.fetch_post_data(metadata)
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| {
                PluginError::custom::<SsuDormPlugin>(
                    "CrawlError".to_string(),
                    format!("Failed to fetch all posts: {}", e),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to initialize logging for tests
    fn setup_tracing() {
        // Using `try_init` to avoid panic if already initialized
        let _ = tracing_subscriber::fmt::try_init();
    }

    #[tokio::test]
    async fn test_fetch_page_posts_metadata_first_page() {
        setup_tracing();
        let plugin = SsuDormPlugin::default();
        let metadata = plugin.fetch_page_posts_metadata(1).await.unwrap();
        assert!(
            !metadata.is_empty(),
            "Should fetch some metadata from the first page."
        );
        // Add more assertions for specific metadata fields if possible
        let first_meta = &metadata[0];
        assert!(!first_meta.id.is_empty());
        assert!(first_meta.url.contains(&first_meta.id));
        assert!(!first_meta.title_from_list.is_empty());
        assert!(!first_meta.date_str_from_list.is_empty());
        tracing::info!("First metadata item: {:?}", first_meta);
    }

    #[tokio::test]
    async fn test_fetch_one_post() {
        setup_tracing();
        let plugin = SsuDormPlugin::default();
        // Fetch metadata for the first page first to get a valid post to test
        let metadata_list = plugin.fetch_page_posts_metadata(1).await.unwrap();
        assert!(
            !metadata_list.is_empty(),
            "Need metadata to test fetching a post."
        );

        // Try to fetch the first post from the list
        let first_metadata = &metadata_list[0];
        tracing::info!(
            "Attempting to fetch post with metadata: {:?}",
            first_metadata
        );

        let post = plugin.fetch_post_data(first_metadata).await;
        match &post {
            Ok(p) => tracing::info!(
                "Fetched post: ID={}, Title='{}', Author='{:?}', Date='{}', Content exists: {}",
                p.id,
                p.title,
                p.author,
                p.created_at,
                !p.content.is_empty()
            ),
            Err(e) => tracing::error!("Failed to fetch post: {:?}", e),
        }
        assert!(post.is_ok(), "Should be able to fetch and parse a post.");
        let unwrapped_post = post.unwrap();
        assert_eq!(unwrapped_post.id, first_metadata.id);
        assert!(!unwrapped_post.title.is_empty());
        assert!(unwrapped_post.author.is_some());
        assert!(!unwrapped_post.content.is_empty());
        // Check date is somewhat reasonable (e.g. year > 2000)
        assert!(unwrapped_post.created_at.year() > 2000);
    }

    #[tokio::test]
    async fn test_crawl_limited() {
        setup_tracing();
        let plugin = SsuDormPlugin::default();
        let limit = 5;
        let posts = plugin.crawl(limit).await.unwrap();
        assert_eq!(
            posts.len() as u32,
            limit,
            "Should fetch exactly 'limit' posts if available, or fewer if not enough total posts exist."
        );
        tracing::info!("Fetched {} posts with limit {}", posts.len(), limit);
        for post in posts.iter().take(3) {
            // Log details of a few posts
            tracing::info!(
                "Post details: ID={}, Title='{}', Date='{}'",
                post.id,
                post.title,
                post.created_at
            );
        }
    }

    #[tokio::test]
    async fn test_crawl_more_than_one_page() {
        setup_tracing();
        let plugin = SsuDormPlugin::default();
        let limit = 20; // Assuming there are more than 15 posts (typical page size)
        let posts = plugin.crawl(limit).await.unwrap();
        assert!(posts.len() <= limit as usize);
        // This assertion is tricky: if total posts are < limit, it might not fetch more than one page.
        // A better check is if posts.len() > default_page_size (15) if limit > 15 and total posts allow.
        // For now, we check if it fetched up to the limit.
        assert_eq!(
            posts.len(),
            limit as usize,
            "Should fetch 'limit' posts if that many are available across pages. Fetched: {}",
            posts.len()
        );
        tracing::info!("Fetched {} posts with limit {}", posts.len(), limit);
    }
}
