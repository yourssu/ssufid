use reqwest::Client;
use scraper::{Html, Selector};
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset, macros::format_description};
use url::Url;

fn full_url(base: &str, path: &str) -> Result<String, PluginError> {
    let base_url = Url::parse(base).map_err(|e| {
        PluginError::custom::<EePlugin>(
            "UrlParse".to_string(),
            format!("Base URL parse error: {}", e),
        )
    })?;
    base_url
        .join(path)
        .map_err(|e| {
            PluginError::custom::<EePlugin>("UrlJoin".to_string(), format!("URL join error: {}", e))
        })
        .map(|u| u.to_string())
}

struct Selectors {
    post_item: Selector,
    post_link: Selector,
    post_title_view: Selector,
    post_author_view: Selector,
    post_date_view: Selector,
    post_content_view: Selector,
    attachment_link: Selector,
    next_page_link: Selector,
}

impl Selectors {
    fn new() -> Result<Self, PluginError> {
        Ok(Selectors {
            post_item: Selector::parse("div.board-list2 > ul > li:not(.label)")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_item selector: {}", e)))?,
            post_link: Selector::parse("div.subject > a")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_link selector: {}", e)))?,
            post_title_view: Selector::parse("div.board-view > div.head > h3.tit")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_title_view selector: {}", e)))?,
            post_author_view: Selector::parse("div.board-view > div.head > div.info > span.name strong")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_author_view selector: {}", e)))?,
            post_date_view: Selector::parse("div.board-view > div.head > div.info > span.date")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_date_view selector: {}", e)))?,
            post_content_view: Selector::parse("div.board-view > div.body")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse post_content_view selector: {}", e)))?,
            attachment_link: Selector::parse("div.board-view > div.head > div.files a[onclick*='download'], div.board-view > div.body a[href^='/uploaded/']")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse attachment_link selector: {}", e)))?,
            next_page_link: Selector::parse("div.paginate > a.next:not(.disabled)")
                .map_err(|e| PluginError::custom::<EePlugin>("SelectorParse".to_string(), format!("Failed to parse next_page_link selector: {}", e)))?,
        })
    }
}

pub struct EePlugin {
    selectors: Selectors,
    client: Client,
}

impl Default for EePlugin {
    fn default() -> Self {
        Self {
            selectors: Selectors::new().expect("Failed to initialize selectors"),
            client: Client::builder()
                .cookie_store(true)
                .build()
                .expect("Failed to build reqwest client"),
        }
    }
}

impl EePlugin {
    const KST_OFFSET: UtcOffset = match UtcOffset::from_hms(9, 0, 0) {
        Ok(offset) => offset,
        Err(_) => panic!("Invalid KST offset"),
    };

