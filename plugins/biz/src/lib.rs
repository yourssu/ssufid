use futures::{TryStreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector};
use thiserror::Error;
use url::Url;

use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{
    Date,
    macros::{format_description, offset},
};

// Selectors updated based on previous HTML analysis during the temp_fetcher step
struct Selectors {
    notice_items: Selector,
    post_link_and_title: Selector,
    date_author: Selector, // Covers combined date/author string

    title_detail: Selector,
    content_detail: Selector,
    attachments_container: Selector,
    attachment_item: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // --- List Page (Based on biz.ssu.ac.kr structure from temp_fetcher) ---
            notice_items: Selector::parse("ul#bList01 li")
                .expect("Failed to parse notices_item selector"),
            post_link_and_title: Selector::parse("div > a")
                .expect("Failed to parse post_link_and_title selector"),
            date_author: Selector::parse("div:nth-of-type(2) > span") // Selector for the "date / author" string
                .expect("Failed to parse date_list selector"),

            // --- Detail Page (Based on biz.ssu.ac.kr structure from temp_fetcher) ---
            title_detail: Selector::parse("div#postTitle > span")
                .expect("Failed to parse title_detail selector"),
            content_detail: Selector::parse("div#postContents")
                .expect("Failed to parse content_detail selector"),
            attachments_container: Selector::parse("ul#postFileList")
                .expect("Failed to parse attachments_container selector"),
            attachment_item: Selector::parse("li > a")
                .expect("Failed to parse attachment_item selector"),
        }
    }
}

#[derive(Debug, Clone)]
struct BizMetadata {
    id: String,
    url: String,
    date_str: String,
    author: String,
}

#[derive(Debug, Error)]
enum BizScrapingError {
    #[error("List page: Post link not found in list item (li > div > a)")]
    LinkNotFound,
    // TitleNotFound commented out as empty titles on list are handled with a placeholder
    // #[error("List page: Post title not found in list item")]
    // TitleNotFound,
    #[error("List page: Post ID (aId/seq) not found in URL: {0}")]
    IdParamMissing(String),
    #[error("List page: Date/Author string not found for post")]
    DateAuthorStringMissingList,
    // AuthorParseErrorList commented out as author is optional and parsing combined string
    // #[error("List page: Could not parse author from date/author string: '{0}'")]
    // AuthorParseErrorList(String),
    #[error("Detail page: Title (div#postTitle > span.fixedPost) not found for URL: {0}")]
    TitleNotFoundDetail(String),
    #[error("Detail page: Content (ul#postContent) not found for URL: {0}")]
    ContentNotFoundDetail(String),
    #[error("Detail page: Date parsing error '{date_str}': {source}")]
    DateParseErrorDetail {
        date_str: String,
        source: time::error::Parse,
    },
    #[error("Detail page: Could not parse date from detail string: {0}")]
    DateExtractionErrorDetail(String),
    // AuthorExtractionErrorDetail commented out as author is optional from detail page
    // #[error("Detail page: Could not parse author from detail string: {0}")]
    // AuthorExtractionErrorDetail(String),
}

pub struct BizPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl Default for BizPlugin {
    fn default() -> Self {
        Self::new()
    }
}

const DATE_FORMAT_BIZ: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]"); // format_description! macro is brought into scope by the use statement above

fn parse_date_author_string(s: &str) -> Result<(String, String), PluginError> {
    let parts: Vec<&str> = s.splitn(2, '/').map(str::trim).collect();
    let date_str = parts
        .first()
        .map(|x| x.to_string())
        .ok_or(PluginError::parse::<BizPlugin>(
            BizScrapingError::DateExtractionErrorDetail(s.to_string()).to_string(),
        ))?;
    let author_str = parts
        .get(1)
        .map(|x| x.to_string())
        .ok_or(PluginError::parse::<BizPlugin>(
            BizScrapingError::DateExtractionErrorDetail(s.to_string()).to_string(),
        ))?;
    Ok((date_str, author_str))
}

