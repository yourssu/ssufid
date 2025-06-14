use reqwest::Client;
use scraper::{Html, Selector};
use ssufid::core::{Attachment, SsufidPlugin, SsufidPost};
use ssufid::error::PluginError;
use time::macros::offset;
use time::{OffsetDateTime, PrimitiveDateTime};
use tracing::{error, trace, warn};
use url::Url as UrlParser; // Removed info, debug as per compiler warning

const BASE_URL_HOST_ONLY: &str = "https://materials.ssu.ac.kr"; // Removed 'static

#[derive(Debug, Clone)]
struct Selectors {
    // For listing page
    list_item_selector: Selector,
    post_link_selector: Selector,
    title_selector: Selector,
    // author_selector: Selector, // Removed
    // date_selector: Selector, // Removed as date_str was removed from PostMetadata
    notice_icon_selector: Selector,
    next_page_link_selector: Selector,
    list_item_selector_str: String,

    // For individual post page (detail view)
    post_title_selector_detail: Selector,
    post_author_selector_detail: Selector,
    post_date_selector_detail: Selector,
    post_content_selector: Selector,
    attachment_item_selector: Selector,
    attachment_link_selector: Selector,
    attachment_name_selector: Selector,
}

impl Selectors {
    fn new() -> Self {
        let list_item_selector_str = "div.news-list > ul > li".to_string();
        Self {
            list_item_selector: Selector::parse(&list_item_selector_str)
                .expect("Failed to parse list_item_selector"),
            post_link_selector: Selector::parse("a").expect("Failed to parse post_link_selector"),
            title_selector: Selector::parse("div.txt_box > div.tit_box > strong")
                .expect("Failed to parse title_selector"),
            // author_selector: Selector::parse("span.this-will-not-be-found-on-list-page").expect("Failed to parse author_selector"), // Removed
            // date_selector: Selector::parse("div.txt_box > p.mob_date").expect("Failed to parse date_selector"), // Removed
            notice_icon_selector: Selector::parse(
                "div.txt_box > div.tit_box > strong > span.tag01",
            )
            .expect("Failed to parse notice_icon_selector"),
            next_page_link_selector: Selector::parse(
                "div.paging_wrap ul.paging li.page_arrow a:has(img[src*=\"paging_next.png\"])",
            )
            .or_else(|_| {
                Selector::parse("div.paging_wrap ul.paging a:has(img[src*=\"paging_next.png\"])")
            })
            .expect("Failed to parse next_page_link_selector"),
            list_item_selector_str,

            post_title_selector_detail: Selector::parse("div.view_head div.subject p")
                .expect("Failed to parse post_title_selector_detail"),
            post_author_selector_detail: Selector::parse("div.view_head div.name > div.sv_member")
                .expect("Failed to parse post_author_selector_detail"),
            post_date_selector_detail: Selector::parse("div.view_head div.date p")
                .expect("Failed to parse post_date_selector_detail"),
            post_content_selector: Selector::parse("div#viewContent")
                .expect("Failed to parse post_content_selector"),
            attachment_item_selector: Selector::parse("div.attach > div.photo_type li")
                .expect("Failed to parse attachment_item_selector"),
            attachment_link_selector: Selector::parse("a")
                .expect("Failed to parse attachment_link_selector"),
            attachment_name_selector: Selector::parse("a span.name")
                .expect("Failed to parse attachment_name_selector"),
        }
    }
}

#[derive(Debug, Clone)]
struct PostMetadata {
    id: String,
    url: String,
    title: String,
    // date_str: String, // Removed
}

#[derive(Debug, Clone)]
struct FullPostData {
    id: String,
    url: String,
    title: String,
    author: Option<String>,
    created_at: OffsetDateTime,
    content: String,
    attachments: Vec<Attachment>,
}

pub struct MaterialsPlugin {
    selectors: Selectors,
    client: Client,
}

impl Default for MaterialsPlugin {
    fn default() -> Self {
        Self {
            selectors: Selectors::new(),
            client: Client::builder()
                        .danger_accept_invalid_certs(true) // No trailing whitespace
                        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.0.0 Safari/537.36")
                        .build()
                        .unwrap(),
        }
    }
}

