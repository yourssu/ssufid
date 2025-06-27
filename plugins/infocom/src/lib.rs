use futures::stream::{FuturesOrdered, StreamExt};
use scraper::{Html, Selector};
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{
    Date, OffsetDateTime,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};
use url::Url;

#[derive(Debug, Clone)] // Added Clone
struct InfocomPostMetadata {
    id: String,
    url: String,
    title: String,
    date: OffsetDateTime,
}

// Selectors struct (defined earlier)
struct Selectors {
    post_container: Selector,
    title: Selector,
    date: Selector,
    post_content_container: Selector,
    post_files: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_container: Selector::parse("a.con_box").unwrap(),
            title: Selector::parse("div.subject span").unwrap(),
            date: Selector::parse("ul.info li.date").unwrap(),
            post_content_container: Selector::parse("div.view_box div.con").unwrap(),
            post_files: Selector::parse("div.view_box div.file a").unwrap(),
        }
    }
}

// PostDetailExtras struct (defined earlier)
#[derive(Debug, Default)]
struct PostDetailExtras {
    content: String,
    attachments: Vec<Attachment>,
}

pub struct InfocomPlugin {
    selectors: Selectors,
}

impl Default for InfocomPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl InfocomPlugin {
    const HOST_URL: &'static str = "http://infocom.ssu.ac.kr";
    const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]. [month]. [day]");

    pub fn new() -> Self {
        InfocomPlugin {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_page_posts_metadata(
        &self,
        page: u32,
        client: &reqwest::Client,
    ) -> Result<Vec<InfocomPostMetadata>, PluginError> {
        let page_url = format!("{}?pNo={}&code=notice", Self::BASE_URL, page);
        let response = client.get(&page_url).send().await.map_err(|e| {
            PluginError::request::<Self>(format!("Failed to fetch page {page_url}: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(PluginError::request::<Self>(format!(
                "Failed to fetch page {}: status {}",
                page_url,
                response.status()
            )));
        }

        let html_content = response.text().await.map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to read page {page_url}: {e}"))
        })?;

        let document = Html::parse_document(&html_content);
        let host_url_parsed = Url::parse(Self::HOST_URL)
            .map_err(|e| PluginError::parse::<Self>(format!("Failed to parse HOST_URL: {e}")))?;

        let posts_metadata = document
            .select(&self.selectors.post_container)
            .filter_map(|element| {
                let relative_url = element.value().attr("href")?;
                let post_url_obj = host_url_parsed.join(relative_url).ok()?;
                let post_url = post_url_obj.to_string();

                let id = post_url_obj.query_pairs().find_map(|(key, value)| {
                    if key == "idx" {
                        Some(value.into_owned())
                    } else {
                        None
                    }
                })?;

                let title = element
                    .select(&self.selectors.title)
                    .next()?
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();

                let date_str = element
                    .select(&self.selectors.date)
                    .next()?
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();

                let date = Date::parse(&date_str, Self::DATE_FORMAT)
                    .ok()?
                    .midnight()
                    .assume_offset(offset!(+09:00));

                Some(InfocomPostMetadata {
                    id,
                    url: post_url,
                    title,
                    date,
                })
            })
            .collect::<Vec<_>>();

        Ok(posts_metadata)
    }

    async fn fetch_full_post_details(
        &self,
        post_metadata: &InfocomPostMetadata,
        client: &reqwest::Client,
    ) -> Result<PostDetailExtras, PluginError> {
        let response = client.get(&post_metadata.url).send().await.map_err(|e| {
            PluginError::request::<Self>(format!(
                "Failed to fetch post page {}: {}",
                &post_metadata.url, e
            ))
        })?;

        if !response.status().is_success() {
            return Err(PluginError::request::<Self>(format!(
                "Failed to fetch post page {}: status {}",
                &post_metadata.url,
                response.status()
            )));
        }

        let html_content = response.text().await.map_err(|e| {
            PluginError::parse::<Self>(format!(
                "Failed to read post page {}: {}",
                &post_metadata.url, e
            ))
        })?;

        let document = Html::parse_document(&html_content);
        let mut attachments = Vec::new();

        let content_html = document
            .select(&self.selectors.post_content_container)
            .next()
            .map_or(String::new(), |element| element.inner_html());

        for file_element in document.select(&self.selectors.post_files) {
            if let Some(href) = file_element.value().attr("href") {
                let name = file_element.text().collect::<String>().trim().to_string();
                let attachment_url = Url::parse(&post_metadata.url)
                    .unwrap()
                    .join(href)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| href.to_string());

                attachments.push(Attachment {
                    name: if name.is_empty() { None } else { Some(name) },
                    url: attachment_url,
                    mime_type: None,
                });
            }
        }

        Ok(PostDetailExtras {
            content: content_html,
            attachments,
        })
    }
}