impl BizPlugin {
    const BIZ_BASE_URL: &'static str = "http://biz.ssu.ac.kr";

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap(),
        }
    }

    async fn fetch_page_posts_metadata(&self, page: u32) -> Result<Vec<BizMetadata>, PluginError> {
        let list_url = format!(
            "{}/bbs/list.do?bId=BBS_03_NOTICE&page={}",
            Self::BIZ_BASE_URL,
            page
        );

        tracing::debug!("Fetching metadata from: {}", list_url);

        let response_text = self
            .http_client
            .get(&list_url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);
        let mut metadata_list = Vec::new();

        for item_li in document.select(&self.selectors.notice_items) {
            let link_element = match item_li.select(&self.selectors.post_link_and_title).next() {
                Some(el) => el,
                None => {
                    if item_li // Clippy: Use .is_some_and()
                        .value()
                        .attr("class")
                        .is_some_and(|c| c.contains("paging"))
                    {
                        tracing::trace!("Skipping pagination li element");
                    } else {
                        tracing::warn!(
                            "Skipping li item without a link/title element: {:?}",
                            item_li.html()
                        );
                    }
                    continue;
                }
            };

            let relative_url = link_element.value().attr("href").ok_or_else(|| {
                PluginError::parse::<Self>(BizScrapingError::LinkNotFound.to_string())
            })?;

            let base_url_for_join = Url::parse(Self::BIZ_BASE_URL).unwrap();
            let full_url = base_url_for_join
                .join(relative_url)
                .map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "URL join error for '{relative_url}' with base '{base_url_for_join}': {e}",
                    ))
                })?
                .to_string();

            let parsed_url = Url::parse(&full_url).map_err(|e| {
                PluginError::parse::<Self>(format!("URL re-parse error for '{full_url}': {e}"))
            })?;

            let id = parsed_url
                .query_pairs()
                .find_map(|(key, value)| {
                    if key == "aId" || key == "seq" {
                        Some(value.into_owned())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    PluginError::parse::<Self>(
                        BizScrapingError::IdParamMissing(full_url.clone()).to_string(),
                    )
                })?;

            let title_on_list = link_element.text().collect::<String>().trim().to_string();
            if title_on_list.is_empty() && relative_url.contains("javascript:void(0)") {
                tracing::debug!(
                    "Skipping item with empty title and javascript void link: {}",
                    relative_url
                );
                continue;
            }
            if title_on_list.is_empty() {
                tracing::warn!(
                    "Found empty title for URL: {}. Using placeholder.",
                    full_url
                );
            }

            let date_author_str_element = item_li
                .select(&self.selectors.date_author)
                .next()
                .ok_or_else(|| {
                    PluginError::parse::<Self>(
                        BizScrapingError::DateAuthorStringMissingList.to_string(),
                    )
                })?;

            let date_author_str = date_author_str_element
                .text()
                .collect::<String>()
                .trim()
                .to_string();
            let (date_str, author) = parse_date_author_string(&date_author_str)?;

            metadata_list.push(BizMetadata {
                id,
                url: full_url,
                date_str,
                author,
            });
        }
        Ok(metadata_list)
    }

    async fn fetch_post(&self, post_metadata: &BizMetadata) -> Result<SsufidPost, PluginError> {
        tracing::debug!("Fetching post content from: {}", post_metadata.url);
        let response_text = self
            .http_client
            .get(&post_metadata.url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);

        let title = document
            .select(&self.selectors.title_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty()) // Ensure title is not empty
            .ok_or_else(|| {
                PluginError::parse::<Self>(
                    BizScrapingError::TitleNotFoundDetail(post_metadata.url.clone()).to_string(),
                )
            })?;

        let created_at = Date::parse(&post_metadata.date_str, &DATE_FORMAT_BIZ)
            .map_err(|e| {
                PluginError::parse::<Self>(
                    BizScrapingError::DateParseErrorDetail {
                        date_str: post_metadata.date_str.clone(),
                        source: e,
                    }
                    .to_string(),
                )
            })?
            .midnight()
            .assume_offset(offset!(+9));

        let content_html = document
            .select(&self.selectors.content_detail)
            .next()
            .map(|el| el.html())
            .ok_or_else(|| {
                PluginError::parse::<Self>(
                    BizScrapingError::ContentNotFoundDetail(post_metadata.url.clone()).to_string(),
                )
            })?;

        let mut attachments = Vec::new();
        if let Some(container) = document
            .select(&self.selectors.attachments_container)
            .next()
        {
            for item_a in container.select(&self.selectors.attachment_item) {
                if let Some(href) = item_a.value().attr("href") {
                    let attachment_base =
                        Url::parse(&format!("{}/bbs/", Self::BIZ_BASE_URL)).unwrap();
                    let attachment_url = attachment_base
                        .join(href)
                        .map_err(|e| {
                            PluginError::parse::<Self>(format!(
                                "Attachment URL join error for '{href}' with base '{attachment_base}': {e}"
                            ))
                        })?
                        .to_string();

                    let name = item_a.text().collect::<String>().trim().to_string();
                    attachments.push(Attachment {
                        name: Some(name),
                        url: attachment_url,
                        mime_type: None,
                    });
                }
            }
        }

        Ok(SsufidPost {
            id: post_metadata.id.clone(),
            url: post_metadata.url.clone(),
            author: Some(post_metadata.author.clone()),
            title,
            description: None,
            category: vec![],
            created_at,
            updated_at: None,
            thumbnail: None,
            content: content_html,
            attachments,
            metadata: None,
        })
    }
}

