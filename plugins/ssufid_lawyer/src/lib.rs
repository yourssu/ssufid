use ssufid::core::{Attachment, SsufidPlugin, SsufidPost}; // MODIFIED
use ssufid::PluginError; // MODIFIED (removed PluginErrorKind)
                         // use reqwest; // REMOVED
use futures::{stream::FuturesOrdered, StreamExt};
use scraper::{Html, Selector, Element}; // ADDED Element trait
use thiserror::Error;
use time::{format_description::well_known::Iso8601, Date, OffsetDateTime}; // MODIFIED (removed macros::offset)
use url::Url;

// Define a custom error type for this plugin
#[derive(Debug, Error)]
pub enum LawyerPluginError {
    #[error("Network request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("URL parsing failed: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("HTML parsing error: {0}")] // Added detail
    HtmlParsingError(String),
    #[error("Selector error: {0}")]
    SelectorError(String),
    #[error("Date parsing failed: {0}")]
    DateParseError(#[from] time::error::Parse),
    #[error("Mandatory field missing: {0}")]
    MissingFieldError(String),
}

// Implement conversion to PluginError
impl From<LawyerPluginError> for PluginError {
    fn from(err: LawyerPluginError) -> Self {
        // Using custom as per compiler notes from previous turn, assuming PluginErrorKind::Internal maps to a general custom error.
        // If PluginError had a more direct constructor like `new(plugin_id, kind, source)`, that would be used.
        PluginError::custom::<LawyerPlugin>(LawyerPlugin::IDENTIFIER.to_string(), err.to_string())
        // Explicit type for T
    }
}

// Struct to hold compiled selectors
struct Selectors {
    // List page selectors
    post_row_selector: Selector,
    title_link_selector: Selector,
    date_selector: Selector,

    // Post page selectors
    post_title_selector: Selector,
    post_author_selector: Selector, // Might not be used if author isn't available
    post_content_selector: Selector,
    post_attachment_selector: Selector,
}

impl Selectors {
    fn new() -> Result<Self, LawyerPluginError> {
        Ok(Self {
            // List page selectors
            post_row_selector: Selector::parse("div") // Attempt 2: Broadened for discovery
                .map_err(|e| {
                    LawyerPluginError::SelectorError(format!("post_row_selector: {}", e))
                })?,
            title_link_selector: Selector::parse("p.b-title > a")
                .map_err(|e| {
                LawyerPluginError::SelectorError(format!("title_link_selector: {}", e))
            })?,
            date_selector: Selector::parse("p.b-date")
                .map_err(|e| LawyerPluginError::SelectorError(format!("date_selector: {}", e)))?,

            // Post page selectors
            post_title_selector: Selector::parse("dl.board_view_info_title > dd")
                .map_err(|e| {
                    LawyerPluginError::SelectorError(format!("post_title_selector: {}", e))
                })?,
            post_author_selector: Selector::parse(
                "ul.board_view_info_user > li:nth-child(1) > dl > dd",
            ) // Author: <ul class=\"board_view_info_user\"><li><dl><dt>작성자</dt><dd>법학과</dd></dl></li>...</ul> - This site seems to use \"법학과\" (Department of Law) as author
            .map_err(|e| {
                LawyerPluginError::SelectorError(format!("post_author_selector: {}", e))
            })?,
            post_content_selector: Selector::parse("div#board_content").map_err(|e| {
                LawyerPluginError::SelectorError(format!("post_content_selector: {}", e))
            })?,
            post_attachment_selector: Selector::parse("div.board_view_file > ul > li > a") // Attachments: <div class=\"board_view_file\"><ul><li><a href=\"url\" title=\"Download\"><img src=\"/img/board/icn_file.png\" alt=\"첨부파일\">파일명.hwp (55.5KB)</a></li></ul></div>
                .map_err(|e| {
                    LawyerPluginError::SelectorError(format!("post_attachment_selector: {}", e))
                })?,
        })
    }
}

// Metadata struct (remains the same)
#[derive(Debug, Clone)] // Added Clone
struct LawyerPostMetadata {
    id: String,
    url: String,
    title: String,
    date_str: String,
}

pub struct LawyerPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl Default for LawyerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LawyerPlugin {
    pub fn new() -> Self {
        Self {
            selectors: Selectors::new().expect("Failed to compile selectors"),
            http_client: reqwest::Client::new(),
        }
    }

