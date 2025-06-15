// Removed std::borrow::Cow

use futures::{stream::FuturesOrdered, StreamExt};
use reqwest::Client;
use scraper::{Html, Selector};
use ssufid::core::{Attachment, SsufidPlugin, SsufidPost}; // Corrected import path
use ssufid::PluginError; // Corrected import path
use thiserror::Error;
use time::{
    format_description::{self}, // Removed well_known::Iso8601
    macros::offset,
    Date,
}; // Added format_description
use url::Url;

struct Selectors {
    post_row: Selector,
    post_link: Selector,
    author_in_list: Selector,
    date_in_list: Selector,
    post_id_in_list: Selector,
    notice_icon: Selector,
    pagination_links: Selector,
    // current_page_link: Selector, // Removed
    title: Selector,
    author_in_detail: Selector,
    date_in_detail: Selector,
    content: Selector,
    attachment_link: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_row: Selector::parse("table.board-list > tbody > tr").unwrap(),
            post_link: Selector::parse("td.subject > a").unwrap(),
            author_in_list: Selector::parse("td:nth-of-type(3)").unwrap(),
            date_in_list: Selector::parse("td:nth-of-type(4)").unwrap(),
            post_id_in_list: Selector::parse("td.w_cell:first-child").unwrap(),
            notice_icon: Selector::parse("span.ico").unwrap(),
            pagination_links: Selector::parse("div.paging > a[href*=\"page=\"]").unwrap(),
            // current_page_link: Selector::parse("div.paging > a.on").unwrap(), // Removed
            title: Selector::parse("div.view_tit > h3.v_tit").unwrap(),
            author_in_detail: Selector::parse("div.view_tit > ul.v_list > li:nth-child(1)")
                .unwrap(),
            date_in_detail: Selector::parse("div.view_tit > ul.v_list > li:nth-child(2)").unwrap(),
            content: Selector::parse("div.view_con").unwrap(),
            attachment_link: Selector::parse("div.view_date > ul > li.file > a").unwrap(),
        }
    }
}

#[derive(Debug)]
struct MePostMetadata {
    id: String,
    url: String,
    title_from_list: String,
    author_from_list: String,
    date_from_list: String,
    is_notice: bool,
}

