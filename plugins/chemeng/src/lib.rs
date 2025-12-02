// Content for plugins/ssufid_chemeng/src/lib.rs

use futures::{StreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector};
use thiserror::Error;
use url::Url;

use ssufid::{
    core::{SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{Date, macros::format_description, macros::offset};

// Selector guesses - these will likely need refinement
struct Selectors {
    // List page selectors
    notice_row: Selector,
    row_link_title: Selector,
    row_author: Selector,
    row_date: Selector,
    // Detail page selectors
    post_title: Selector,
    post_author_info: Selector, // Changed from post_author_date_info
    post_date_info: Selector,   // New selector for date
    post_content: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // --- List page selectors (verified from previous attempt) ---
            notice_row: Selector::parse("table tr").unwrap(),
            row_link_title: Selector::parse("td:nth-child(2) > a").unwrap(),
            row_author: Selector::parse("td:nth-child(3)").unwrap(),
            row_date: Selector::parse("td:nth-child(4)").unwrap(),

            // --- Detail page selectors (newly provided) ---
            post_title: Selector::parse("div.board-view > div.head > h3.tit").unwrap(),
            post_author_info: Selector::parse("div.board-view > div.head > div.info > span.name")
                .unwrap(),
            post_date_info: Selector::parse("div.board-view > div.head > div.info > span.date")
                .unwrap(),
            post_content: Selector::parse("div.board-view > div.body").unwrap(),
        }
    }
}

#[derive(Debug)]
struct ChemEngPostMetadata {
    id: String,
    url: String,
    title_on_list: String,
    author_on_list: String,
    date_str_on_list: String,
}

#[derive(Debug, Error)]
#[allow(dead_code)] // Allow dead code for variants that might be used later
enum MetadataError {
    #[error("URL not found in row")]
    UrlNotFound,
    #[error("Title not found in row")]
    TitleNotFound,
    #[error("Author not found in row")]
    AuthorNotFound,
    #[error("Date not found in row")]
    DateNotFound,
    #[error("Post ID (idx) not found in URL: {0}")]
    PostIdMissing(String),
    #[error(
        "Row is likely a header or malformed (e.g. no 'idx' in URL, not '공지'), skipping: {row_html}"
    )]
    RowMalformedSkipped { row_html: String },
}

impl From<MetadataError> for PluginError {
    fn from(err: MetadataError) -> Self {
        PluginError::parse::<ChemEngPlugin>(format!("Metadata error: {err}"))
    }
}

pub struct ChemEngPlugin {
    selectors: Selectors,
    client: reqwest::Client,
}