impl SsufidPlugin for BizPlugin {
    const IDENTIFIER: &'static str = "biz.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 경영학부 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 경영학부 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = BizPlugin::BIZ_BASE_URL;

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        if posts_limit == 0 {
            return Ok(vec![]);
        }

        let mut all_metadata: Vec<BizMetadata> = Vec::new();
        let mut current_page = 1;

        loop {
            if all_metadata.len() >= posts_limit as usize {
                break;
            }

            tracing::info!(
                "Fetching metadata from page {} for plugin '{}'. Current metadata count: {}, Target: {}",
                current_page,
                Self::IDENTIFIER,
                all_metadata.len(),
                posts_limit
            );

            let page_metadata = match self.fetch_page_posts_metadata(current_page).await {
                Ok(md) => md,
                Err(e) => {
                    tracing::error!("Failed to fetch metadata from page {}: {}", current_page, e);
                    return Err(e);
                }
            };

            if page_metadata.is_empty() {
                tracing::info!("No more metadata found on page {}. Stopping.", current_page);
                break;
            }

            for meta_item in page_metadata {
                if all_metadata.len() < posts_limit as usize {
                    all_metadata.push(meta_item);
                } else {
                    break;
                }
            }

            current_page += 1;
            if current_page > 50 {
                tracing::warn!(
                    "Reached page limit (50) during metadata fetch. Consider refining pagination logic or increasing limit."
                );
                break;
            }
        }

        tracing::info!(
            "Fetched {} metadata items. Now fetching full posts.",
            all_metadata.len()
        );

        let post_futures = all_metadata
            .iter()
            .map(|metadata| self.fetch_post(metadata))
            .collect::<FuturesOrdered<_>>();

        let posts = post_futures.try_collect::<Vec<SsufidPost>>().await?;