    async fn fetch_page_posts_metadata(
        &self,
        page_num: u32,
    ) -> Result<Vec<LawyerPostMetadata>, LawyerPluginError> {
        let page_url_str = format!("{}?currPage={}", LawyerPlugin::BASE_URL, page_num);
        let page_url = Url::parse(&page_url_str)?;
        // Using println! for diagnostics as tracing subscriber might not be active in tests
        println!("DEBUG: Fetching metadata page: {}", page_url);

        let res = self.http_client.get(page_url.clone()).send().await?;
        if !res.status().is_success() {
            println!("ERROR: Request to {} failed with status: {}", page_url, res.status());
            return Err(LawyerPluginError::RequestError(
                res.error_for_status().unwrap_err(),
            ));
        }
        let html_content = res.text().await?;
        println!("DEBUG: HTML content length for page {}: {}", page_num, html_content.len());
        let document = Html::parse_document(&html_content);
        let mut posts_metadata = Vec::new();

        for row_element in document.select(&self.selectors.post_row_selector) {
            // DEBUG_DIV_DISCOVERY logic
            let current_id = row_element.value().attr("id");
            let current_class = row_element.value().attr("class");
            let current_text = row_element.text().collect::<String>();
            let current_html = row_element.html();

            println!("DEBUG_DIV_DISCOVERY:                 ID: {:?},                 Class: {:?},                 Text (first 50 chars): {:.50},                 HTML (first 100 chars): {:.100}",
                current_id,
                current_class,
                current_text.trim(),
                current_html.trim()
            );

            if row_element.select(&self.selectors.title_link_selector).next().is_some() {
                println!("DEBUG_DIV_DISCOVERY: Found title link pattern ('p.b-title > a') within this div!");
            }
            if row_element.select(&self.selectors.date_selector).next().is_some() {
                println!("DEBUG_DIV_DISCOVERY: Found date pattern ('p.b-date') within this div!");
            }

            if let Some(parent) = row_element.parent_element() {
                println!("DEBUG_DIV_DISCOVERY_PARENT:                     Parent Tag: {},                     Parent ID: {:?},                     Parent Class: {:?}",
                    parent.value().name(),
                    parent.value().attr("id"),
                    parent.value().attr("class")
                );
            }
            println!("---"); // Separator

            // Original logic adapted for discovery context (primarily for when selector is more specific)
            // This part will be very noisy with "div" but useful if we were closer.
            // For "div" selector, we rely on the print statements above to find the right structure.
            if let Some(post_id_str) = current_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
                 println!("DEBUG: Potential row with ID: [{}]", post_id_str);
                let title_link_element_opt = row_element.select(&self.selectors.title_link_selector).next();
                let date_element_opt = row_element.select(&self.selectors.date_selector).next();

                let title_opt = title_link_element_opt.map(|el| el.text().collect::<String>().trim().to_string()).filter(|s| !s.is_empty());
                let date_str_opt = date_element_opt.map(|el| el.text().collect::<String>().trim().to_string()).filter(|s| !s.is_empty());

                if let (Some(title), Some(date_str)) = (title_opt, date_str_opt) {
                    println!("INFO: Extracted (potentially valid) post_id: [{}], title: [{}], date_str: [{}]", post_id_str, title, date_str);
                    let view_url_base = "http://lawyer.ssu.ac.kr/web/05/notice_view.do";
                    let post_url_str = format!("{}?board_seq={}", view_url_base, post_id_str);
                    match Url::parse(&post_url_str) {
                        Ok(parsed_post_url) => {
                            println!("DEBUG: Pushing metadata: id={}, url={}", post_id_str, parsed_post_url.to_string());
                            posts_metadata.push(LawyerPostMetadata {
                                id: post_id_str,
                                url: parsed_post_url.to_string(),
                                title,
                                date_str,
                            });
                        }
                        Err(parse_err) => {
                            println!("WARN: Failed to parse constructed URL [{}] for post_id [{}]: {}", post_url_str, post_id_str, parse_err);
                        }
                    }
                } else {
                     // This will be noisy if the main selector is just "div"
                     // println!("WARN: Could not extract title or date for div with ID [{}]. Title opt: {:?}, Date opt: {:?}", post_id_str, title_link_element_opt.map(|e|e.html()), date_element_opt.map(|e|e.html()));
                }
            }
        }
        Ok(posts_metadata)
    }

    const POSTS_PER_PAGE: u32 = 15;

