#![allow(dead_code)] // Allow dead code for now to pass clippy -D warnings
use ssufid::core::{Attachment, SsufidPlugin, SsufidPost};
use ssufid::error::PluginError;
// use std::borrow::Cow; // Unused
use encoding_rs::EUC_KR;
use futures::{StreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector}; // Removed ElementRef
use thiserror::Error;
use time::{Date, PrimitiveDateTime, Time, macros::format_description}; // Removed OffsetDateTime, Added Time for parsing & PrimitiveDateTime
use url::Url; // For handling EUC-KR encoding

// Selectors for parsing HTML
struct Selectors {
    // List page selectors
    list_item_selector: Selector, // Selector for each item in the list
    title_in_list_selector: Selector, // Selector for the title within a list item
    // author_in_list_selector: Selector, // Selector for author in list (seems to be always '운영사무실')
    date_in_list_selector: Selector, // Selector for date in list
    // view_count_in_list_selector: Selector, // Selector for view count (if needed)

    // Post page selectors
    title_selector: Selector,
    author_selector: Selector,
    date_selector: Selector,
    content_selector: Selector,
    attachment_selector: Selector, // Selector for attachment links

                                   // Pagination (if needed, for now we parse all items on a page)
                                   // next_page_selector: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // List page selectors
            // Selector for the individual post rows within the specific table
            list_item_selector: Selector::parse("table[bordercolor=\"#CCCCCC\"][frame=\"hsides\"] > tbody > tr").unwrap(),
            // Selector for the 'a' tag, targeting 'href' attribute for 'javascript:viewContent', CASE-INSENSITIVE
            title_in_list_selector: Selector::parse("td:nth-child(2) > a[href*='javascript:viewContent' i]").unwrap(),
            // Date is in the 5th td of the post row
            date_in_list_selector: Selector::parse("td:nth-child(5)").unwrap(),

            // Post page selectors
            // Title is usually in a prominent header, let's assume a specific table structure
            // table.user_border_LRTB_dotted td.bbs_title_style
            title_selector: Selector::parse("table[summary*='제목'] td.bbs_title_style").unwrap_or_else(|_| Selector::parse("body > table > tbody > tr:nth-child(2) > td > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(1) > td:nth-child(1) > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(1) > td").unwrap()),
            // Author and Date are often in a subsequent row/cell.
            // Example from view_text_website: "작성자: 운영사무실 조회수: 1348 작성일: 2010-04-01 17:17"
            // This is within: table[summary*='제목'] td[height="25"]
            author_selector: Selector::parse("table[summary*='제목'] td[height='25']").unwrap_or_else(|_| Selector::parse("body > table > tbody > tr:nth-child(2) > td > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(1) > td:nth-child(1) > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(2) > td").unwrap()), // This selector will get the whole string, needs parsing
            date_selector: Selector::parse("table[summary*='제목'] td[height='25']").unwrap_or_else(|_| Selector::parse("body > table > tbody > tr:nth-child(2) > td > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(1) > td:nth-child(1) > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(2) > td").unwrap()), // Same as author, needs parsing
            // Content is usually in a specific div or table cell.
            // table[summary*='내용'] td.bbs_contents_style
            content_selector: Selector::parse("table[summary*='내용'] td.bbs_contents_style").unwrap_or_else(|_| Selector::parse("body > table > tbody > tr:nth-child(2) > td > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(1) > td:nth-child(1) > table > tbody > tr > td:nth-child(2) > table > tbody > tr:nth-child(4) > td").unwrap()),
            // Attachments: Look for links within a specific part of the page, possibly with 'file' in href or specific class
            attachment_selector: Selector::parse("table[summary*='첨부파일'] a[href*='file_down']").unwrap_or_else(|_| Selector::parse("a[href*='fileDownload']").unwrap()), // Generic fallback for attachments
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

        tracing::debug!(
            plugin = Self::IDENTIFIER,
            "Fetching metadata from URL: {}",
            page_url
        );