impl Default for ChemEngPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ChemEngPlugin {
    const POSTS_PER_PAGE: u32 = 10;
    const DATE_FORMAT_PARSE: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]-[month]-[day]");

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            client: reqwest::Client::new(),
        }
    }

    fn get_base_url_object(&self) -> Url {
        Url::parse(Self::BASE_URL).expect("BASE_URL is invalid")
    }

    fn get_list_page_url(&self, page_num: u32) -> Url {
        let mut url = self.get_base_url_object().join("sub/sub03_01.php").unwrap();
        if page_num > 1 {
            let offset_val = (page_num - 1) * Self::POSTS_PER_PAGE;
            url.query_pairs_mut()
                .append_pair("boardid", "notice1")
                .append_pair("offset", &offset_val.to_string());
        } else {
            url.query_pairs_mut().append_pair("boardid", "notice1");
        }
        url
    }

    async fn fetch_page_posts_metadata(
        &self,
        page_num: u32,
    ) -> Result<Vec<ChemEngPostMetadata>, PluginError> {
        let page_url = self.get_list_page_url(page_num);
        tracing::debug!(message = "Fetching metadata list", page_url = %page_url);

        let response_text = self
            .client
            .get(page_url.clone())
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!("Requesting list page {page_num}: {e}"))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::parse::<Self>(format!("Parsing list page {page_num}: {e}"))
            })?;

        let document = Html::parse_document(&response_text);
        let mut posts_metadata = Vec::new();

        for element in document.select(&self.selectors.notice_row) {
            if element
                .select(&Selector::parse("th").unwrap())
                .next()
                .is_some()
            {
                tracing::trace!("Skipping header row: {:?}", element.html());
                continue;
            }
            if element.select(&Selector::parse("td").unwrap()).count() < 4 {
                tracing::trace!("Skipping row, not enough cells: {:?}", element.html());
                continue;
            }

            let first_cell_text_raw = element
                .select(&Selector::parse("td:first-child").unwrap())
                .next();
            let first_cell_text = first_cell_text_raw.map_or(String::new(), |el| {
                el.text().collect::<String>().trim().to_string()
            });

            let is_announcement = first_cell_text == "공지";

            let link_element = element.select(&self.selectors.row_link_title).next();

            let title_on_list = link_element
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|t| !t.is_empty())
                .ok_or(MetadataError::TitleNotFound)?; // Clippy: unnecessary_lazy_evaluations

            let relative_url_str = link_element
                .and_then(|el| el.value().attr("href"))
                .ok_or(MetadataError::UrlNotFound)?;

            let post_url_obj = self
                .get_base_url_object()
                .join(relative_url_str)
                .map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Invalid post URL '{relative_url_str}': {e}"
                    ))
                })?;

            let id_from_url = post_url_obj.query_pairs().find_map(|(key, value)| {
                if key == "idx" {
                    Some(value.into_owned())
                } else {
                    None
                }
            });

            let id = match id_from_url {
                Some(id_val) if !id_val.is_empty() => id_val,
                _ => {
                    if is_announcement {
                        // Create a pseudo-ID for announcements based on title and date to make it somewhat unique
                        let date_for_id = element
                            .select(&self.selectors.row_date)
                            .next()
                            .map_or("nodate".to_string(), |el| {
                                el.text().collect::<String>().trim().to_string()
                            });
                        format!(
                            "notice_{}_{}",
                            date_for_id,
                            title_on_list.chars().take(10).collect::<String>()
                        )
                    } else {
                        // If not an announcement and no 'idx', it's an issue.
                        tracing::warn!(message="Post ID (idx) missing and not '공지'", url=%post_url_obj, row_html=?element.html());
                        // Continue to next iteration instead of returning error for the whole page
                        continue;
                    }
                }
            };

            let author_on_list = element
                .select(&self.selectors.row_author)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .ok_or(MetadataError::AuthorNotFound)?;

            let date_str_on_list = element
                .select(&self.selectors.row_date)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .ok_or(MetadataError::DateNotFound)?;

            if Date::parse(&date_str_on_list, Self::DATE_FORMAT_PARSE).is_err() {
                tracing::warn!(message="Date format error on list page, skipping row.", date_str=%date_str_on_list, title=%title_on_list);
                continue; // Skip this post if date is unparseable
            }

            posts_metadata.push(ChemEngPostMetadata {
                id,
                url: post_url_obj.to_string(),
                title_on_list,
                author_on_list,
                date_str_on_list,
            });
        }
        Ok(posts_metadata)
    }

    async fn fetch_post(
        &self,
        post_metadata: ChemEngPostMetadata, // Take ownership
    ) -> Result<SsufidPost, PluginError> {
        tracing::debug!(message="Fetching post content", url=%post_metadata.url);
        let response_text = self
            .client
            .get(&post_metadata.url)
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!("Requesting post {}: {}", post_metadata.id, e))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::parse::<Self>(format!("Parsing post {}: {}", post_metadata.id, e))
            })?;

        let document = Html::parse_document(&response_text);

        let title = document
            .select(&self.selectors.post_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| post_metadata.title_on_list.clone());

        let author_from_page = document
            .select(&self.selectors.post_author_info)
            .next()
            .map(|el| {
                // Try to get <strong> text, then fall back to span's text
                el.select(&Selector::parse("strong").unwrap())
                    .next()
                    .map_or_else(
                        || el.text().collect::<String>().trim().to_string(),
                        |strong_el| strong_el.text().collect::<String>().trim().to_string(),
                    )
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| post_metadata.author_on_list.clone());

        let date_str_from_page = document
            .select(&self.selectors.post_date_info)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| post_metadata.date_str_on_list.clone());

        let created_at_date = Date::parse(&date_str_from_page, Self::DATE_FORMAT_PARSE)
            .or_else(|e| {
                tracing::warn!(
                    error = ?e,
                    message = "Date format error on detail page, falling back to list date.",
                    parsed_date_str = %date_str_from_page,
                    fallback_date_str = %post_metadata.date_str_on_list,
                    post_id = %post_metadata.id
                );
                Date::parse(&post_metadata.date_str_on_list, Self::DATE_FORMAT_PARSE)
            })
            .map_err(|e| {
                PluginError::parse::<Self>(format!(
                    "Failed to parse date '{}' for post {}: {}",
                    &post_metadata.date_str_on_list, post_metadata.id, e
                ))
            })?;
        let created_at = created_at_date.midnight().assume_offset(offset!(+9));

        let content = document
            .select(&self.selectors.post_content)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        Ok(SsufidPost {
            id: post_metadata.id.clone(),
            url: post_metadata.url.clone(),
            title,
            author: Some(author_from_page),
            description: None,
            category: vec!["학부공지사항".to_string()],
            created_at,
            content,
            updated_at: None,
            thumbnail: None,
            attachments: vec![],
            metadata: None,
        })
    }

    fn get_total_pages_from_list_html(&self, document: &Html) -> u32 {
        // Attempt to parse "페이지정보 : X / Y" from the raw text of the page
        // This is fragile and depends on the exact text format.
        let body_text_nodes = document
            .select(&Selector::parse("body").unwrap())
            .next()
            .map(|b| b.text().collect::<String>());
        if let Some(body_text) = body_text_nodes
            && let Some(page_info_start_idx) = body_text.find("페이지정보 :") {
                let relevant_part = &body_text[page_info_start_idx + "페이지정보 :".len()..];
                if let Some(slash_idx) = relevant_part.find('/') {
                    let after_slash = &relevant_part[slash_idx + 1..];
                    // Take characters until a non-digit (excluding whitespace) is found
                    let total_pages_str: String = after_slash
                        .trim()
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(num_pages) = total_pages_str.parse::<u32>()
                        && num_pages > 0 {
                            tracing::debug!(
                                "Parsed total pages from '페이지정보' text: {}",
                                num_pages
                            );
                            return num_pages;
                        }
                }
            }
        tracing::warn!(
            "Could not parse total pages from '페이지정보' text. Using fallback of 70 based on observation."
        );
        70 // Fallback based on initial observation "1 / 69" implies around 69-70 pages.
    }
}