impl MaterialsPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    async fn fetch_page_post_metadata_helper(
        &self,
        page_url: &str,
    ) -> Result<(Vec<PostMetadata>, Option<String>), PluginError> {
        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching metadata from page: {}", page_url);

        let response_text = self
            .client
            .get(page_url)
            .send()
            .await
            .map_err(|e| PluginError::request::<MaterialsPlugin>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::request::<MaterialsPlugin>(e.to_string()))?;

        let document = Html::parse_document(&response_text);
        let mut posts_on_page = Vec::new();

        let list_items = document.select(&self.selectors.list_item_selector);
        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Found {} items with list_item_selector ('{}').", list_items.clone().count(), self.selectors.list_item_selector_str);

        for (i, item_element) in list_items.enumerate() {
            trace!(target: MaterialsPlugin::IDENTIFIER, "Processing list item {}: {:?}", i, item_element.html());

            let post_link_element = match item_element
                .select(&self.selectors.post_link_selector)
                .next()
            {
                Some(el) => el,
                None => {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "No post_link_element found within list item {}. HTML: {}", i, item_element.html());
                    continue;
                }
            };

            if post_link_element
                .select(&self.selectors.notice_icon_selector)
                .next()
                .is_some()
            {
                let notice_title = post_link_element
                    .select(&self.selectors.title_selector)
                    .next()
                    .map_or_else(String::new, |t| {
                        t.text().collect::<String>().trim().to_string()
                    });
                tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Skipping notice item (Title: '{}').", notice_title);
                continue;
            }

            let relative_url_str = match post_link_element.value().attr("href") {
                Some(href) => href.trim().to_string(),
                None => {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "post_link_element in item {} has no href. HTML: {}", i, post_link_element.html());
                    continue;
                }
            };

            if relative_url_str.is_empty()
                || relative_url_str == "#"
                || relative_url_str.starts_with("javascript:")
            {
                warn!(target: MaterialsPlugin::IDENTIFIER, "Skipping invalid href in item {}: '{}'", i, relative_url_str);
                continue;
            }

            let absolute_url = UrlParser::parse(BASE_URL_HOST_ONLY)
                .expect("BASE_URL_HOST_ONLY should be a valid URL")
                .join(&relative_url_str)
                .map_err(|e| {
                    PluginError::parse::<MaterialsPlugin>(format!(
                        "Failed to join post URL '{}' with base '{}': {}",
                        relative_url_str, BASE_URL_HOST_ONLY, e
                    ))
                })?;

            let post_id = absolute_url
                .query_pairs()
                .find(|(key, _)| key == "num")
                .map(|(_, value)| value.into_owned())
                .ok_or_else(|| {
                    PluginError::parse::<MaterialsPlugin>(format!(
                        "Could not extract post ID ('num') from URL: {}",
                        absolute_url
                    ))
                })?;

            let title = post_link_element
                .select(&self.selectors.title_selector)
                .next()
                .map_or_else(String::new, |t| {
                    t.text().collect::<String>().trim().to_string()
                });

            posts_on_page.push(PostMetadata {
                id: post_id,
                url: absolute_url.to_string(),
                title,
            });
            tracing::trace!(target: MaterialsPlugin::IDENTIFIER, "Successfully extracted PostMetadata for post ID '{}'", posts_on_page.last().unwrap().id);
        }

        let next_page_url = document.select(&self.selectors.next_page_link_selector)
            .next()
            .and_then(|el| el.value().attr("href"))
            .and_then(|href_str| {
                UrlParser::parse(BASE_URL_HOST_ONLY)
                    .expect("BASE_URL_HOST_ONLY is valid")
                    .join(href_str.trim())
                    .map(|url| url.to_string())
                    .inspect_err(|e| { // No trailing whitespace
                        error!(target: MaterialsPlugin::IDENTIFIER, "Failed to join next page relative URL '{}' with base '{}': {}", href_str, BASE_URL_HOST_ONLY, e);
                    })
                    .ok()
            });

        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Next page link found: {:?}", next_page_url);
        Ok((posts_on_page, next_page_url))
    }

    async fn fetch_full_post_details(
        &self,
        meta: PostMetadata,
        client: &Client,
    ) -> Result<FullPostData, PluginError> {
        tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching full post details for ID {}: {}", meta.id, meta.url);
        let response_text = client
            .get(&meta.url)
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<MaterialsPlugin>(format!(
                    "Failed to fetch post page {}: {}",
                    meta.url, e
                ))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::request::<MaterialsPlugin>(format!(
                    "Failed to read post page text {}: {}",
                    meta.url, e
                ))
            })?;

        let document = Html::parse_document(&response_text);

        let title = document.select(&self.selectors.post_title_selector_detail).next()
            .map_or_else(
                || {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "Title not found on detail page for {}. Using title from metadata: '{}'", meta.url, meta.title);
                    meta.title.clone()
                },
                |el| el.text().collect::<String>().trim().to_string()
            );

        let author = document
            .select(&self.selectors.post_author_selector_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        let date_text_element = document
            .select(&self.selectors.post_date_selector_detail)
            .next();

        let created_at = if let Some(el) = date_text_element {
            let text_nodes = el.text().collect::<Vec<_>>();
            let mut date_str_opt: Option<&str> = None;
            let mut time_str_opt: Option<&str> = None;

            for s in &text_nodes {
                let s_trimmed = s.trim();
                if s_trimmed.matches('-').count() == 2 && s_trimmed.len() >= 8 {
                    date_str_opt = Some(s_trimmed);
                } else if s_trimmed.matches(':').count() == 1 && s_trimmed.len() >= 4 {
                    time_str_opt = Some(s_trimmed);
                }
                if date_str_opt.is_some() && time_str_opt.is_some() {
                    break;
                }
            }

            if let (Some(date_s), Some(time_s)) = (date_str_opt, time_str_opt) {
                let datetime_str = format!("{} {}", date_s, time_s);
                let format =
                    time::format_description::parse("[year]-[month]-[day] [hour]:[minute]")
                        .map_err(|e| {
                            PluginError::parse::<MaterialsPlugin>(format!(
                                "Failed to parse date format description: {}",
                                e
                            ))
                        })?;
                PrimitiveDateTime::parse(&datetime_str, &format)
                    .map_err(|e| {
                        PluginError::parse::<MaterialsPlugin>(format!(
                            "Failed to parse datetime string '{}': {}",
                            datetime_str, e
                        ))
                    })?
                    .assume_offset(offset!(+9))
            } else {
                warn!(target: MaterialsPlugin::IDENTIFIER, "Date/time parts not found or format unexpected on detail page for {}: {:?}. Using current time.", meta.url, text_nodes);
                OffsetDateTime::now_utc().to_offset(offset!(+9))
            }
        } else {
            warn!(target: MaterialsPlugin::IDENTIFIER, "Date element not found on detail page for {}. Using current time.", meta.url);
            OffsetDateTime::now_utc().to_offset(offset!(+9))
        };

        let content_html = document
            .select(&self.selectors.post_content_selector)
            .next()
            .map_or_else(String::new, |el| el.inner_html());

        let mut attachments = Vec::new();
        for attach_item_el in document.select(&self.selectors.attachment_item_selector) {
            if let Some(link_el) = attach_item_el
                .select(&self.selectors.attachment_link_selector)
                .next()
            {
                if let Some(href) = link_el.value().attr("href") {
                    let attach_url = UrlParser::parse(BASE_URL_HOST_ONLY)
                        .expect("BASE_URL_HOST_ONLY is valid")
                        .join(href.trim())
                        .map_err(|e| {
                            PluginError::parse::<MaterialsPlugin>(format!(
                                "Failed to join attachment URL '{}': {}",
                                href, e
                            ))
                        })?;

                    let name = link_el
                        .select(&self.selectors.attachment_name_selector)
                        .next()
                        .map(|name_el| name_el.text().collect::<String>().trim().to_string());

                    attachments.push(Attachment::from_guess(
                        name.unwrap_or_else(|| "unknown_attachment".to_string()),
                        attach_url.to_string(),
                    ));
                }
            }
        }

        Ok(FullPostData {
            id: meta.id.clone(),
            url: meta.url.clone(),
            title,
            author,
            created_at,
            content: content_html,
            attachments,
        })
    }
}