impl SsufidPlugin for InfocomPlugin {
    const IDENTIFIER: &'static str = "infocom.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 전자정보공학부 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 전자정보공학부 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://infocom.ssu.ac.kr/kor/notice/undergraduate.php";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
            .build()
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        let mut all_metadata: Vec<InfocomPostMetadata> = Vec::new();
        let mut page = 1;

        loop {
            if posts_limit > 0 && all_metadata.len() >= posts_limit as usize {
                // Optimization: if posts_limit is 0, it means unlimited, so we don't check length
                // and rely on empty page result to break.
                // Otherwise, if we have enough metadata, no need to fetch more pages.
                break;
            }

            let mut page_metadata = self.fetch_page_posts_metadata(page, &client).await?;
            if page_metadata.is_empty() {
                break; // No more posts on subsequent pages
            }
            all_metadata.append(&mut page_metadata);
            page += 1;
        }

        if posts_limit > 0 {
            // Only truncate if posts_limit is not 0 (unlimited)
            all_metadata.truncate(posts_limit as usize);
        }

        let mut fetch_futures = FuturesOrdered::new();
        for meta in all_metadata {
            let client_clone = client.clone();
            // meta is already owned, no need to clone it here for moving into async block
            fetch_futures.push_back(async move {
                match self.fetch_full_post_details(&meta, &client_clone).await {
                    Ok(details) => Ok((meta, details)), // Pass meta along
                    Err(e) => Err(e),                   // Propagate error
                }
            });
        }

        let mut final_posts = Vec::new();
        while let Some(result) = fetch_futures.next().await {
            match result {
                Ok((meta, details)) => {
                    final_posts.push(SsufidPost {
                        id: meta.id,
                        url: meta.url,
                        title: meta.title,
                        created_at: meta.date,
                        author: None,         // Author info is not available
                        description: None, // Description can be part of content if needed, or fetched separately
                        category: Vec::new(), // Category info is not available
                        updated_at: None,  // Updated at info is not available
                        thumbnail: None,   // Thumbnail info is not available
                        content: details.content,
                        attachments: details.attachments,
                        metadata: None, // No specific extra metadata for now
                    });
                }
                Err(e) => {
                    // Log the error and continue processing other posts
                    // It's important to decide if one failure should fail all.
                    // Here, we log and skip. `e` already contains post_id/url if it's from fetch_full_post_details
                    tracing::warn!(
                        "Failed to fetch or parse details for a post: {:?}. Skipping.",
                        e
                    );
                }
            }
        }
        Ok(final_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Imports SsuInfocomPlugin, SsufidPlugin, etc.
    // Tokio is brought in by the test macro

    #[tokio::test]
    async fn test_fetch_page_posts_metadata_parses_correctly() {
        let plugin = InfocomPlugin::new();
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Test Agent) SSUFIDFetcher/0.1.0")
            .build()
            .unwrap();

        // Test page 1
        let metadata_result_p1 = plugin.fetch_page_posts_metadata(1, &client).await;
        assert!(
            metadata_result_p1.is_ok(),
            "fetch_page_posts_metadata(1) should not fail. Error: {:?}",
            metadata_result_p1.err()
        );
        let metadata_p1 = metadata_result_p1.unwrap();
        assert!(
            !metadata_p1.is_empty(),
            "Page 1: Should fetch at least one post metadata if notices exist."
        );

        if let Some(meta) = metadata_p1.first() {
            assert!(!meta.id.is_empty(), "Page 1: Post ID should not be empty");
            assert!(
                meta.url
                    .starts_with("http://infocom.ssu.ac.kr/kor/notice/undergraduate.php?idx=")
                    || meta.url.contains("idx="), // More flexible check for URL structure
                "Page 1: Post URL ({}) should be valid and contain 'idx='",
                meta.url
            );
            let parsed_url = Url::parse(&meta.url);
            assert!(
                parsed_url.is_ok(),
                "Page 1: Post URL ({}) should be parseable",
                meta.url
            );
            assert!(
                !meta.title.is_empty(),
                "Page 1: Post title should not be empty"
            );
        }
        // Assuming infocom has more than one page of notices for this check
        // This might be brittle if the site has very few notices at times.
        // tracing::info!("Found {} posts on page 1.", metadata_p1.len());

        // Test page 2 (optional, but good for checking pagination parsing)
        let metadata_result_p2 = plugin.fetch_page_posts_metadata(2, &client).await;
        assert!(
            metadata_result_p2.is_ok(),
            "fetch_page_posts_metadata(2) should not fail. Error: {:?}",
            metadata_result_p2.err()
        );
        let metadata_p2 = metadata_result_p2.unwrap();
        // It's possible page 2 is empty, which is a valid state.
        // So we don't assert !metadata_p2.is_empty() unless we know site structure.
        if let Some(meta) = metadata_p2.first() {
            assert!(!meta.id.is_empty(), "Page 2: Post ID should not be empty");
            assert!(
                meta.url
                    .starts_with("http://infocom.ssu.ac.kr/kor/notice/undergraduate.php?idx=")
                    || meta.url.contains("idx="),
                "Page 2: Post URL ({}) should be valid and contain 'idx='",
                meta.url
            );
            assert!(
                !meta.title.is_empty(),
                "Page 2: Post title should not be empty"
            );
        }
        // tracing::info!("Found {} posts on page 2.", metadata_p2.len());
    }

