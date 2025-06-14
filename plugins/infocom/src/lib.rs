use futures::stream::{FuturesOrdered, StreamExt};
use scraper::{Html, Selector};
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{Date, macros::offset};
use url::Url;

// Selectors struct (defined earlier)
struct Selectors {
    post_container: Selector,
    title: Selector,
    date: Selector,
    post_content_container: Selector,
    post_links: Selector,
    post_images: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_container: Selector::parse("a.con_box").unwrap(),
            title: Selector::parse("div.subject span").unwrap(),
            date: Selector::parse("ul.info li.date").unwrap(),
            post_content_container: Selector::parse("div.view_box div.con").unwrap(),
            post_links: Selector::parse("div.view_box div.con a[href]").unwrap(),
            post_images: Selector::parse("div.view_box div.con img[src]").unwrap(),
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
    pub fn new() -> Self {
        InfocomPlugin {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_full_post_details(
        &self,
        post_url: &str,
        client: &reqwest::Client,
    ) -> Result<PostDetailExtras, PluginError> {
        let response = client.get(post_url).send().await.map_err(|e| {
            PluginError::request::<Self>(format!("Failed to fetch post page {}: {}", post_url, e))
        })?;

        if !response.status().is_success() {
            return Err(PluginError::request::<Self>(format!(
                "Failed to fetch post page {}: status {}",
                post_url,
                response.status()
            )));
        }

        let html_content = response.text().await.map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to read post page {}: {}", post_url, e))
        })?;

        let document = Html::parse_document(&html_content);
        let mut attachments = Vec::new();

        let content_html = document
            .select(&self.selectors.post_content_container)
            .next()
            .map_or(String::new(), |element| element.inner_html());

        for link_element in document.select(&self.selectors.post_links) {
            if let Some(href) = link_element.value().attr("href") {
                let name = link_element.text().collect::<String>().trim().to_string();
                let attachment_url = Url::parse(Self::BASE_URL)
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

        for img_element in document.select(&self.selectors.post_images) {
            if let Some(src) = img_element.value().attr("src") {
                let name = img_element.value().attr("alt").map(str::to_string);
                let image_url = Url::parse(Self::BASE_URL)
                    .unwrap()
                    .join(src)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| src.to_string());

                attachments.push(Attachment {
                    name,
                    url: image_url,
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
    const TITLE: &'static str = "숭실대학교 컴퓨터학부 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 컴퓨터학부 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://infocom.ssu.ac.kr/kor/notice/undergraduate.php";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
            .build()
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        let initial_posts_metadata: Vec<SsufidPost> = {
            let response = client
                .get(Self::BASE_URL)
                .send()
                .await
                .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

            if !response.status().is_success() {
                return Err(PluginError::request::<Self>(format!(
                    "Failed to fetch HTML list page: {}",
                    response.status()
                )));
            }

            let html_content_list = response
                .text()
                .await
                .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

            let document_list = Html::parse_document(&html_content_list);
            let mut posts_data = Vec::new();

            for element in document_list
                .select(&self.selectors.post_container)
                .take(posts_limit as usize)
            {
                let relative_url = element.value().attr("href").ok_or_else(|| {
                    PluginError::parse::<Self>("Missing href attribute on list item".to_string())
                })?;

                let post_url = Url::parse(Self::BASE_URL)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!("Failed to parse BASE_URL: {}", e))
                    })?
                    .join(relative_url)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to join URL from list item: {}",
                            e
                        ))
                    })?
                    .to_string();

                let id = Url::parse(&post_url)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to parse post_url for id: {}",
                            e
                        ))
                    })?
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
                            "Missing 'idx' in query params for URL from list: {}",
                            post_url
                        ))
                    })?;

                let title_element =
                    element
                        .select(&self.selectors.title)
                        .next()
                        .ok_or_else(|| {
                            PluginError::parse::<Self>(
                                "Missing title element on list item".to_string(),
                            )
                        })?;
                let title = title_element.text().collect::<String>().trim().to_string();

                let date_element =
                    element.select(&self.selectors.date).next().ok_or_else(|| {
                        PluginError::parse::<Self>("Missing date element on list item".to_string())
                    })?;
                let date_str = date_element.text().collect::<String>().trim().to_string();

                let date_format = time::format_description::parse("[year]. [month]. [day]")
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Invalid date format description: {}",
                            e
                        ))
                    })?;
                let parsed_date = Date::parse(&date_str, &date_format).map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Failed to parse date string '{}' from list: {}",
                        date_str, e
                    ))
                })?;

                let created_at = parsed_date.midnight().assume_offset(offset!(+09:00));

                posts_data.push(SsufidPost {
                    id,
                    url: post_url,
                    title,
                    created_at,
                    author: None,
                    description: None,
                    category: Vec::new(),
                    updated_at: None,
                    thumbnail: None,
                    content: String::new(),
                    attachments: Vec::new(),
                    metadata: None,
                });
            }
            posts_data
        };

        let mut detailed_posts_futures = FuturesOrdered::new();
        for mut post_meta in initial_posts_metadata {
            let client_clone = client.clone();
            detailed_posts_futures.push_back(async move {
                match self.fetch_full_post_details(&post_meta.url, &client_clone).await {
                    Ok(details) => {
                        post_meta.content = details.content;
                        post_meta.attachments = details.attachments;
                    }
                    Err(e) => {
                        tracing::warn!(post_id = %post_meta.id, url = %post_meta.url, "Failed to fetch full details for post: {:?}. Returning basic details.", e);
                    }
                }
                post_meta
            });
        }

        let mut final_posts = Vec::new();
        while let Some(post) = detailed_posts_futures.next().await {
            final_posts.push(post);
        }

        Ok(final_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Imports SsuInfocomPlugin, SsufidPlugin, etc.
    // Tokio is brought in by the test macro

    #[tokio::test]
    async fn test_crawl_fetches_posts_and_details() {
        let plugin = InfocomPlugin::new();
        let posts_result = plugin.crawl(2).await; // Fetch up to 2 posts for testing

        assert!(
            posts_result.is_ok(),
            "Crawl should not return an error. Error: {:?}",
            posts_result.err()
        );

        let posts = posts_result.unwrap();
        assert!(
            !posts.is_empty(),
            "Should fetch at least one post if notices exist."
        );

        if let Some(post) = posts.first() {
            println!(
                "Fetched {} posts. First post content snippet: {:.100}",
                posts.len(),
                post.content
            );
            assert!(!post.id.is_empty(), "Post ID should not be empty");
            assert!(
                post.url.starts_with(InfocomPlugin::BASE_URL),
                "Post URL should start with BASE_URL"
            );
            assert!(!post.title.is_empty(), "Post title should not be empty");
            assert!(
                post.created_at.year() >= 2020,
                "Post creation year should be reasonable."
            );

            assert!(
                !post.content.is_empty(),
                "Post content should be fetched and not empty"
            );

            tracing::info!("Test Post Details: {:?}", post);
        }
    }

    #[tokio::test]
    async fn test_fetch_full_post_details_parses_content_and_attachments() {
        let plugin = InfocomPlugin::new();
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
            .build().unwrap();

        let list_response = client.get(InfocomPlugin::BASE_URL).send().await.unwrap();
        let list_html = list_response.text().await.unwrap();
        let list_doc = Html::parse_document(&list_html);
        let first_post_link_el = list_doc.select(&plugin.selectors.post_container).next();

        if first_post_link_el.is_none() {
            tracing::warn!(
                "No posts found on list page for test_fetch_full_post_details. Skipping."
            );
            return;
        }
        let first_post_relative_url = first_post_link_el.unwrap().value().attr("href").unwrap();
        let post_detail_url = Url::parse(InfocomPlugin::BASE_URL)
            .unwrap()
            .join(first_post_relative_url)
            .unwrap()
            .to_string();

        println!(
            "Testing fetch_full_post_details with URL: {}",
            post_detail_url
        );

        let details_result = plugin
            .fetch_full_post_details(&post_detail_url, &client)
            .await;
        assert!(
            details_result.is_ok(),
            "fetch_full_post_details should not fail. Error: {:?}",
            details_result.err()
        );

        let details = details_result.unwrap();
        assert!(
            !details.content.is_empty(),
            "Fetched content should not be empty."
        );

        println!("Fetched content (snippet): {:.100}", details.content);
        println!("Fetched {} attachments.", details.attachments.len());
        for att in details.attachments.iter().take(2) {
            println!("  Attachment: Name: {:?}, URL: {}", att.name, att.url);
        }
    }

    #[tokio::test]
    async fn test_individual_post_structure_after_crawl() {
        let plugin = InfocomPlugin::new();
        let posts = plugin
            .crawl(1)
            .await
            .expect("Crawl failed for single post structure test");

        if posts.is_empty() {
            tracing::warn!(
                "No posts found on the site for testing post structure. Skipping assertions."
            );
            return;
        }

        let post = &posts[0];
        assert!(!post.id.is_empty(), "Post ID should not be empty");
        assert!(
            post.url.starts_with(InfocomPlugin::BASE_URL),
            "Post URL should start with BASE_URL"
        );
        assert!(!post.title.is_empty(), "Post title should not be empty");
        assert!(
            post.created_at.year() >= 2020,
            "Post creation year should be reasonable."
        );
        assert!(
            !post.content.is_empty(),
            "Post content should be filled after crawl."
        );
    }
}