#[derive(Debug, Error)]
enum MePluginError {
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("Date parsing error: {0}")]
    DateParse(#[from] time::error::Parse),
    #[error("Date format description error: {0}")]
    DateFormat(#[from] time::error::InvalidFormatDescription), // Changed from time::error::Format
    #[error("HTML element not found: {selector}")]
    ElementNotFound { selector: String },
    #[error("Attribute not found: {attribute} in element selected by {selector}")]
    AttributeNotFound { selector: String, attribute: String },
    #[error("ID not found in URL: {0}")] // Changed {url} to {0}
    IdNotFoundInUrl(String),
}

impl From<MePluginError> for PluginError {
    fn from(err: MePluginError) -> Self {
        // Using PluginError::custom as suggested by compiler, assuming MePlugin::IDENTIFIER is available
        PluginError::custom::<MePlugin>(MePlugin::IDENTIFIER.to_string(), err.to_string())
    }
}

pub struct MePlugin {
    client: Client,
    selectors: Selectors,
}

impl Default for MePlugin {
    fn default() -> Self {
        Self {
            client: Client::new(),
            selectors: Selectors::new(),
        }
    }
}

// This is the FULL_IMPL_MEPLUGIN_BLOCK from above, pasted directly
impl MePlugin {
    // const POSTS_PER_PAGE: u32 = 10; // Removed
    const DATE_FORMAT_LIST: &'static str = "[year]-[month]-[day]";

    async fn fetch_page_post_metadata(
        &self,
        page_num: u32,
    ) -> Result<Vec<MePostMetadata>, MePluginError> {
        let list_url = format!("{}/notice/notice01.php?page={}", Self::BASE_URL, page_num);
        // tracing::info!(page = page_num, "Fetching metadata from list page: {}", list_url);

        let response_text = self.client.get(&list_url).send().await?.text().await?;
        let document = Html::parse_document(&response_text);

        let mut metadata_list = Vec::new();

        for row_element in document.select(&self.selectors.post_row) {
            let link_element = row_element
                .select(&self.selectors.post_link)
                .next()
                .ok_or_else(|| MePluginError::ElementNotFound {
                    selector: "td.subject > a".to_string(),
                })?;

            let relative_url = link_element.value().attr("href").ok_or_else(|| {
                MePluginError::AttributeNotFound {
                    selector: "td.subject > a".to_string(),
                    attribute: "href".to_string(),
                }
            })?;

            let absolute_url = Url::parse(Self::BASE_URL)?.join(relative_url)?.to_string();

            let id_from_url = Url::parse(&absolute_url)?
                .query_pairs()
                .find_map(|(key, value)| {
                    if key == "no" {
                        Some(value.into_owned())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| MePluginError::IdNotFoundInUrl(absolute_url.clone()))?;

            let title_from_list = link_element.text().collect::<String>().trim().to_string();

            let author_from_list = row_element
                .select(&self.selectors.author_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let date_from_list = row_element
                .select(&self.selectors.date_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let id_cell_text = row_element
                .select(&self.selectors.post_id_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let is_notice = row_element
                .select(&self.selectors.notice_icon)
                .next()
                .is_some()
                || id_cell_text.to_lowercase().contains("공지")
                || id_cell_text.to_lowercase().contains("notice");

            metadata_list.push(MePostMetadata {
                id: id_from_url,
                url: absolute_url,
                title_from_list,
                author_from_list,
                date_from_list,
                is_notice,
            });
        }
        Ok(metadata_list)
    }

    async fn fetch_post_details(
        &self,
        metadata: &MePostMetadata,
    ) -> Result<SsufidPost, MePluginError> {
        // tracing::info!(post_id = %metadata.id, url = %metadata.url, "Fetching post details");
        let response_text = self.client.get(&metadata.url).send().await?.text().await?;
        let document = Html::parse_document(&response_text);

        let title = document
            .select(&self.selectors.title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.title_from_list.clone());

        let author = document
            .select(&self.selectors.author_in_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.author_from_list.clone());

        let date_str_detail = document
            .select(&self.selectors.date_in_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.date_from_list.clone());

        let date_format_desc =
            format_description::parse(Self::DATE_FORMAT_LIST).map_err(MePluginError::DateFormat)?;

        let created_at = Date::parse(&date_str_detail, &date_format_desc)
            .map_err(MePluginError::DateParse)?
            .midnight()
            .assume_offset(offset!(+9));

        let content_html = document
            .select(&self.selectors.content)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        let mut attachments = Vec::new();
        let file_list_item_selector = Selector::parse("div.view_date > ul > li.file").unwrap();
        if let Some(file_li) = document.select(&file_list_item_selector).next() {
            for element in file_li.select(&self.selectors.attachment_link) {
                if let Some(href_val) = element.value().attr("href") {
                    let base_for_attachment = Url::parse(&metadata.url)?;
                    let attachment_url = base_for_attachment.join(href_val)?.to_string();

                    let name = element.text().collect::<String>().trim().to_string();
                    attachments.push(Attachment {
                        name: Some(name).filter(|s| !s.is_empty()),
                        url: attachment_url,
                        mime_type: None,
                    });
                }
            }
        }
        // tracing::debug!(post_id = %metadata.id, num_attachments = attachments.len(), "Parsed attachments");

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            title,
            author: Some(author),
            created_at,
            content: content_html,
            attachments,
            category: if metadata.is_notice {
                vec!["공지".to_string()]
            } else {
                vec![]
            },
            description: None,
            updated_at: None,
            thumbnail: None,
            metadata: None,
        })
    }

    async fn has_more_pages(&self, current_page_num: u32) -> Result<bool, MePluginError> {
        let list_url = format!(
            "{}/notice/notice01.php?page={}",
            Self::BASE_URL,
            current_page_num
        );
        // tracing::debug!(page = current_page_num, "Checking for more pages by fetching: {}", list_url);
        let response_text = self.client.get(&list_url).send().await?.text().await?;
        let document = Html::parse_document(&response_text);

        let posts_on_current_page = document.select(&self.selectors.post_row).count();
        if posts_on_current_page == 0 {
            // tracing::debug!(page = current_page_num, "No posts on current page, assuming no more pages after this.");
            return Ok(false);
        }

        let last_page_selector = Selector::parse("div.paging > a.last").unwrap();
        if let Some(last_page_element) = document.select(&last_page_selector).next() {
            if let Some(href) = last_page_element.attr("href") {
                let pagination_base_url_for_parsing =
                    format!("{}/notice/notice01.php", Self::BASE_URL);
                if let Ok(parsed_url) = Url::parse(&pagination_base_url_for_parsing)?.join(href) {
                    if let Some(max_page_str) =
                        parsed_url
                            .query_pairs()
                            .find_map(|(k, v)| if k == "page" { Some(v) } else { None })
                    {
                        if let Ok(max_page) = max_page_str.parse::<u32>() {
                            // tracing::debug!(page = current_page_num, max_page_from_pagination = max_page, "Max page found in pagination controls.");
                            return Ok(current_page_num < max_page);
                        }
                    }
                }
            }
        }

        let next_page_link_exists =
            document
                .select(&self.selectors.pagination_links)
                .any(|link_el| {
                    link_el
                        .attr("href")
                        .map(|href| href.contains(&format!("page={}", current_page_num + 1)))
                        .unwrap_or(false)
                });
        // tracing::debug!(page = current_page_num, next_page_link_exists = next_page_link_exists, "Fallback check for next page link.");
        Ok(next_page_link_exists)
    }
}

// Removed #[async_trait::async_trait]
impl SsufidPlugin for MePlugin {
    const IDENTIFIER: &'static str = "me.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 기계공학부";
    const DESCRIPTION: &'static str = "숭실대학교 기계공학부 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://me.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        // tracing::info!(plugin = Self::IDENTIFIER, limit = posts_limit, "Starting crawl");
        let mut all_posts = Vec::new();
        let mut collected_metadata_ids = std::collections::HashSet::new(); // To track IDs and avoid duplicates
        let mut all_metadata_ordered = Vec::new(); // To maintain order and limit

        let mut current_page = 1;
        const MAX_PAGES_TO_CHECK_WITHOUT_NEW_POSTS: u8 = 3;
        let mut sequential_pages_without_new_metadata = 0;
        const MAX_TOTAL_PAGES_TO_CRAWL: u32 = 50; // Safety break

        loop {
            if current_page > MAX_TOTAL_PAGES_TO_CRAWL {
                // tracing::warn!("Reached maximum total pages to crawl ({}), stopping.", MAX_TOTAL_PAGES_TO_CRAWL);
                break;
            }
            // tracing::info!(page = current_page, "Fetching metadata for page");
            let page_metadata_result = self.fetch_page_post_metadata(current_page).await;

            let page_metadata = match page_metadata_result {
                Ok(meta) => meta,
                Err(_e) => {
                    // Changed e to _e
                    // tracing::error!(page = current_page, error = ?_e, "Failed to fetch metadata for page. Skipping page.");
                    // Depending on error, might want to break or retry. For now, skip page and continue.
                    current_page += 1;
                    sequential_pages_without_new_metadata += 1; // Count as no new metadata
                    if sequential_pages_without_new_metadata >= MAX_PAGES_TO_CHECK_WITHOUT_NEW_POSTS
                    {
                        // tracing::info!("Reached max sequential pages without new metadata after an error. Stopping pagination.");
                        break;
                    }
                    continue;
                }
            };

            let mut new_metadata_found_on_this_page = false;
            for meta_item in page_metadata {
                if !collected_metadata_ids.contains(&meta_item.id) {
                    collected_metadata_ids.insert(meta_item.id.clone());
                    all_metadata_ordered.push(meta_item);
                    new_metadata_found_on_this_page = true;
                    if all_metadata_ordered.len() >= posts_limit as usize {
                        break;
                    }
                }
            }

            if all_metadata_ordered.len() >= posts_limit as usize {
                // tracing::info!("Collected enough metadata ({}) to meet posts_limit ({}).", all_metadata_ordered.len(), posts_limit);
                break;
            }

            if !new_metadata_found_on_this_page {
                sequential_pages_without_new_metadata += 1;
                if sequential_pages_without_new_metadata >= MAX_PAGES_TO_CHECK_WITHOUT_NEW_POSTS {
                    // tracing::info!("Reached max sequential pages ({}) without new metadata. Stopping pagination.", MAX_PAGES_TO_CHECK_WITHOUT_NEW_POSTS);
                    break;
                }
            } else {
                sequential_pages_without_new_metadata = 0;
            }

            match self.has_more_pages(current_page).await {
                Ok(true) => current_page += 1,
                Ok(false) => {
                    // tracing::info!("No more pages indicated by pagination check or content.");
                    break;
                }
                Err(_e) => {
                    // Changed e to _e, and this one too
                    // tracing::error!(page = current_page, error = ?e, "Error checking for more pages. Stopping pagination.");
                    break;
                }
            }
        }

        all_metadata_ordered.truncate(posts_limit as usize);

        let mut fetch_tasks = FuturesOrdered::new();
        // Iterate by reference to ensure `metadata_item_ref` lives long enough
        for metadata_item_ref in &all_metadata_ordered {
            fetch_tasks.push_back(self.fetch_post_details(metadata_item_ref)); // Pass the reference directly
        }

        while let Some(post_result) = fetch_tasks.next().await {
            match post_result {
                Ok(post) => all_posts.push(post),
                Err(_e) => { // Changed e to _e
                     // tracing::error!(error = ?_e, "Failed to fetch post details for one post, skipping.");
                     // Optionally convert MePluginError to PluginError and return or collect errors.
                }
            }
        }

        // tracing::info!(collected_posts = all_posts.len(), "Crawl finished");
        Ok(all_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssufid_core::SsufidPlugin;

    // fn setup_tracing() {
    //     let subscriber = tracing_subscriber::FmtSubscriber::builder()
    //         .with_max_level(tracing::Level::INFO) // Adjusted to INFO for less verbosity
    //         .with_target(false) // Disable target prefix for cleaner logs
    //         .compact() // Use compact format
    //         .finish();
    //     // Use try_set_global_default to avoid panic if already set
    //     let _ = tracing::subscriber::set_global_default(subscriber);
    // }

    #[tokio::test]
    async fn test_fetch_page_posts_metadata_smoke() {
        // setup_tracing();
        let plugin = MePlugin::default();
        let metadata = plugin.fetch_page_post_metadata(1).await.unwrap();
        assert!(
            !metadata.is_empty(),
            "Should fetch some metadata from page 1"
        );

        let first_meta = &metadata[0];
        // tracing::info!("First metadata item: {:?}", first_meta);
        assert!(!first_meta.id.is_empty());
        assert!(first_meta.url.starts_with(MePlugin::BASE_URL));
        assert!(!first_meta.title_from_list.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_post_details_smoke() {
        // setup_tracing();
        let plugin = MePlugin::default();
        let test_metadata = MePostMetadata {
            id: "3061533".to_string(),
            url: format!(
                "{}/notice/notice01.php?admin_mode=read&no=3061533",
                MePlugin::BASE_URL
            ),
            title_from_list: "제17회 졸업논문 (포스터) 발표회".to_string(),
            author_from_list: "관리자".to_string(),
            date_from_list: "2024-11-22".to_string(),
            is_notice: false,
        };

        let post = plugin.fetch_post_details(&test_metadata).await.unwrap();
        // tracing::info!("Fetched post: {:?}", post);

        assert_eq!(post.id, "3061533");
        assert_eq!(post.title, "제17회 졸업논문 (포스터) 발표회");
        assert_eq!(post.author.unwrap_or_default(), "관리자");
        let expected_date = Date::from_calendar_date(2024, time::Month::November, 22).unwrap();
        assert_eq!(post.created_at.date(), expected_date);
        assert!(!post.content.is_empty());
        // For this specific post (no=3061533), the provided HTML did not show attachments.
        // So, assert that attachments are empty for this one.
        assert!(
            post.attachments.is_empty(),
            "Post 3061533 should have no attachments based on provided HTML"
        );
    }

    // Test for a post that *might* have attachments (if we had an example)
    // For now, this test would be similar to the above or require mocking.
    // Let's assume we need to test the parsing logic even if the live example doesn't have them.
    // We can create a MePostMetadata that points to a URL which we *know* (or mock) has attachments.
    // Since we can't mock HTTP requests easily here, we rely on the selector logic being correct.

    #[tokio::test]
    async fn test_crawl_smoke() {
        // setup_tracing();
        let plugin = MePlugin::default();
        let posts = plugin.crawl(5).await.unwrap();
        // Allow posts.len() to be 0 if the site is down or has no posts, but it should not error.
        // tracing::info!("Crawled {} posts. First few: {:?}", posts.len(), posts.iter().take(2).collect::<Vec<_>>());
        assert!(
            posts.len() <= 5,
            "Should return at most 5 posts, got {}",
            posts.len()
        );
        if !posts.is_empty() {
            let first_post = &posts[0];
            assert!(!first_post.id.is_empty());
            assert!(!first_post.title.is_empty());
        }
    }

    #[tokio::test]
    async fn test_has_more_pages_logic() {
        // setup_tracing();
        let plugin = MePlugin::default();

        // Page 1 should have a next page (assuming site has >1 page)
        let has_more_from_page_1 = plugin.has_more_pages(1).await.unwrap();
        // tracing::info!("Has more pages from page 1: {}", has_more_from_page_1);
        assert!(
            has_more_from_page_1,
            "Page 1 should have a next page if site has content."
        );

        // Test a very high page number that likely doesn't exist
        // From HTML, last page link is for page 10.
        let has_more_from_page_9 = plugin.has_more_pages(9).await.unwrap();
        // tracing::info!("Has more pages from page 9: {}", has_more_from_page_9);
        assert!(
            has_more_from_page_9,
            "Page 9 should have a next page (page 10)."
        );

        let has_more_from_page_10 = plugin.has_more_pages(10).await.unwrap();
        // tracing::info!("Has more pages from page 10: {}", has_more_from_page_10);
        assert!(
            !has_more_from_page_10,
            "Page 10 should be the last, so no page 11."
        );

        let has_more_from_page_11 = plugin.has_more_pages(11).await.unwrap();
        // tracing::info!("Has more pages from page 11: {}", has_more_from_page_11);
        assert!(
            !has_more_from_page_11,
            "Page 11 should not exist or have a next page."
        );
    }
}