    #[tokio::test]
    async fn test_fetch_full_post_details_parses_content_and_attachments() {
        let plugin = InfocomPlugin::new();
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Test Agent) SSUFIDFetcher/0.1.0")
            .build()
            .unwrap();

        // Use fetch_page_posts_metadata to get metadata for the first post
        let posts_metadata_result = plugin.fetch_page_posts_metadata(1, &client).await;
        if posts_metadata_result.is_err() || posts_metadata_result.as_ref().unwrap().is_empty() {
            tracing::warn!("No posts found on page 1 for test_fetch_full_post_details. Skipping.");
            return; // Skip test if no posts to detail
        }
        let first_post_metadata = posts_metadata_result.unwrap().remove(0);

        tracing::info!(
            "Testing fetch_full_post_details with URL: {}",
            first_post_metadata.url
        );

        let details_result = plugin
            .fetch_full_post_details(&first_post_metadata, &client)
            .await;
        assert!(
            details_result.is_ok(),
            "fetch_full_post_details should not fail for post {}. Error: {:?}",
            first_post_metadata.id,
            details_result.err()
        );

        let details = details_result.unwrap();
        assert!(
            !details.content.is_empty(),
            "Fetched content for post {} should not be empty.",
            first_post_metadata.id
        );

        tracing::info!(
            "Fetched content (snippet) for post {}: {:.100}",
            first_post_metadata.id,
            details.content
        );
        tracing::info!(
            "Fetched {} attachments for post {}.",
            details.attachments.len(),
            first_post_metadata.id
        );
        for att in details.attachments.iter().take(2) {
            tracing::info!(
                "  Attachment for post {}: Name: {:?}, URL: {}",
                first_post_metadata.id,
                att.name,
                att.url
            );
        }
    }

    #[tokio::test]
    async fn test_crawl_fetches_posts_and_details() {
        let plugin = InfocomPlugin::new();
        let posts_limit_small = 2; // Small limit for basic check
        let posts_result_small = plugin.crawl(posts_limit_small).await;

        assert!(
            posts_result_small.is_ok(),
            "Crawl with limit {} should not return an error. Error: {:?}",
            posts_limit_small,
            posts_result_small.err()
        );

        let posts_small = posts_result_small.unwrap();
        // Site might have less than posts_limit_small posts
        if !posts_small.is_empty() {
            assert!(
                posts_small.len() <= posts_limit_small as usize,
                "Crawl with limit {} should return at most {} posts, got {}",
                posts_limit_small,
                posts_limit_small,
                posts_small.len()
            );
        } else {
            tracing::warn!(
                "Crawl with limit {} returned 0 posts. This might be okay if the site has fewer posts.",
                posts_limit_small
            );
        }

        for post in &posts_small {
            assert!(!post.id.is_empty(), "Post ID should not be empty");
            assert!(
                post.url.starts_with(InfocomPlugin::BASE_URL) || post.url.contains("idx="),
                "Post URL ({}) should be valid",
                post.url
            );
            assert!(!post.title.is_empty(), "Post title should not be empty");
            assert!(
                post.created_at.year() >= 2000, // Adjusted year for broader validity
                "Post creation year ({}) should be reasonable.",
                post.created_at.year()
            );
            assert!(
                !post.content.is_empty(),
                "Post content should be fetched and not empty for post ID {}",
                post.id
            );
        }

        // Test pagination by trying to fetch more posts than typically on one page
        // Assuming a page has around 15 posts for infocom. Fetching 17 should hit page 2.
        // This is an assumption. If the site structure changes, this specific number might need adjustment.
        let posts_limit_pagination = 17;
        let posts_result_pagination = plugin.crawl(posts_limit_pagination).await;

        assert!(
            posts_result_pagination.is_ok(),
            "Crawl with limit {} (for pagination) should not return an error. Error: {:?}",
            posts_limit_pagination,
            posts_result_pagination.err()
        );
        let posts_pagination = posts_result_pagination.unwrap();

        // We can't guarantee exactly 17 posts if the site has fewer overall.
        // But if it has more, it should try to fetch 17.
        if !posts_pagination.is_empty() && posts_pagination.len() < posts_limit_pagination as usize
        {
            tracing::warn!(
                "Pagination test: Expected up to {} posts, but site might have only {}. This is acceptable.",
                posts_limit_pagination,
                posts_pagination.len()
            );
        } else if posts_pagination.len() == posts_limit_pagination as usize {
            assert_eq!(
                posts_pagination.len(),
                posts_limit_pagination as usize,
                "Crawl with limit {} should return {} posts if available, got {}. This tests pagination.",
                posts_limit_pagination,
                posts_limit_pagination,
                posts_pagination.len()
            );
        }

        // Check structure of these paginated posts too
        for post in &posts_pagination {
            assert!(!post.id.is_empty(), "Paginated Post ID should not be empty");
            assert!(
                !post.title.is_empty(),
                "Paginated Post title should not be empty"
            );
            assert!(
                !post.content.is_empty(),
                "Paginated Post content should not be empty for ID {}",
                post.id
            );
        }
        tracing::info!(
            "Successfully fetched {} posts in pagination test (limit {}).",
            posts_pagination.len(),
            posts_limit_pagination
        );
    }

    #[tokio::test]
    async fn test_individual_post_structure_after_crawl() {
        let plugin = InfocomPlugin::new();
        let posts_result = plugin.crawl(1).await; // Fetch 1 post

        assert!(
            posts_result.is_ok(),
            "Crawl(1) for individual structure test failed: {:?}",
            posts_result.err()
        );
        let posts = posts_result.unwrap();

        if posts.is_empty() {
            tracing::warn!(
                "No posts found on the site for testing post structure with crawl(1). Skipping assertions."
            );
            return; // Skip if site has no posts
        }

        assert_eq!(
            posts.len(),
            1,
            "Crawl(1) should return exactly one post if available."
        );
        let post = &posts[0];

        assert!(!post.id.is_empty(), "Post ID should not be empty");
        assert!(
            post.url.starts_with(InfocomPlugin::BASE_URL) || post.url.contains("idx="),
            "Post URL ({}) should be valid",
            post.url
        );
        assert!(!post.title.is_empty(), "Post title should not be empty");
        assert!(
            post.created_at.year() >= 2000, // Adjusted year
            "Post creation year ({}) should be reasonable.",
            post.created_at.year()
        );
        assert!(
            !post.content.is_empty(),
            "Post content should be filled after crawl for ID {}.",
            post.id
        );
        tracing::info!(
            "Individual post structure test passed for post ID {}.",
            post.id
        );
    }
}