impl SsufidPlugin for ChemEngPlugin {
    const IDENTIFIER: &'static str = "chemeng.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 화학공학과";
    const DESCRIPTION: &'static str = "숭실대학교 화학공학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://chemeng.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        if posts_limit == 0 {
            return Ok(Vec::new());
        }

        let total_pages_on_site = {
            // Create a new scope to ensure first_page_document is dropped
            let first_page_url = self.get_list_page_url(1);
            let first_page_response_text = self
                .client
                .get(first_page_url)
                .send()
                .await
                .map_err(|e| {
                    PluginError::request::<Self>(format!(
                        "Fetching first page for total_pages: {e}"
                    ))
                })?
                .text()
                .await
                .map_err(|e| {
                    PluginError::parse::<Self>(format!("Parsing first page for total_pages: {e}"))
                })?;
            let first_page_document = Html::parse_document(&first_page_response_text);
            self.get_total_pages_from_list_html(&first_page_document)
        };

        tracing::info!("Estimated total pages on site: {}", total_pages_on_site);

        let mut all_posts_metadata: Vec<ChemEngPostMetadata> = Vec::new();
        let mut current_page = 1;

        // The comment below about re-fetching page 1 is addressed by the loop structure.
        // Page 1 metadata will be fetched by fetch_page_posts_metadata in the first iteration.

        loop {
            if all_posts_metadata.len() >= posts_limit as usize {
                tracing::debug!(
                    "Reached posts_limit for metadata ({}) at page {}",
                    posts_limit,
                    current_page
                );
                break;
            }
            // Stop if current_page exceeds known total pages or a safety limit
            if current_page > total_pages_on_site || current_page > 200 {
                // 200 as a hard safety limit
                tracing::debug!(
                    "Stopping metadata collection: current_page ({}) > total_pages_on_site ({}) or safety limit.",
                    current_page,
                    total_pages_on_site
                );
                break;
            }

            tracing::debug!("Fetching metadata for page {}", current_page);
            let metadata_from_page = match self.fetch_page_posts_metadata(current_page).await {
                Ok(metadata) => metadata,
                Err(e) => {
                    tracing::error!(
                        "Failed to fetch metadata from page {}: {:?}. Stopping crawl.",
                        current_page,
                        e
                    );
                    // Depending on severity, you might choose to return partial results or the error.
                    // For now, let's stop and return the error to indicate an issue.
                    return Err(e);
                }
            };

            // If a page is empty (and it's not the first page trying to determine total pages), assume end of posts
            if metadata_from_page.is_empty() && current_page > 1 {
                tracing::debug!(
                    "No more posts found on page {}. Stopping metadata collection.",
                    current_page
                );
                break;
            }

            all_posts_metadata.extend(metadata_from_page);
            current_page += 1;
        }

        all_posts_metadata.truncate(posts_limit as usize);
        tracing::info!(
            "Collected {} metadata items after truncation to limit {}.",
            all_posts_metadata.len(),
            posts_limit
        );

        let mut posts_futures = FuturesOrdered::new();
        for metadata_item in all_posts_metadata {
            // all_posts_metadata is moved here
            posts_futures.push_back(self.fetch_post(metadata_item));
        }

        let mut fetched_posts = Vec::new();
        while let Some(post_result) = posts_futures.next().await {
            match post_result {
                Ok(post) => fetched_posts.push(post),
                Err(e) => {
                    tracing::warn!(
                        "A post failed to fetch/parse fully: {:?}. It will be skipped.",
                        e
                    );
                }
            }
        }

        tracing::info!("Successfully fetched {} full posts.", fetched_posts.len());
        Ok(fetched_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime; // Added for test_fetch_actual_post_content_and_details
    use tracing_subscriber::EnvFilter;

    fn setup_tracing_subscriber_for_tests() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env()
                    .add_directive("ssufid_chemeng=debug".parse().unwrap()),
            )
            .try_init();
    }

    #[tokio::test]
    async fn test_fetch_page1_metadata_successfully_and_parses_data() {
        setup_tracing_subscriber_for_tests();
        let plugin = ChemEngPlugin::new();
        let metadata_result = plugin.fetch_page_posts_metadata(1).await;

        match metadata_result {
            Ok(metadata) => {
                assert!(
                    !metadata.is_empty(),
                    "Should retrieve some metadata from page 1. If this fails, list page selectors are likely incorrect or the page structure has significantly changed."
                );

                let item = metadata.first().unwrap(); // Check the first item thoroughly
                tracing::info!("First metadata item from page 1: {:?}", item);
                assert!(!item.id.is_empty(), "ID must not be empty.");
                assert!(
                    item.url.starts_with(ChemEngPlugin::BASE_URL) && item.url.contains("idx="),
                    "URL should be absolute and contain 'idx'. URL: {}",
                    item.url
                );
                assert!(
                    !item.title_on_list.is_empty(),
                    "Title on list must not be empty."
                );
                assert!(
                    !item.author_on_list.is_empty(),
                    "Author on list must not be empty."
                );
                assert!(
                    !item.date_str_on_list.is_empty(),
                    "Date string on list must not be empty."
                );
                assert!(
                    Date::parse(&item.date_str_on_list, ChemEngPlugin::DATE_FORMAT_PARSE).is_ok(),
                    "Date string '{}' from list page is not parseable with format YYYY-MM-DD.",
                    item.date_str_on_list
                );
            }
            Err(e) => {
                panic!(
                    "Failed to fetch page 1 metadata: {e:?}. Check network connectivity, list page selectors, or if the website structure has changed."
                );
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_actual_post_content_and_details() {
        setup_tracing_subscriber_for_tests();
        let plugin = ChemEngPlugin::new();

        let metadata_list = plugin.fetch_page_posts_metadata(1).await
            .expect("Prerequisite for post content test: Failed to get metadata from page 1. Check list page selectors.");
        assert!(
            !metadata_list.is_empty(),
            "Prerequisite: Page 1 metadata list is empty. Cannot proceed to test post fetching."
        );

        // Try to find a non-announcement post, as they usually have more standard structure and 'idx'
        let sample_metadata = metadata_list.into_iter()
            .find(|m| !m.id.starts_with("notice_") && m.id.parse::<u32>().is_ok()) // Regular post with numeric idx
            .expect("Could not find a regular post (with numeric 'idx') on page 1 for detailed fetching test. Page might only contain announcements or 'idx' parsing failed.");

        let sample_metadata_id_for_error = sample_metadata.id.clone(); // Clone id for error reporting
        tracing::info!(
            "Attempting to fetch full content for post: ID='{}', URL='{}'",
            sample_metadata.id,
            sample_metadata.url
        );

        let post_result = plugin.fetch_post(sample_metadata).await; // sample_metadata is moved

        match post_result {
            Ok(post) => {
                tracing::info!(
                    "Successfully fetched Post: ID={}, Title='{}', Author='{:?}', Date='{}', Content Length={}",
                    post.id,
                    post.title,
                    post.author,
                    post.created_at,
                    post.content.len()
                );
                assert!(!post.id.is_empty(), "Fetched post ID should not be empty.");
                assert!(
                    !post.url.is_empty(),
                    "Fetched post URL should not be empty."
                );
                assert!(
                    !post.title.is_empty(),
                    "Fetched post title should not be empty. If empty, detail page title selector may be wrong."
                );
                let author = post.author.expect("Post author should be Some.");
                assert!(
                    !author.is_empty(),
                    "Post author string should not be empty."
                );
                assert!(
                    post.created_at.year() > 2000
                        && post.created_at.year() < OffsetDateTime::now_utc().year() + 5, // Allow some future leeway for announcements
                    "Post creation year ({}) seems unreasonable.",
                    post.created_at.year()
                );
                assert!(
                    !post.content.is_empty(),
                    "Post content should not be empty. If empty, detail page content selector ('{:?}') might be wrong or content is indeed empty.",
                    plugin.selectors.post_content
                );
            }
            Err(e) => {
                panic!(
                    "Failed to fetch the sample post (ID: {}): {:?}. Check detail page selectors ('title:{:?}', 'author_date_info:{:?}', 'content:{:?}'), network, or if the specific post structure is unusual.",
                    sample_metadata_id_for_error,
                    e,
                    plugin.selectors.post_title,
                    plugin.selectors.post_author_info, // Corrected to author_info
                    plugin.selectors.post_content,
                );
            }
        }
    }

    #[tokio::test]
    async fn test_get_total_pages_from_live_page() {
        setup_tracing_subscriber_for_tests();
        let plugin = ChemEngPlugin::new();
        let list_page_url = plugin.get_list_page_url(1);
        let response_text = plugin
            .client
            .get(list_page_url)
            .send()
            .await
            .expect("Network error fetching page 1 for total_pages test")
            .text()
            .await
            .expect("Text parsing error for page 1 total_pages test");
        let document = Html::parse_document(&response_text);

        let total_pages = plugin.get_total_pages_from_list_html(&document);
        tracing::info!(
            "Total pages reported by get_total_pages_from_list_html: {}",
            total_pages
        );
        assert!(
            total_pages > 0,
            "Total pages should be a positive number. If it's the fallback (e.g. 70), verify '페이지정보' parsing logic."
        );
        // Example: The site shows "1 / 69", so we expect around 69.
        assert!(
            (1..200).contains(&total_pages),
            "Total pages ({total_pages}) seems out of a reasonable range (expected e.g. 1-199). Check parsing."
        );
    }

    #[tokio::test]
    async fn test_crawl_limited_to_3_posts() {
        setup_tracing_subscriber_for_tests();
        let plugin = ChemEngPlugin::new();
        let limit = 3u32;
        let posts_result = plugin.crawl(limit).await;

        match posts_result {
            Ok(posts) => {
                tracing::info!("Crawl with limit {} returned {} posts.", limit, posts.len());
                assert!(
                    posts.len() <= limit as usize,
                    "Number of crawled posts ({}) should not exceed the limit ({}).",
                    posts.len(),
                    limit
                );

                if posts.is_empty() && limit > 0 {
                    tracing::warn!(
                        "Crawl with limit {} returned NO posts. This could be due to selector issues, no posts on the site, or all posts being filtered out before full fetch.",
                        limit
                    );
                } else if limit > 0 {
                    assert_eq!(
                        posts.len(),
                        limit as usize,
                        "Expected exactly {limit} posts for limit {limit} when site has enough posts."
                    );
                }
                for post in &posts {
                    assert!(!post.id.is_empty(), "Crawled post should have an ID.");
                    assert!(
                        !post.title.is_empty(),
                        "Crawled post (ID: {}) should have a title.",
                        post.id
                    );
                    tracing::info!(?post);
                }
            }
            Err(e) => {
                panic!(
                    "Crawl with limit {limit} failed: {e:?}. This could indicate a problem with fetching metadata, individual posts, or logic in the crawl loop."
                );
            }
        }
    }

    #[tokio::test]
    async fn test_crawl_with_limit_0_returns_empty_vec() {
        setup_tracing_subscriber_for_tests();
        let plugin = ChemEngPlugin::new();
        let limit = 0u32;
        let posts_result = plugin.crawl(limit).await;
        match posts_result {
            Ok(posts) => {
                assert!(
                    posts.is_empty(),
                    "Crawl with limit 0 should return an empty vector, but got {} posts.",
                    posts.len()
                );
            }
            Err(e) => {
                panic!("Crawl with limit 0 failed unexpectedly: {e:?}");
            }
        }
    }
}