    async fn fetch_post(&self, meta: &LawyerPostMetadata) -> Result<SsufidPost, LawyerPluginError> {
        // Reverting tracing to println! for consistency in this debugging phase
        println!("DEBUG: Fetching post content for URL: {}", meta.url);
        let post_url = Url::parse(&meta.url)?;

        let res = self.http_client.get(post_url.clone()).send().await?;
        if !res.status().is_success() {
            println!("ERROR: Request to {} failed with status: {}", meta.url, res.status());
            return Err(LawyerPluginError::RequestError(
                res.error_for_status().unwrap_err(),
            ));
        }
        let html_content = res.text().await?;
        let document = Html::parse_document(&html_content);

        let title = document
            .select(&self.selectors.post_title_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| meta.title.clone()); // Fallback to metadata title

        let author = document
            .select(&self.selectors.post_author_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        // The author for this site seems to consistently be "법학과" (Department of Law)
        // If author field is empty or "법학과", we can set it to a more generic "숭실대학교 국제법무학과" or leave as is.
        // For now, we'll use what's scraped or None.
        let final_author = author
            .filter(|s| !s.is_empty() && s != "법학과")
            .or_else(|| Some(LawyerPlugin::TITLE.to_string())); // MODIFIED Self::TITLE

        let content_html = document
            .select(&self.selectors.post_content_selector)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let mut attachments = Vec::new();
        for attachment_element in document.select(&self.selectors.post_attachment_selector) {
            if let Some(relative_url) = attachment_element.value().attr("href") {
                // Attachment URLs might be relative to the domain or a specific path
                // Example: /board/download.do?board_seq=...&file_seq=...
                // Need to join with a proper base. The post_url's domain should be fine.
                let attachment_base_url = Url::parse(&format!(
                    "{}://{}",
                    post_url.scheme(),
                    post_url.host_str().unwrap_or_default()
                ))
                .unwrap_or_else(|_| Url::parse(LawyerPlugin::BASE_URL).unwrap()); // MODIFIED Self::BASE_URL, Fallback to plugin's BASE_URL domain

                match attachment_base_url.join(relative_url) {
                    Ok(full_url) => {
                        // Text content of the link often includes file name and size, e.g., "파일명.hwp (55.5KB)"
                        // We need to extract just the file name.
                        let full_text = attachment_element
                            .text()
                            .collect::<String>()
                            .trim()
                            .to_string();
                        let name = full_text.split('(').next().unwrap_or("").trim().to_string();

                        if !name.is_empty() {
                            attachments.push(Attachment {
                                name: Some(name),
                                url: full_url.to_string(),
                                mime_type: None,
                            });
                        } else {
                            println!("WARN: Empty attachment name for URL: {}", full_url);
                        }
                    }
                    Err(e) => {
                        println!("ERROR: Failed to form absolute attachment URL from {}: {}", relative_url, e);
                    }
                }
            }
        }

        let created_at = Date::parse(&meta.date_str, &Iso8601::DATE)
            .map(|d| OffsetDateTime::new_utc(d, time::Time::MIDNIGHT))
            .map_err(LawyerPluginError::DateParseError)?;

        Ok(SsufidPost {
            id: meta.id.clone(),
            url: meta.url.clone(),
            title,
            content: content_html,
            author: final_author,
            created_at,
            updated_at: None,
            attachments,
            description: None,    // ADDED
            category: Vec::new(), // ADDED
            thumbnail: None,      // ADDED
            metadata: None,       // ADDED
        })
    }

    // const POSTS_PER_PAGE: u32 = 15; // MOVED and value changed to 15
    // This was already moved in the previous turn's successful patch.
    // It's correctly an associated const of LawyerPlugin now.
}

impl SsufidPlugin for LawyerPlugin {
    // #[async_trait::async_trait] is already removed, ensuring it stays that way.
    const IDENTIFIER: &'static str = "lawyer.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 국제법무학과"; // Used as default author if specific not found
    const DESCRIPTION: &'static str = "숭실대학교 국제법무학과 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://lawyer.ssu.ac.kr/web/05/notice_list.do";
    // POSTS_PER_PAGE moved to impl LawyerPlugin

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        // Changed back to async fn
        // Removed Box::pin(async move { ... }) wrapper
        println!("INFO: Starting crawl for {posts_limit} posts from {}", LawyerPlugin::BASE_URL);

        let mut metadata_futures = FuturesOrdered::new();
        // LawyerPlugin::POSTS_PER_PAGE is already an associated const of LawyerPlugin struct
        let num_pages_to_crawl =
            (posts_limit as f32 / LawyerPlugin::POSTS_PER_PAGE as f32).ceil() as u32;
        println!("DEBUG: Calculated pages to crawl for metadata: {}", num_pages_to_crawl);