impl SsufidPlugin for MaterialsPlugin {
    const IDENTIFIER: &'static str = "materials.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 신소재공학과 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 신소재공학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://materials.ssu.ac.kr/bbs/board.php?tbl=bbs51";

    #[allow(clippy::manual_async_fn)] // Allowed because SsufidPlugin::crawl is not async_trait
    fn crawl(
        &self,
        posts_limit: u32,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, PluginError>> + Send {
        async move {
            tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Crawling started. Limit: {}", posts_limit);

            let mut collected_metadata: Vec<PostMetadata> = Vec::new();
            let mut current_page_url = MaterialsPlugin::BASE_URL.to_string();
            let mut page_count = 0;

            loop {
                page_count += 1;
                tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching metadata page {}: {}", page_count, current_page_url);

                let (page_meta, next_page_option) = self
                    .fetch_page_post_metadata_helper(&current_page_url)
                    .await?;

                if page_meta.is_empty() && page_count == 1 {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "No metadata found on the first page ({}). Check selectors or website structure.", current_page_url);
                }
                tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Metadata Page {} yielded {} items.", page_count, page_meta.len());

                for meta in page_meta {
                    if collected_metadata.len() < posts_limit as usize {
                        collected_metadata.push(meta);
                    } else {
                        break;
                    }
                }

                if collected_metadata.len() >= posts_limit as usize {
                    tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Reached posts_limit for metadata ({}) with {} items.", posts_limit, collected_metadata.len());
                    break;
                }

                if let Some(next_url) = next_page_option {
                    if next_url == current_page_url {
                        tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Next page URL is the same as current ({}). Assuming end of metadata pagination.", next_url);
                        break;
                    }
                    current_page_url = next_url;
                } else {
                    tracing::info!(target: MaterialsPlugin::IDENTIFIER, "No next page for metadata. Assuming end of pagination.");
                    break;
                }

                if page_count > 20 {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "Reached metadata page limit of 20. Stopping.");
                    break;
                }
            }
            tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Collected {} post metadata items in total.", collected_metadata.len());

            let mut ssufid_posts: Vec<SsufidPost> = Vec::new();
            for (i, meta) in collected_metadata.iter().enumerate() {
                if i >= posts_limit as usize {
                    break;
                }
                tracing::debug!(target: MaterialsPlugin::IDENTIFIER, "Fetching full details for metadata item {}/{} (ID: {})", i+1, collected_metadata.len(), meta.id);
                match self
                    .fetch_full_post_details(meta.clone(), &self.client)
                    .await
                {
                    Ok(full_data) => {
                        ssufid_posts.push(SsufidPost {
                            id: full_data.id,
                            url: full_data.url,
                            title: full_data.title,
                            author: full_data.author,
                            description: None,
                            category: vec![],
                            created_at: full_data.created_at,
                            updated_at: None,
                            thumbnail: None,
                            content: full_data.content,
                            attachments: full_data.attachments,
                            metadata: None,
                        });
                    }
                    Err(e) => {
                        error!(target: MaterialsPlugin::IDENTIFIER, "Failed to fetch full post details for ID {}: {}. Skipping.", meta.id, e);
                    }
                }
            }

            tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Successfully processed {} posts into SSUFID format.", ssufid_posts.len());
            Ok(ssufid_posts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{error, info, warn};

    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(format!("{}=trace", MaterialsPlugin::IDENTIFIER))
            .try_init();
    }

    #[tokio::test]
    async fn test_crawl_collects_full_posts() {
        init_tracing();
        let plugin = MaterialsPlugin::new();
        let posts_limit = 3;

        match plugin.crawl(posts_limit).await {
            Ok(posts_vec) => {
                assert!(!posts_vec.is_empty(), "Crawl should return some posts.");
                assert!(
                    posts_vec.len() <= posts_limit as usize,
                    "Should not exceed posts_limit."
                );
                for post in posts_vec {
                    assert!(!post.id.is_empty(), "Post ID should not be empty");
                    assert!(
                        !post.title.is_empty(),
                        "Post title should not be empty for ID {}",
                        post.id
                    );
                    info!(target: MaterialsPlugin::IDENTIFIER, "Tested Post: ID={}, Title='{}', HasContent={}", post.id, post.title, !post.content.is_empty());
                }
            }
            Err(e) => {
                panic!("Crawl failed: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_first_page_metadata_directly() {
        init_tracing();
        let plugin = MaterialsPlugin::new();

        tracing::info!(target: MaterialsPlugin::IDENTIFIER, "Testing fetch_page_post_metadata_helper with BASE_URL: {}", MaterialsPlugin::BASE_URL);

        let result = plugin
            .fetch_page_post_metadata_helper(MaterialsPlugin::BASE_URL)
            .await;

        if let Err(e) = &result {
            error!(target: MaterialsPlugin::IDENTIFIER, "fetch_page_post_metadata_helper failed: {:?}", e);
        }
        let (metadata, next_page_url_opt) =
            result.expect("Fetching metadata from first page should succeed");

        if metadata.is_empty() {
            let debug_client = reqwest::Client::new();
            let page_html_for_debug = debug_client
                .get(MaterialsPlugin::BASE_URL)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            warn!(target: MaterialsPlugin::IDENTIFIER,
                "No metadata collected from the first page. This might be due to website changes or incorrect selectors. \
                Current list_item_selector: '{}'. Page HTML for debug:\n{}",
                plugin.selectors.list_item_selector_str,
                page_html_for_debug
            );
        }

        assert!(
            !metadata.is_empty(),
            "Should collect some metadata from the first page."
        );

        for item in &metadata {
            assert!(
                !item.id.is_empty(),
                "Post ID should not be empty: {:?}",
                item
            );
            assert!(
                item.url.starts_with(BASE_URL_HOST_ONLY),
                "Post URL ('{}') should be absolute, starting with '{}': {:?}",
                item.url,
                BASE_URL_HOST_ONLY,
                item
            );
            assert!(
                !item.title.is_empty(),
                "Post title should not be empty: {:?}",
                item
            );
            // assert!(!item.date_str.is_empty(), "Post date string should not be empty: {:?}", item); // date_str removed from PostMetadata
        }

        if let Some(url) = &next_page_url_opt {
            info!(target: MaterialsPlugin::IDENTIFIER, "Next page URL from metadata helper: {}", url);
            assert!(
                url.starts_with(BASE_URL_HOST_ONLY),
                "Next page URL ('{}') should be absolute if present.",
                url
            );
            assert!(
                url.contains("page="),
                "Next page URL ('{}') should typically contain 'page='.",
                url
            );
        } else {
            info!(target: MaterialsPlugin::IDENTIFIER, "No next page URL found from the first page, this might be normal if there's only one page of results.");
        }
        info!(target: MaterialsPlugin::IDENTIFIER, "First page metadata count: {}. Next page URL: {:?}", metadata.len(), next_page_url_opt);
    }

    #[tokio::test]
    async fn test_fetch_single_post_detail() {
        init_tracing();
        let plugin = MaterialsPlugin::new();

        let metadata_items_result = plugin
            .fetch_page_post_metadata_helper(MaterialsPlugin::BASE_URL)
            .await;
        assert!(
            metadata_items_result.is_ok(),
            "Fetching metadata for single post detail test failed: {:?}",
            metadata_items_result.err()
        );
        let metadata_items = metadata_items_result.unwrap().0;

        assert!(
            !metadata_items.is_empty(),
            "Need at least one post from listing to test detail fetching."
        );

        let test_meta = metadata_items[0].clone();
        info!(target: MaterialsPlugin::IDENTIFIER, "Testing full detail fetch for: ID={}, URL={}", test_meta.id, test_meta.url);

        match plugin
            .fetch_full_post_details(test_meta.clone(), &plugin.client)
            .await
        {
            Ok(full_data) => {
                assert_eq!(full_data.id, test_meta.id);
                assert_eq!(full_data.url, test_meta.url);
                assert!(
                    !full_data.title.is_empty(),
                    "Full post title should not be empty"
                );
                info!(target: MaterialsPlugin::IDENTIFIER, "Fetched Full Data: Title='{}', Author='{:?}', ContentNotEmpty={}", full_data.title, full_data.author, !full_data.content.is_empty());
                if full_data.author.is_none() {
                    warn!(target: MaterialsPlugin::IDENTIFIER, "Author was None for post ID {}", full_data.id);
                }
                assert!(
                    full_data.created_at.year() > 2000,
                    "Parsed year seems too old, check date parsing. Year: {}",
                    full_data.created_at.year()
                );
            }
            Err(e) => {
                panic!(
                    "fetch_full_post_details failed for URL {}: {:?}",
                    test_meta.url, e
                );
            }
        }
    }
}