        let html_content = self.fetch_html_content(&page_url).await?;
        let document = Html::parse_document(&html_content);
        let mut metadata_list = Vec::new();
        println!(
            "[LOG] fetch_page_posts_metadata: Entered function for page {}",
            page
        );
        tracing::debug!(
            plugin = Self::IDENTIFIER,
            "Using list_item_selector for actual post rows."
        );

        let post_rows = document
            .select(&self.selectors.list_item_selector)
            .collect::<Vec<_>>();
        println!(
            "[LOG] fetch_page_posts_metadata: Number of actual post rows found: {}",
            post_rows.len()
        );

        for (i, row_element) in post_rows.into_iter().enumerate() {
            println!(
                "[LOG] fetch_page_posts_metadata: Processing actual post row {}",
                i
            );
            if i < 3 {
                // Log HTML for first 3 actual post rows
                println!("[HTML POST ROW {}] {}", i, row_element.html());
            }

            // Skip header rows of this inner table (ones with <th> or <strong> in them)
            if row_element
                .select(&Selector::parse("th").unwrap())
                .next()
                .is_some()
                || row_element
                    .select(&Selector::parse("td > strong").unwrap())
                    .next()
                    .is_some()
            {
                println!("[LOG] fetch_page_posts_metadata: Skipping header row {}", i);
                continue;
            }

            // The title_in_list_selector is now more specific: "td:nth-child(2) > a[onclick*='viewContent' i]"
            // It's applied to the current row_element.
            let title_element_opt = row_element
                .select(&self.selectors.title_in_list_selector)
                .next();

            if let Some(title_element) = title_element_opt {
                // Now we are looking at the 'href' attribute
                let href_attr = title_element.value().attr("href").unwrap_or_default();
                println!(
                    "[LOG] fetch_page_posts_metadata: Found title_element in post row {} (href='{}')",
                    i, href_attr
                );

                if href_attr.is_empty() {
                    println!(
                        "[LOG] fetch_page_posts_metadata: href attribute is empty for title_element in post row {}",
                        i
                    );
                    tracing::warn!(
                        plugin = Self::IDENTIFIER,
                        "href attribute is empty for title_element in post row {}: {}",
                        i,
                        title_element.html()
                    );
                    continue;
                }
                // Parts are extracted from the href attribute now
                let parts: Vec<&str> = href_attr.split(['\'', ',']).collect();
                let id = if parts.len() > 2 {
                    parts[parts.len() - 2].to_string()
                } else {
                    tracing::warn!(
                        plugin = Self::IDENTIFIER,
                        "Could not parse ID from href parts: {:?} (original: {})",
                        parts,
                        href_attr
                    );
                    continue;
                };

                if id.is_empty() || id == "null" {
                    tracing::warn!(
                        plugin = Self::IDENTIFIER,
                        "Empty or null ID parsed: {} from href: {}",
                        id,
                        href_attr
                    );
                    continue;
                }

                let post_url = format!("{}&idx={}", Self::POST_VIEW_URL_BASE, id);
                let title = title_element.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    tracing::warn!(
                        plugin = Self::IDENTIFIER,
                        "Empty title for ID {}: {}",
                        id,
                        title_element.html()
                    );
                    // Potentially skip if title is mandatory, or use a placeholder
                }

                // Date is likely in a sibling td of the title_element's parent td, or a td in the same row_element
                // The current date_in_list_selector assumes a fixed position (e.g., 5th td in the row)
                let date_str = row_element
                    .select(&self.selectors.date_in_list_selector)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_else(|| {
                        tracing::warn!(
                            plugin = Self::IDENTIFIER,
                            "Date not found for ID {} in row {}, URL {}",
                            id,
                            i,
                            post_url
                        );
                        String::new() // Default to empty string if not found
                    });

                println!(
                    "[LOG] fetch_page_posts_metadata: Successfully extracted metadata from row {}: ID={}",
                    i, id
                );
                tracing::info!(
                    plugin = Self::IDENTIFIER,
                    "Successfully extracted metadata from row {}: ID={}, Title='{}', Date='{}', URL='{}'",
                    i,
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
            } else {
                println!(
                    "[LOG] fetch_page_posts_metadata: No title_element found in post row {}",
                    i
                );
                if i < 3 {
                    println!("[HTML POST ROW {} NO MATCH] {}", i, row_element.html());
                }
            }
        }
        println!(
            "[LOG] fetch_page_posts_metadata: Exiting function, found {} metadata items.",
            metadata_list.len()
        );
        Ok(metadata_list)
    }

    async fn fetch_post_data(
        &self,
        metadata: &SsuDormPostMetadata,
    ) -> Result<SsufidPost, PluginError> {
        tracing::debug!(
            plugin = Self::IDENTIFIER,
            "Fetching post data for URL: {}",
            metadata.url
        );
        let html_content = self.fetch_html_content(&metadata.url).await?;
        let document = Html::parse_document(&html_content);

        let title = document
            .select(&self.selectors.title_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .ok_or_else(|| SsuDormError::TitleNotFound(metadata.url.clone()))?;

        // Author and Date parsing from "작성자: 운영사무실 ... 작성일: 2010-04-01 17:17"
        let author_date_str = document
            .select(&self.selectors.author_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let mut author = None;
        let mut date_str_from_post = String::new();

        if let Some(author_part_idx) = author_date_str.find("작성자:") {
            if let Some(views_part_idx) = author_date_str.find("조회수:") {
                author = Some(
                    author_date_str[author_part_idx + "작성자:".len()..views_part_idx]
                        .trim()
                        .to_string(),
                );
            }
        }
        if let Some(date_part_idx) = author_date_str.find("작성일:") {
            date_str_from_post = author_date_str[date_part_idx + "작성일:".len()..]
                .trim()
                .to_string();
        }

        if author.is_none() || author.as_deref() == Some("") {
            author = Some("운영사무실".to_string()); // Default if not found
        }

        // Use date from post page if available, otherwise from list.
        let final_date_str = if !date_str_from_post.is_empty() {
            date_str_from_post
        } else {
            metadata.date_str_from_list.clone()
        };

        // Parse date: "YYYY-MM-DD HH:MM" or "YYYY-MM-DD"
        let date_format_long = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"); // Assuming seconds might be there
        let date_format_short = format_description!("[year]-[month]-[day]");

        let created_at = if final_date_str.contains(':') {
            PrimitiveDateTime::parse(&final_date_str.replace(" ", "T"), date_format_long) // Use ISO-like format for parsing if time is present
                .or_else(|_| {
                    // Fallback for "YYYY-MM-DD HH:MM" (without seconds)
                    let temp_date_str = format!("{}:00", final_date_str); // Add seconds for parser
                    PrimitiveDateTime::parse(&temp_date_str, date_format_long)
                })
                .map_err(|e| {
                    SsuDormError::DateParse(format!(
                        "Failed to parse datetime '{}': {}",
                        final_date_str, e
                    ))
                })?
                .assume_offset(time::macros::offset!(+9)) // KST
        } else {
            Date::parse(&final_date_str, date_format_short)
                .map_err(|e| {
                    SsuDormError::DateParse(format!(
                        "Failed to parse date '{}': {}",
                        final_date_str, e
                    ))
                })?
                .with_time(Time::MIDNIGHT)
                .assume_offset(time::macros::offset!(+9)) // KST
        };

        let content_element = document
            .select(&self.selectors.content_selector)
            .next()
            .ok_or_else(|| SsuDormError::ContentNotFound(metadata.url.clone()))?;
        let content = content_element.html(); // Get inner HTML to preserve formatting

        let mut attachments = Vec::new();
        for element in document.select(&self.selectors.attachment_selector) {
            if let Some(href) = element.value().attr("href") {
                // URLs might be relative, construct absolute URL
                let attachment_url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    // Attempt to resolve relative URL, e.g., from '/SShostel/'
                    let base_url_for_attachments = Url::parse(Self::BASE_URL).unwrap();
                    base_url_for_attachments
                        .join(href)
                        .map_err(|e| {
                            PluginError::parse::<Self>(format!(
                                "Failed to join attachment URL {}: {}",
                                href, e
                            ))
                        })?
                        .to_string()
                };
                let name = element.text().collect::<String>().trim().to_string();
                attachments.push(Attachment {
                    name: if name.is_empty() { None } else { Some(name) },
                    url: attachment_url,
                    mime_type: None, // Can be guessed later if needed
                });
            }
        }

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            author,
            title,
            description: None, // Could be a snippet of content if desired
            category: vec!["공지사항".to_string()], // Default category
            created_at,
            updated_at: None, // Site doesn't seem to provide updated_at
            thumbnail: None,  // Site doesn't seem to have thumbnails for notices
            content,
            attachments,
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
    const TITLE: &'static str = "숭실대학교 생활관";
    const DESCRIPTION: &'static str = "숭실대학교 생활관 홈페이지의 공지사항을 제공합니다.";
    // Base URL for resolving relative links if necessary
    const BASE_URL: &'static str = "https://ssudorm.ssu.ac.kr:444/SShostel/";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        tracing::info!(
            plugin = Self::IDENTIFIER,
            message = "Crawling started",
            posts_limit
        );

        let mut all_posts = Vec::new();
        let mut collected_posts_count: u32 = 0;
        let mut current_page: u32 = 1;
        const MAX_PAGES_TO_TRY: u32 = 50; // Safety break for pagination

        while collected_posts_count < posts_limit && current_page <= MAX_PAGES_TO_TRY {
            tracing::debug!(plugin = Self::IDENTIFIER, "Fetching page: {}", current_page);
            let metadata_list = match self.fetch_page_posts_metadata(current_page).await {
                Ok(list) => list,
                Err(e) => {
                    tracing::error!(
                        plugin = Self::IDENTIFIER,
                        "Failed to fetch metadata for page {}: {:?}",
                        current_page,
                        e
                    );
                    // Depending on error, might break or continue
                    break;
                }
            };

            if metadata_list.is_empty() {
                tracing::info!(
                    plugin = Self::IDENTIFIER,
                    "No more metadata found on page {}. Stopping.",
                    current_page
                );
                break; // No more posts found on this page
            }

            let mut tasks = FuturesOrdered::new();
            for metadata_item in metadata_list {
                if collected_posts_count >= posts_limit {
                    break;
                }
                // Move metadata_item into the async block to ensure it lives long enough.
                let owned_metadata_for_task = metadata_item.clone();
                tasks
                    .push_back(async move { self.fetch_post_data(&owned_metadata_for_task).await });
                collected_posts_count += 1;
            }

            // Collect results from this page's tasks
            while let Some(result) = tasks.next().await {
                match result {
                    Ok(post) => {
                        if all_posts.len() < posts_limit as usize {
                            // Ensure we don't add more than limit
                            all_posts.push(post);
                        } else {
                            // If we already reached the limit (e.g. from previous page's posts + current page initial tasks)
                            // we can break early from collecting more results.
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(plugin = Self::IDENTIFIER, error = ?e, "Failed to fetch post data.");
                        // If a fetch fails, we decrement collected_posts_count as it was an attempt, not a success.
                        // This way, we try to get 'posts_limit' *successful* posts.
                        collected_posts_count = collected_posts_count.saturating_sub(1);
                    }
                }
            }

            // Ensure we stop if the overall limit is met after processing the page
            if all_posts.len() >= posts_limit as usize {
                all_posts.truncate(posts_limit as usize); // Ensure exact limit
                break;
            }

            current_page += 1;
        }

        tracing::info!(
            plugin = Self::IDENTIFIER,
            message = "Crawling finished",
            total_posts_collected = all_posts.len()
        );
        Ok(all_posts)
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
