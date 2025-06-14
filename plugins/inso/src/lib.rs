use futures::{StreamExt as _, stream::FuturesOrdered};
use scraper::{Html, Selector};
use thiserror::Error;
use time::{Date, OffsetDateTime, macros::offset}; // Removed unused Rfc3339
use url::Url; // Added OffsetDateTime

// Use actual package name 'ssufid' and correct module path
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};

struct Selectors {
    post_row: Selector,
    post_link_and_title: Selector,
    view_table_rows: Selector,
    view_content_container: Selector,
    view_attachments_container: Selector,
    view_attachment_link: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // For list page:
            // Table rows are under form name="board_list", then table > tbody > tr
            post_row: Selector::parse("form[name='board_list'] > table > tbody > tr")
                .expect("Failed to parse post_row selector"),
            // Title is in the 3rd td, contains an <a> tag with mode=view in href.
            post_link_and_title: Selector::parse("td:nth-child(3) > a[href*='mode=view']")
                .expect("Failed to parse post_link_and_title selector"),
            // For post view page:
            // Table rows containing metadata like title, author, date.
            view_table_rows: Selector::parse("form[name='board_view_frm'] > table > tbody > tr")
                .expect("Failed to parse view_table_rows selector"),
            // Main content container for the post body.
            view_content_container: Selector::parse("td.content")
                .expect("Failed to parse view_content_container selector"),
            // Container for attachment links (speculative).
            view_attachments_container: Selector::parse("div.attach")
                .expect("Failed to parse view_attachments_container selector"),
            // Individual attachment links (speculative).
            view_attachment_link: Selector::parse("a[href*='board_download']")
                .expect("Failed to parse view_attachment_link selector"),
        }
    }
}

#[derive(Debug)]
struct InsoPostMetadata {
    id: String,
    url: String,
}

#[derive(Debug, Error)]
enum InsoPluginError {
    #[error("URL parsing error: {0}")]
    UrlParseError(String),
}

pub struct InsoPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl Default for InsoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl InsoPlugin {
    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::new(),
        }
    }

    async fn fetch_page_posts_metadata(
        &self,
        category_param: &str,
        page_offset: u32,
    ) -> Result<Vec<InsoPostMetadata>, PluginError> {
        let list_url = format!(
            "{}/sub/sub04_01.php?boardid=notice&category={}&mode=list&offset={}",
            Self::BASE_URL,
            category_param,
            page_offset
        );

        tracing::info!(url = %list_url, "Fetching post metadata list");

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

        for row_element in document.select(&self.selectors.post_row) {
            if let Some(link_element) = row_element
                .select(&self.selectors.post_link_and_title)
                .next()
            {
                let relative_url = match link_element.value().attr("href") {
                    Some(href) => href.to_string(),
                    None => {
                        tracing::warn!("Found a post row without a link. Skipping.");
                        continue;
                    }
                };

                let base_path_for_relative_url = Url::parse(&list_url)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Base URL for list failed to parse: {}",
                            e
                        ))
                    })?
                    .join("./")
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to establish base path for relative URLs: {}",
                            e
                        ))
                    })?;

                let absolute_post_url = base_path_for_relative_url
                    .join(&relative_url)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to join post URL: {}. Base: {}, Relative: {}",
                            e, base_path_for_relative_url, relative_url
                        ))
                    })?
                    .to_string();

                let parsed_url = Url::parse(&absolute_post_url)
                    .map_err(|e| InsoPluginError::UrlParseError(e.to_string()))
                    .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

                let post_id = parsed_url
                    .query_pairs()
                    .find_map(|(key, value)| {
                        if key == "idx" {
                            Some(value.into_owned())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| {
                        PluginError::parse::<Self>(format!(
                            "Could not find 'idx' in post URL: {}",
                            absolute_post_url
                        ))
                    })?;

                metadata_list.push(InsoPostMetadata {
                    id: post_id,
                    url: absolute_post_url,
                });
            }
        }

        tracing::info!(
            "Found {} metadata items from {}",
            metadata_list.len(),
            list_url
        );
        Ok(metadata_list)
    }

    async fn fetch_post(
        &self,
        post_metadata: InsoPostMetadata, // Take by value
    ) -> Result<SsufidPost, PluginError> {
        tracing::info!(url = %post_metadata.url, id = %post_metadata.id, "Fetching post content");

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

        let mut title = String::new();
        let mut author = String::new();
        let mut date_str = String::new();
        let mut category_str = String::new();

        for row_element in document.select(&self.selectors.view_table_rows) {
            let th_texts: Vec<String> = row_element
                .select(&Selector::parse("th").unwrap())
                .map(|el| el.text().collect::<String>().trim().to_lowercase())
                .collect();
            let td_texts: Vec<String> = row_element
                .select(&Selector::parse("td").unwrap())
                .map(|el| el.text().collect::<String>().trim().to_string())
                .collect();

            if let Some(th_text) = th_texts.first() {
                if let Some(td_text) = td_texts.first() {
                    match th_text.as_str() {
                        "제목" => title = td_text.clone(),
                        "작성자" => author = td_text.clone(),
                        "작성일자" => date_str = td_text.clone(),
                        "분류" => category_str = td_text.clone(),
                        _ => {}
                    }
                }
            }
        }

        if title.is_empty() {
            tracing::warn!(url = %post_metadata.url, "Title not found using view_table_rows selector. Attempting fallback based on text output structure.");
        }

        let created_at = if !date_str.is_empty() {
            let date_format =
                time::format_description::parse("[year]-[month]-[day]").map_err(|e| {
                    PluginError::parse::<Self>(format!("Failed to parse date format: {}", e))
                })?;
            Date::parse(&date_str, &date_format)
                .map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Failed to parse date string '{}': {}",
                        date_str, e
                    ))
                })?
                .midnight()
                .assume_offset(offset!(+9)) // KST
        } else {
            tracing::warn!(url = %post_metadata.url, "Date string is empty. Defaulting to current time (this might be incorrect).");
            OffsetDateTime::now_utc().to_offset(offset!(+9))
        };

        let content = document
            .select(&self.selectors.view_content_container)
            .next()
            .map_or_else(
                || {
                    tracing::warn!(url = %post_metadata.url, "Content container 'td.content' not found. Content will be empty.");
                    String::new()
                },
                |el| el.inner_html()
            );

        let mut attachments = Vec::new();
        if let Some(attachment_container) = document
            .select(&self.selectors.view_attachments_container)
            .next()
        {
            for link_element in attachment_container.select(&self.selectors.view_attachment_link) {
                if let Some(href) = link_element.value().attr("href") {
                    let attachment_name =
                        link_element.text().collect::<String>().trim().to_string();
                    let attachment_url = Url::parse(Self::BASE_URL)
                        .map_err(|e| {
                            PluginError::parse::<Self>(format!(
                                "Failed to parse BASE_URL for attachment: {}",
                                e
                            ))
                        })?
                        .join(href)
                        .map_err(|e| {
                            PluginError::parse::<Self>(format!(
                                "Failed to join attachment URL: {}",
                                e
                            ))
                        })?
                        .to_string();
                    attachments.push(Attachment {
                        name: Some(attachment_name).filter(|s| !s.is_empty()),
                        url: attachment_url,
                        mime_type: None,
                    });
                }
            }
        } else {
            tracing::debug!(url = %post_metadata.url, "Attachment container 'div.attach' not found. Assuming no attachments via this selector.");
        }

        Ok(SsufidPost {
            id: post_metadata.id.clone(),
            url: post_metadata.url.clone(),
            title,
            author: Some(author).filter(|s| !s.is_empty()),
            description: None,
            category: if category_str.is_empty() {
                vec![]
            } else {
                vec![category_str]
            },
            created_at,
            updated_at: None,
            thumbnail: None,
            content,
            attachments,
            metadata: None,
        })
    }
}

