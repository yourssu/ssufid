use futures::{StreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector};
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use thiserror::Error;
use time::{Date, macros::offset};
use url::Url;

// Selectors
struct Selectors {
    post_row: Selector,
    post_link_and_title: Selector,
    post_author: Selector,
    post_date: Selector,
    post_id_param: &'static str,
    view_title: Selector,
    view_author: Selector,
    view_date: Selector,
    view_content: Selector,
    view_attachments_link: Selector,
    // next_page_link field was removed
}

impl Selectors {
    fn new() -> Self {
        Self {
            post_row: Selector::parse("table.board_list > tbody > tr").unwrap(),
            post_link_and_title: Selector::parse("td.subject > a").unwrap(),
            post_author: Selector::parse("td.writer").unwrap(),
            post_date: Selector::parse("td.date").unwrap(),
            post_id_param: "idx",
            view_title: Selector::parse("div.view_head h4.ellipsis")
                .unwrap_or_else(|_| Selector::parse("div.view_head h3.subject").unwrap()),
            view_author: Selector::parse("div.view_info span.writer")
                .unwrap_or_else(|_| Selector::parse("dl.writer dd").unwrap()),
            view_date: Selector::parse("div.view_info span.date")
                .unwrap_or_else(|_| Selector::parse("dl.date dd").unwrap()),
            view_content: Selector::parse("div.view_content")
                .unwrap_or_else(|_| Selector::parse("div.board_viewContent").unwrap()),
            view_attachments_link: Selector::parse("div.file_attach_dl dd > a")
                .unwrap_or_else(|_| Selector::parse("div.attach_file_dl a").unwrap()),
            // next_page_link initialization was removed
        }
    }
}