        tracing::info!(
            "Successfully crawled {} posts for '{}'.",
            posts.len(),
            Self::IDENTIFIER
        );
        Ok(posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[tokio::test]
    async fn test_plugin_creation() {
        let _plugin = BizPlugin::new();
        assert_eq!(BizPlugin::IDENTIFIER, "biz.ssu.ac.kr");
    }

    #[test]
    fn test_selectors_creation() {
        let _selectors = Selectors::new();
    }

    #[test]
    fn test_parse_date_author_string() {
        let (date, author) = parse_date_author_string("2024-07-30 / 경영학부").unwrap();
        assert_eq!(date, "2024-07-30".to_string());
        assert_eq!(author, "경영학부".to_string());
    }

    #[traced_test]
    #[tokio::test]
    async fn test_fetch_one_page_metadata() {
        let plugin = BizPlugin::new();
        match plugin.fetch_page_posts_metadata(1).await {
            Ok(metadata_list) => {
                // It's possible the page is empty if it's a holiday or no notices,
                // but generally, there should be notices.
                // If this fails often, the site structure might have changed or there are no notices.
                assert!(
                    !metadata_list.is_empty(),
                    "Metadata list should ideally not be empty for page 1."
                );
                if let Some(first_meta) = metadata_list.first() {
                    // Clippy: Use .first()
                    assert!(!first_meta.id.is_empty(), "ID should not be empty");
                    assert!(
                        first_meta.url.starts_with("http"),
                        "URL should be valid and absolute"
                    );
                    // title_on_list and date_on_list_str removed from BizMetadata, so assertions removed.
                    // Author can be optional, so no direct assert for !is_empty() on author_on_list
                }
                tracing::info!(
                    "Fetched {} metadata items from page 1.",
                    metadata_list.len()
                );
            }
            Err(e) => {
                // Provide more context on failure
                let response =
                    reqwest::get("http://biz.ssu.ac.kr/bbs/list.do?bId=BBS_03_NOTICE&page=1").await;
                let status_and_body = match response {
                    Ok(r) => format!(
                        "Status: {}. Body: {:.500}",
                        r.status(),
                        r.text().await.unwrap_or_default()
                    ),
                    Err(re) => format!("Request error: {re}"),
                };
                panic!(
                    "fetch_page_posts_metadata failed: {e}\nResponse details: {status_and_body}"
                );
            }
        }
    }

    #[traced_test]
    #[tokio::test]
    async fn test_fetch_one_post() {
        let plugin = BizPlugin::new();
        // Fetch metadata first to get a valid post to test
        let metadata_list = plugin
            .fetch_page_posts_metadata(1)
            .await
            .expect("Failed to get metadata for post fetching test");
        assert!(
            !metadata_list.is_empty(),
            "Need at least one metadata item to test fetch_post. Site might be empty or list parsing failed."
        );

        // Take the first post for testing.
        // title_on_list was removed, so direct clone of first item.
        let first_metadata = metadata_list[0].clone();

        tracing::info!(
            "Attempting to fetch post with metadata: {:?}",
            first_metadata
        );

        match plugin.fetch_post(&first_metadata).await {
            Ok(post) => {
                assert!(
                    !post.title.is_empty(),
                    "Post title should not be empty. Parsed from: {}",
                    first_metadata.url
                );
                assert!(
                    !post.content.is_empty(),
                    "Post content should not be empty. Parsed from: {}",
                    first_metadata.url
                );
                assert!(
                    post.created_at.year() >= 2020,
                    "Post date (year {}) seems too old or invalid. Parsed from: {}",
                    post.created_at.year(),
                    first_metadata.url
                );
                tracing::info!("Fetched post successfully: '{}'", post.title);
            }
            Err(e) => {
                let response = reqwest::get(&first_metadata.url).await;
                let status_and_body = match response {
                    Ok(r) => format!(
                        "Status: {}. Body: {:.500}",
                        r.status(),
                        r.text().await.unwrap_or_default()
                    ),
                    Err(re) => format!("Request error: {re}"),
                };
                panic!(
                    "fetch_post for '{}' failed: {}\nResponse details: {}",
                    first_metadata.url, e, status_and_body
                );
            }
        }
    }

    #[traced_test]
    #[tokio::test]
    async fn test_crawl_few_posts() {
        let plugin = BizPlugin::new();
        let limit = 2; // Request a small number of posts
        match plugin.crawl(limit).await {
            Ok(posts) => {
                assert!(
                    posts.len() <= limit as usize,
                    "Returned more posts ({}) than limit ({})",
                    posts.len(),
                    limit
                );
                // This assertion might fail if the site has less than `limit` posts.
                // For a notice board, it's usually expected to have at least a few.
                assert!(
                    !posts.is_empty(),
                    "Crawl returned no posts, expected at least 1 (up to limit of {limit})."
                );
                tracing::info!("Crawled {} posts successfully.", posts.len());
            }
            Err(e) => {
                panic!("crawl(limit={limit}) failed: {e}");
            }
        }
    }
}