impl SsufidPlugin for InsoPlugin {
    const IDENTIFIER: &'static str = "inso.ssu.ac.kr";
    const TITLE: &'static str = "정보사회학과 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 정보사회학과 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://inso.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        if posts_limit == 0 {
            tracing::debug!("posts_limit is 0, returning empty vector.");
            return Ok(Vec::new());
        }

        let category_param = "%ED%95%99%EC%83%9D%EA%B3%B5%EC%A7%80";

        tracing::info!(
            "Starting crawl for plugin: {}, category: {}, posts_limit: {}",
            Self::IDENTIFIER,
            category_param,
            posts_limit
        );

        let mut all_collected_metadata = Vec::new();
        let mut current_offset = 0;
        const POSTS_PER_PAGE: u32 = 10;

        loop {
            tracing::debug!("Fetching metadata page with offset: {}", current_offset);
            let metadata_from_page = self
                .fetch_page_posts_metadata(category_param, current_offset)
                .await?;

            if metadata_from_page.is_empty() {
                tracing::info!(
                    "No more metadata found at offset {}. Stopping metadata collection.",
                    current_offset
                );
                break;
            }

            let was_last_page_from_source = metadata_from_page.len() < POSTS_PER_PAGE as usize;

            for metadata_item in metadata_from_page {
                if all_collected_metadata.len() < posts_limit as usize {
                    all_collected_metadata.push(metadata_item);
                } else {
                    break;
                }
            }

            if all_collected_metadata.len() >= posts_limit as usize {
                tracing::info!(
                    "Reached posts_limit ({}). Stopping metadata collection.",
                    posts_limit
                );
                break;
            }

            if was_last_page_from_source {
                tracing::info!(
                    "Fetched a page with fewer posts than posts_per_page ({} < {}), indicating it's the last page from source. Stopping metadata collection.",
                    all_collected_metadata.len() % (POSTS_PER_PAGE as usize),
                    POSTS_PER_PAGE
                );
                break;
            }

            current_offset += POSTS_PER_PAGE;
        }

        tracing::info!(
            "Collected {} metadata items. Now fetching full post details.",
            all_collected_metadata.len()
        );

        if all_collected_metadata.is_empty() {
            return Ok(Vec::new());
        }

        let mut fetch_tasks = FuturesOrdered::new();
        for meta in all_collected_metadata {
            fetch_tasks.push_back(self.fetch_post(meta)); // Pass by value
        }

        let mut all_posts = Vec::with_capacity(fetch_tasks.len());
        while let Some(post_result) = fetch_tasks.next().await {
            match post_result {
                Ok(post) => all_posts.push(post),
                Err(e) => {
                    tracing::error!(
                        "Failed to fetch individual post: {:?}. Skipping this post.",
                        e
                    );
                }
            }
        }

        tracing::info!("Successfully fetched {} posts.", all_posts.len());
        Ok(all_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_tracing() {
        let _ = tracing_subscriber::fmt::try_init();
    }

    #[tokio::test]
    async fn test_selectors_parse() {
        setup_tracing();
        let _selectors = Selectors::new();
    }
}