        for page_num in 1..=num_pages_to_crawl {
            metadata_futures.push_back(self.fetch_page_posts_metadata(page_num));
        }

        let mut all_metadata = Vec::new();
        while let Some(result) = metadata_futures.next().await {
            match result {
                Ok(page_metadata) => {
                    if page_metadata.is_empty()
                        && all_metadata.len() >= LawyerPlugin::POSTS_PER_PAGE as usize
                    {
                        println!("DEBUG: Page {} returned no metadata, assuming end of posts.", all_metadata.len() / LawyerPlugin::POSTS_PER_PAGE as usize + 1);
                        break;
                    }
                    all_metadata.extend(page_metadata);
                }
                Err(e) => {
                    println!("ERROR: Error fetching page metadata: {:?}", e)
                }
            }
        }

        all_metadata.truncate(posts_limit as usize);
        println!("INFO: Fetched {} metadata items.", all_metadata.len());

        let mut post_futures = FuturesOrdered::new();
        for meta in &all_metadata {
            post_futures.push_back(self.fetch_post(meta));
        }

        let mut ssufid_posts = Vec::new();
        while let Some(result) = post_futures.next().await {
            match result {
                Ok(post) => ssufid_posts.push(post),
                Err(e) => {
                    println!("ERROR: Error fetching post content: {:?}", e)
                }
            }
        }

        println!("INFO: Successfully fetched {} posts.", ssufid_posts.len());
        Ok(ssufid_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Initialize tracing for tests
    fn init_trace() {
        let _ = tracing_subscriber::fmt::try_init();
    }

    // tokio should be available as a workspace dev-dependency or direct dependency
    // If not, it might need to be added to ssufid_lawyer's Cargo.toml as a dev-dependency.

    #[tokio::test]
    async fn test_fetch_page_posts_metadata_not_empty() {
        init_trace(); // Call tracing init
        let plugin = LawyerPlugin::default();
        let metadata_result = plugin.fetch_page_posts_metadata(1).await;
        assert!(metadata_result.is_ok(), "fetch_page_posts_metadata failed: {:?}", metadata_result.err());
        let metadata = metadata_result.unwrap();
        assert!(!metadata.is_empty(), "Fetched metadata for page 1 should not be empty.");

        if let Some(first_meta) = metadata.get(0) {
            assert!(!first_meta.id.is_empty(), "Post ID should not be empty");
            assert!(first_meta.url.starts_with("http"), "Post URL should be absolute, got: {}", first_meta.url);
            assert!(!first_meta.title.is_empty(), "Post title should not be empty");
            assert!(first_meta.date_str.matches(r"^\d{4}\.\d{2}\.\d{2}$").next().is_some(), "Date string should be YYYY.MM.DD, got: {}", first_meta.date_str); // MODIFIED .is_some() to .next().is_some()
        }
    }

    #[tokio::test]
    async fn test_fetch_one_post() {
        init_trace(); // Call tracing init
        let plugin = LawyerPlugin::default();
        let metadata_list_result = plugin.fetch_page_posts_metadata(1).await;
        assert!(metadata_list_result.is_ok(), "Failed to fetch metadata for test_fetch_one_post: {:?}", metadata_list_result.err());
        let metadata_list = metadata_list_result.unwrap();
        assert!(!metadata_list.is_empty(), "Metadata list is empty, cannot proceed to fetch a post.");

        let first_meta = metadata_list[0].clone();

        let post_result = plugin.fetch_post(&first_meta).await;
        assert!(post_result.is_ok(), "Failed to fetch the first post: {:?}", post_result.err());
        let post = post_result.unwrap();

        assert_eq!(post.id, first_meta.id, "Post ID mismatch");
        assert_eq!(post.url, first_meta.url, "Post URL mismatch");
        assert!(!post.title.is_empty(), "Fetched post title should not be empty");
        assert!(!post.content.is_empty(), "Fetched post content should not be empty");
        assert!(post.created_at.year() > 2000, "Year seems too old or invalid. Parsed as: {}", post.created_at);
    }

    #[test]
    fn plugin_constants_check() {
        assert_eq!(LawyerPlugin::IDENTIFIER, "lawyer.ssu.ac.kr");
        assert_eq!(LawyerPlugin::TITLE, "숭실대학교 국제법무학과");
    }
}
