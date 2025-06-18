use futures::{StreamExt as _, stream::FuturesOrdered};
use scraper::{ElementRef, Html, Selector};
use thiserror::Error;
use time::{
    Date,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
}; // Removed unused Rfc3339
use url::Url; // Added OffsetDateTime

// Use actual package name 'ssufid' and correct module path
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};

struct Selectors {
    post_row: Selector,
    post_link_and_title: Selector,
    metadata_list: Selector,
    view_content: Selector,
    attachment_links: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // For list page:
            // Table rows are under form name="board_list", then table > tbody > tr
            post_row: Selector::parse("div.board_list > table > tbody > tr")
                .expect("Failed to parse post_row selector"),
            // Title is in the 3rd td, contains an <a> tag with mode=view in href.
            post_link_and_title: Selector::parse("td.subject > a")
                .expect("Failed to parse post_link_and_title selector"),
            metadata_list: Selector::parse("div.board_view > dl")
                .expect("Failed to parse metadata_list selector"),
            view_content: Selector::parse("div.view_content")
                .expect("Failed to parse view_content selector"),
            attachment_links: Selector::parse("a[title='첨부파일 내려받기(새창열림)")
                .expect("Failed to parse attachment_links selector"),
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

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

impl InsoPlugin {
    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::new(),
        }
    }

    async fn fetch_page_posts_metadata(
        &self,
        page_offset: u32,
    ) -> Result<Vec<InsoPostMetadata>, PluginError> {
        let list_url = format!(
            "{}?boardid=notice&category=&mode=list&offset={}",
            Self::BASE_URL,
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
                let relative_url = link_element.value().attr("href").ok_or_else(|| {
                    PluginError::parse::<Self>(
                        "Failed to find 'href' attribute in post link".to_string(),
                    )
                })?;

                let absolute_post_url = format!("http://inso.ssu.ac.kr{}", relative_url);

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

        let mut metadata = document.select(&self.selectors.metadata_list);

        fn extract_str(elem: Option<ElementRef<'_>>) -> Result<String, PluginError> {
            Ok(elem
                .ok_or_else(|| {
                    PluginError::parse::<InsoPlugin>(
                        "No title found in the post content".to_string(),
                    )
                })?
                .child_elements()
                .last()
                .ok_or_else(|| {
                    PluginError::parse::<InsoPlugin>(
                        "No title found in the post content".to_string(),
                    )
                })?
                .text()
                .collect::<String>()
                .trim()
                .to_string())
        }

        let title = extract_str(metadata.next())
            .map_err(|e| PluginError::parse::<Self>(format!("Failed to extract title: {}", e)))?;

        let author = extract_str(metadata.next())
            .map_err(|e| PluginError::parse::<Self>(format!("Failed to extract author: {}", e)))?;

        let date_str = extract_str(metadata.next())
            .map_err(|e| PluginError::parse::<Self>(format!("Failed to extract date: {}", e)))?;

        let date = Date::parse(&date_str, DATE_FORMAT)
            .map_err(|e| PluginError::parse::<Self>(format!("Failed to parse date: {e:?}")))?
            .midnight()
            .assume_offset(offset!(+09:00));

        let category = extract_str(metadata.nth(1)).map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to extract category: {}", e))
        })?;

        let mut attachments = Vec::new();

        for link_element in document.select(&self.selectors.attachment_links) {
            if let Some(href) = link_element.value().attr("href") {
                let attachment_name = link_element.text().collect::<String>().trim().to_string();
                let mut script_parsed = href
                    .split("('")
                    .nth(1)
                    .and_then(|s| s.split("')").next())
                    .ok_or_else(|| {
                        PluginError::parse::<Self>("Failed to parse attachment URL".to_string())
                    })?
                    .split("','");
                let board_id = script_parsed.next().ok_or_else(|| {
                    PluginError::parse::<Self>(
                        "Failed to parse board ID from attachment URL".to_string(),
                    )
                })?;
                let b_idx = script_parsed.next().ok_or_else(|| {
                    PluginError::parse::<Self>(
                        "Failed to parse b_idx from attachment URL".to_string(),
                    )
                })?;
                let idx = script_parsed.next().ok_or_else(|| {
                    PluginError::parse::<Self>(
                        "Failed to parse idx from attachment URL".to_string(),
                    )
                })?;
                attachments.push(Attachment {
                    name: Some(attachment_name).filter(|s| !s.is_empty()),
                    url: format!("http://inso.ssu.ac.kr/module/board/download.php?boardid={board_id}&b_idx={b_idx}&idx={idx}"),
                    mime_type: None,
                });
            }
        }

        let content = document
            .select(&self.selectors.view_content)
            .next()
            .map(|el| el.inner_html())
            .ok_or_else(|| {
                PluginError::parse::<Self>("Failed to find content in the post".to_string())
            })?;

        Ok(SsufidPost {
            id: post_metadata.id.clone(),
            url: post_metadata.url.clone(),
            title,
            author: Some(author).filter(|s| !s.is_empty()),
            description: None,
            category: vec![category],
            created_at: date,
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
    const BASE_URL: &'static str = "http://inso.ssu.ac.kr/sub/sub04_01.php";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        if posts_limit == 0 {
            tracing::debug!("posts_limit is 0, returning empty vector.");
            return Ok(Vec::new());
        }

        tracing::info!(
            "Starting crawl for plugin: {},  posts_limit: {}",
            Self::IDENTIFIER,
            posts_limit
        );

        let mut all_collected_metadata = Vec::new();
        let mut current_offset = 0;
        const POSTS_PER_PAGE: u32 = 10;

        loop {
            tracing::debug!("Fetching metadata page with offset: {}", current_offset);
            let metadata_from_page = self.fetch_page_posts_metadata(current_offset).await?;

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