#[derive(Debug, Error)]
enum MePluginError {
    #[error("Failed to parse post ID from URL: {0}")]
    ParsePostId(String),
    #[error("Date parsing error: {0}")]
    DateParse(#[from] time::error::Parse),
    #[error("Missing element for selector: {0}")]
    MissingElement(String),
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

impl From<MePluginError> for PluginError {
    fn from(err: MePluginError) -> Self {
        PluginError::parse::<MePlugin>(err.to_string())
    }
}

pub struct MePlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl MePlugin {
    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    fn get_text(
        element: &scraper::ElementRef,
        selector: &Selector,
        selector_name: &str,
    ) -> Result<String, MePluginError> {
        element
            .select(selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .ok_or_else(|| MePluginError::MissingElement(selector_name.to_string()))
    }

    // fn get_attr was removed as unused

    async fn fetch_post_details(
        &self,
        post_url: String,
        post_id: String,
        list_author: String,
        list_date_str: String,
    ) -> Result<SsufidPost, PluginError> {
        tracing::debug!("Fetching post details for URL: {}", post_url);
        let response_text = self
            .http_client
            .get(&post_url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        let document = Html::parse_document(&response_text);

        let title = document.select(&self.selectors.view_title).next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| {
                tracing::warn!("Title not found using view_title selector for {}, using subject selector as fallback", post_url);
                document.select(&self.selectors.post_link_and_title).next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
            });

        let author = document
            .select(&self.selectors.view_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(list_author);

        let date_str_on_page = document
            .select(&self.selectors.view_date)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(list_date_str);

        let date_format = time::format_description::parse("[year].[month].[day]").map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse date format description: {}", e))
        })?;
        let created_at = Date::parse(&date_str_on_page, &date_format)
            .map_err(MePluginError::DateParse)?
            .midnight()
            .assume_offset(offset!(+9));

        let content_html = document
            .select(&self.selectors.view_content)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        let attachments = document
            .select(&self.selectors.view_attachments_link)
            .filter_map(|a_tag| {
                let href = a_tag.value().attr("href")?;
                let name = a_tag.text().collect::<String>().trim().to_string();
                let base_url_for_resolve = Url::parse("http://me.ssu.ac.kr/notice/").ok()?;
                let attachment_url = base_url_for_resolve
                    .join(href)
                    .map(|u| u.to_string())
                    .ok()?;

                Some(Attachment {
                    url: attachment_url,
                    name: Some(name.clone()).filter(|s| !s.is_empty()),
                    mime_type: mime_guess::from_path(&name).first_raw().map(str::to_string),
                })
            })
            .collect();

        Ok(SsufidPost {
            id: post_id,
            url: post_url,
            author: Some(author).filter(|s| !s.is_empty()),
            title,
            description: None,
            category: vec!["공지사항".to_string()],
            created_at,
            updated_at: None,
            thumbnail: None,
            content: content_html,
            attachments,
            metadata: None,
        })
    }
}

impl Default for MePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SsufidPlugin for MePlugin {
    const IDENTIFIER: &'static str = "me.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 기계공학부";
    const DESCRIPTION: &'static str = "숭실대학교 기계공학부 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://me.ssu.ac.kr/notice/notice01.php";

    fn crawl(
        &self,
        posts_limit: u32,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, PluginError>> + Send {
        async move {
            struct TempPostData {
                url: String,
                id: String,
                author: String,
                date_str: String,
            }

            let mut all_posts_details_futures = FuturesOrdered::new();
            let mut temp_posts_data = Vec::new();
            let mut page_num = 1;
            let mut posts_collected_count = 0;

            loop {
                if posts_limit > 0 && posts_collected_count >= posts_limit {
                    break;
                }

                let current_page_url = format!("{}?curPage={}", Self::BASE_URL, page_num);
                tracing::info!("Crawling page: {}", current_page_url);

                let response_text = self
                    .http_client
                    .get(&current_page_url)
                    .send()
                    .await
                    .map_err(|e| PluginError::request::<Self>(e.to_string()))?
                    .text()
                    .await
                    .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

                let mut posts_found_on_this_page = 0;
                {
                    let document = Html::parse_document(&response_text);
                    for row_element in document.select(&self.selectors.post_row) {
                        if posts_limit > 0 && posts_collected_count >= posts_limit {
                            break;
                        }

                        let link_element_opt = row_element
                            .select(&self.selectors.post_link_and_title)
                            .next();
                        if link_element_opt.is_none() {
                            continue;
                        }
                        let link_element = link_element_opt.unwrap();

                        let post_href = match link_element.value().attr("href") {
                            Some(href) => href,
                            None => {
                                tracing::warn!(
                                    "Could not find href for a post row on page {}",
                                    page_num
                                );
                                continue;
                            }
                        };

                        let base_url_obj =
                            Url::parse(Self::BASE_URL).map_err(MePluginError::UrlParse)?;
                        let post_view_url = base_url_obj
                            .join(post_href)
                            .map_err(MePluginError::UrlParse)?
                            .to_string();

                        let parsed_post_url =
                            Url::parse(&post_view_url).map_err(MePluginError::UrlParse)?;
                        let post_id = parsed_post_url
                            .query_pairs()
                            .find(|(key, _)| key == self.selectors.post_id_param)
                            .map(|(_, value)| value.into_owned())
                            .ok_or_else(|| MePluginError::ParsePostId(post_view_url.clone()))?;

                        let _title = link_element.text().collect::<String>().trim().to_string();
                        let author = Self::get_text(
                            &row_element,
                            &self.selectors.post_author,
                            "post_author",
                        )
                        .unwrap_or_default();
                        let date_str =
                            Self::get_text(&row_element, &self.selectors.post_date, "post_date")
                                .unwrap_or_default();

                        temp_posts_data.push(TempPostData {
                            url: post_view_url,
                            id: post_id,
                            author,
                            date_str,
                        });

                        posts_found_on_this_page += 1;
                        posts_collected_count += 1;
                    }
                }

                if posts_found_on_this_page == 0 && page_num > 1 {
                    tracing::info!("No posts found on page {}, assuming end.", page_num);
                    break;
                }
                if posts_found_on_this_page == 0 && page_num == 1 && posts_collected_count == 0 {
                    tracing::warn!(
                        "No posts found on the first page: {}. Check selectors or website structure.",
                        current_page_url
                    );
                    break;
                }

                page_num += 1;
                if page_num > 50 {
                    tracing::warn!(
                        "Reached page 50, stopping pagination to prevent infinite loop."
                    );
                    break;
                }
            }

            for temp_data in temp_posts_data {
                all_posts_details_futures.push_back(self.fetch_post_details(
                    temp_data.url,
                    temp_data.id,
                    temp_data.author,
                    temp_data.date_str,
                ));
            }

            let mut all_posts = Vec::new();
            while let Some(post_result) = all_posts_details_futures.next().await {
                match post_result {
                    Ok(post) => all_posts.push(post),
                    Err(e) => {
                        tracing::error!("Failed to fetch post details (after list parse): {}", e)
                    }
                }
            }

            all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Ok(all_posts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_single_post_directly() {
        let plugin = MePlugin::new();
        let sample_post_idx = "6008788";
        let sample_post_url = format!(
            "http://me.ssu.ac.kr/notice/notice01_view.php?idx={}",
            sample_post_idx
        );
        let sample_post_id = sample_post_idx.to_string();
        let sample_author = "기계공학부".to_string();
        let sample_date_str = "2023.01.01".to_string();

        match plugin
            .fetch_post_details(
                sample_post_url.clone(),
                sample_post_id.clone(),
                sample_author,
                sample_date_str,
            )
            .await
        {
            Ok(post) => {
                assert_eq!(post.id, sample_post_id, "Post ID mismatch");
                assert_eq!(post.url, sample_post_url, "Post URL mismatch");
                assert!(!post.title.is_empty(), "Post title should not be empty");
                assert!(post.author.is_some(), "Post author should be Some");
                assert!(
                    !post.content.is_empty(),
                    "Post content should not be empty (fetched from view page)"
                );
                println!("Fetched single post successfully: {:?}", post);
            }
            Err(e) => {
                panic!(
                    "Failed to fetch sample post directly: {}\\nURL: {}\\nP.S. This test requires network access and a valid post IDX.",
                    e, sample_post_url
                );
            }
        }
    }

    #[tokio::test]
    async fn test_crawl_me_notices() {
        let plugin = MePlugin::new();
        let posts_limit = 3;

        match plugin.crawl(posts_limit).await {
            Ok(posts) => {
                if posts.is_empty() {
                    eprintln!(
                        "Warning: Crawl returned no posts. This could be due to network issues, incorrect selectors, or the site having no recent posts."
                    );
                } else {
                    assert!(
                        posts.len() <= posts_limit as usize,
                        "Should not exceed post limit"
                    );
                    println!("Crawled {} posts successfully.", posts.len());
                    for post in &posts {
                        assert!(!post.id.is_empty(), "Post ID is empty");
                        assert!(
                            post.url.starts_with("http://me.ssu.ac.kr"),
                            "Post URL ( {} ) is invalid",
                            post.url
                        );
                        assert!(!post.title.is_empty(), "Post title is empty");
                        assert!(!post.content.is_empty(), "Post content is empty");
                    }
                    println!("First crawled post (if any): {:?}", posts.first());
                }
            }
            Err(e) => {
                panic!(
                    "Crawl failed: {}\\nP.S. This test requires network access and correct selectors.",
                    e
                );
            }
        }
    }
}