    async fn fetch_page_html(&self, url: &str) -> Result<String, PluginError> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!("Failed to send request to {}: {}", url, e))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!(
                    "Failed to get text from response {}: {}",
                    url, e
                ))
            })
    }

    fn parse_date_string(&self, date_str: &str) -> Result<OffsetDateTime, PluginError> {
        let format_description_datetime =
            format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        let format_description_date = format_description!("[year].[month].[day]");

        if let Ok(dt) = PrimitiveDateTime::parse(date_str, &format_description_datetime) {
            Ok(dt.assume_offset(Self::KST_OFFSET))
        } else if let Ok(d) = Date::parse(date_str, &format_description_date) {
            Ok(d.with_time(Time::MIDNIGHT).assume_offset(Self::KST_OFFSET))
        } else {
            Err(PluginError::parse::<Self>(format!(
                "Failed to parse date string: {}",
                date_str
            )))
        }
    }

    fn extract_text(element: &scraper::ElementRef, selector: &Selector) -> Option<String> {
        element
            .select(selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_html(element: &scraper::ElementRef, selector: &Selector) -> Option<String> {
        element
            .select(selector)
            .next()
            .map(|el| el.inner_html().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_idx_from_url(url_str: &str) -> Result<String, PluginError> {
        let parsed_url = Url::parse(url_str).map_err(|e| {
            PluginError::parse::<EePlugin>(format!(
                "Failed to parse URL for idx extraction {}: {}",
                url_str, e
            ))
        })?;
        parsed_url
            .query_pairs()
            .find_map(|key_value| {
                if key_value.0 == "idx" {
                    Some(key_value.1.into_owned())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                PluginError::parse::<EePlugin>(format!("Could not find 'idx' in URL: {}", url_str))
            })
    }
}

// REMOVED #[async_trait]
impl SsufidPlugin for EePlugin {
    const IDENTIFIER: &'static str = "ee.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 전기공학부";
    const DESCRIPTION: &'static str = "숭실대학교 전기공학부 학부소식 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://ee.ssu.ac.kr";

    // Kept `async fn` but without #[async_trait]
    // This requires the compiler to handle `async fn` in traits implicitly,
    // or match it with `impl Future` if the signatures are compatible.
    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut results = Vec::new();
        let mut page = 1;
        let list_base_url = format!("{}/sub/sub05_01.php", Self::BASE_URL);

        #[derive(Debug)]
        struct PostListItemInfo {
            relative_url: String,
            title_on_list: String,
        }

        loop {
            if results.len() >= posts_limit as usize && posts_limit > 0 {
                break;
            }

            let current_list_url = format!("{}?page={}", list_base_url, page);
            let list_html = self.fetch_page_html(&current_list_url).await?;

            let (items_to_fetch, has_next_page) = {
                let list_doc = Html::parse_document(&list_html);
                let mut items_to_fetch_current_page = Vec::new();
                for item_el in list_doc.select(&self.selectors.post_item) {
                    if let Some(link_el) = item_el.select(&self.selectors.post_link).next() {
                        if let Some(href) = link_el.value().attr("href") {
                            let title_on_list =
                                link_el.text().collect::<String>().trim().to_string();
                            if !title_on_list.is_empty() {
                                items_to_fetch_current_page.push(PostListItemInfo {
                                    relative_url: href.trim().to_string(),
                                    title_on_list,
                                });
                            }
                        }
                    }
                }
                let has_next_page_current = list_doc
                    .select(&self.selectors.next_page_link)
                    .next()
                    .is_some();
                (items_to_fetch_current_page, has_next_page_current)
            };

            // Removed empty if block: if items_to_fetch.is_empty() && page == 1 {}

            let mut posts_found_on_current_page = 0;
            for item_info in items_to_fetch {
                if results.len() >= posts_limit as usize && posts_limit > 0 {
                    break;
                }

                let post_view_url = full_url(Self::BASE_URL, &item_info.relative_url)?;
                let post_id = Self::extract_idx_from_url(&post_view_url)?;

                let view_html = self.fetch_page_html(&post_view_url).await?;

                let (title, author_str, created_date_str, content_str, attachments_data) = {
                    let view_doc = Html::parse_document(&view_html);
                    let title = Self::extract_text(
                        &view_doc.root_element(),
                        &self.selectors.post_title_view,
                    )
                    .unwrap_or_else(|| item_info.title_on_list.clone());

                    let author_str = Self::extract_text(
                        &view_doc.root_element(),
                        &self.selectors.post_author_view,
                    )
                    .unwrap_or_else(|| "전기공학부".to_string());

                    let created_date_str = Self::extract_text(
                        &view_doc.root_element(),
                        &self.selectors.post_date_view,
                    )
                    .ok_or_else(|| {
                        PluginError::parse::<Self>("Could not find date string on view page".into())
                    })?;

                    let content_str = Self::extract_html(
                        &view_doc.root_element(),
                        &self.selectors.post_content_view,
                    )
                    .ok_or_else(|| {
                        PluginError::parse::<Self>("Could not find content on view page".into())
                    })?;

                    let mut attachments_data_local = Vec::new();
                    for att_el in view_doc.select(&self.selectors.attachment_link) {
                        if let Some(href_attr) = att_el.value().attr("href") {
                            let att_name_str = att_el.text().collect::<String>().trim().to_string();
                            let att_url_res = if href_attr.starts_with("javascript:download") {
                                let params_str = href_attr
                                    .replace("javascript:download(", "")
                                    .replace(")", "");
                                let params: Vec<&str> = params_str
                                    .split(',')
                                    .map(|s| s.trim().trim_matches('\''))
                                    .collect();
                                if params.len() == 3 {
                                    Ok(format!(
                                        "{}/module/board/download.php?boardid={}&b_idx={}&idx={}",
                                        Self::BASE_URL,
                                        params[0],
                                        params[1],
                                        params[2]
                                    ))
                                } else {
                                    full_url(Self::BASE_URL, href_attr)
                                }
                            } else {
                                full_url(Self::BASE_URL, href_attr)
                            };
                            if let Ok(att_url) = att_url_res {
                                attachments_data_local.push((att_name_str, att_url));
                            }
                        }
                    }
                    (
                        title,
                        author_str,
                        created_date_str,
                        content_str,
                        attachments_data_local,
                    )
                };

                let created_at = self.parse_date_string(&created_date_str)?;
                let mut final_attachments = Vec::new();
                for (att_name_str, att_url) in attachments_data {
                    final_attachments.push(Attachment {
                        name: if att_name_str.is_empty() {
                            Some(format!("Attachment for post {}", post_id))
                        } else {
                            Some(att_name_str)
                        },
                        url: att_url,
                        mime_type: None,
                    });
                }

                results.push(SsufidPost {
                    id: post_id,
                    title,
                    author: Some(author_str),
                    content: content_str,
                    url: post_view_url,
                    created_at,
                    updated_at: None,
                    attachments: final_attachments,
                    description: None,
                    category: vec![],
                    thumbnail: None,
                    metadata: None,
                });
                posts_found_on_current_page += 1;
            }

            if posts_found_on_current_page == 0 && page > 1 {
                break;
            }
            if !has_next_page {
                break;
            }
            page += 1;
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_full_url_constructor() {
        assert_eq!(
            full_url("http://example.com", "/path").unwrap(),
            "http://example.com/path"
        );
    }

    #[tokio::test]
    async fn test_date_parsing() {
        let plugin = EePlugin::default();
        let kst = EePlugin::KST_OFFSET;

        let dt_str1 = "2025-05-12 14:44:58";
        let expected_dt1 = PrimitiveDateTime::new(
            Date::from_calendar_date(2025, time::Month::May, 12).unwrap(),
            Time::from_hms(14, 44, 58).unwrap(),
        )
        .assume_offset(kst);
        assert_eq!(plugin.parse_date_string(dt_str1).unwrap(), expected_dt1);
    }

    #[tokio::test]
    async fn test_extract_idx_from_url() {
        let url1 = "http://ee.ssu.ac.kr/sub/sub05_01.php?boardid=notice&mode=view&idx=232&sk=&sw=&offset=&category=";
        assert_eq!(EePlugin::extract_idx_from_url(url1).unwrap(), "232");
    }

    #[tokio::test]
    async fn test_crawl_ee_announcements() {
        let plugin = EePlugin::default();
        let posts_limit = 1;
        let posts = plugin.crawl(posts_limit).await.expect("Crawl failed");

        assert!(
            !posts.is_empty(),
            "No posts were crawled. Check selectors or website status."
        );
        assert!(
            posts.len() <= posts_limit as usize,
            "Crawled more posts than the limit."
        );

        println!(
            "Successfully crawled {} posts (limit was {}).",
            posts.len(),
            posts_limit
        );

        for (i, post) in posts.iter().enumerate() {
            println!("--- Post #{} ---", i + 1);
            println!("ID: {}", post.id);
            println!("URL: {}", post.url);
            assert!(
                !post.id.is_empty(),
                "Post ID is empty for post at index {}",
                i
            );
            assert!(
                post.url.starts_with(
                    "http://ee.ssu.ac.kr/sub/sub05_01.php?boardid=notice&mode=view&idx="
                ),
                "Post URL has an unexpected format for post at index {}: {}",
                i,
                post.url
            );
            assert!(
                !post.title.is_empty(),
                "Post title is empty for post at index {}",
                i
            );
            assert!(
                post.author.is_some(),
                "Post author is None for post at index {}",
                i
            );
            assert!(
                post.created_at.year() >= 2022,
                "Post date is too old for post at index {}: year {}",
                i,
                post.created_at.year()
            );
            assert!(
                !post.content.is_empty(),
                "Post content is empty for post at index {}",
                i
            );
        }
    }
}
