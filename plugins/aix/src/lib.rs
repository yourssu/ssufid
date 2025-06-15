use reqwest::Client;
use scraper::{Html, Selector};
use ssufid::core::{Attachment, SsufidPost};
use ssufid::error::PluginError; // Removed PluginErrorKind
use time::{macros::format_description, Date}; // Removed OffsetDateTime
use url::Url;
use std::future::Future; // Added for explicit Future type

const MAX_POSTS_LIMIT: u32 = 20; // Define a reasonable limit for fetching posts

pub struct AixPlugin {
    client: Client,
}

impl AixPlugin {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    async fn fetch_post_content(&self, post_url: &str) -> Result<(String, Vec<Attachment>), PluginError> {
        let url = Url::parse(post_url)
            .map_err(|e| PluginError::request::<AixPlugin>(e.to_string()))?;
        let res = self.client.get(url.clone()).send().await
            .map_err(|e| PluginError::request::<AixPlugin>(e.to_string()))?;
        if !res.status().is_success() {
            return Err(PluginError::request::<AixPlugin>(format!(
                "Failed to fetch post content: {}",
                res.status()
            )));
        }
        let body = res.text().await
            .map_err(|e| PluginError::parse::<AixPlugin>(e.to_string()))?;
        let document = Html::parse_document(&body);

        // Selector for the main content of the post
        // Based on the provided HTML: div.sub_notice_view > table > tr > td > p (and other elements within td)
        let content_selector = Selector::parse("div.sub_notice_view > table > tbody > tr > td").map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse content selector: {}", e)))?;

        let mut content_html = String::new();
        // Content is in the 3rd <td> element selected by content_selector (index 2)
        if let Some(content_element) = document.select(&content_selector).nth(2) {
            content_html = content_element.inner_html();
        } else {
            log::warn!("Could not find content element for post: {}", post_url);
        }

        // Selector for attachments
        // Based on the provided HTML: div.sub_notice_view > table > tr > td > li > a
        let attachment_selector = Selector::parse("div.sub_notice_view > table > tbody > tr > td > li > a").map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse attachment selector: {}", e)))?;
        let mut attachments = Vec::new();

        // Attachments are in the 2nd <td> element selected by content_selector (index 1)
        if let Some(attachment_container_element) = document.select(&content_selector).nth(1) {
            for element in attachment_container_element.select(&attachment_selector) {
                if let Some(href) = element.value().attr("href") {
                    let name = element.text().collect::<String>().trim().to_string();
                    // Attachment URLs on this site are relative like "/lib/download.php?file_name=..."
                    // We need to join them with the base of the *main site*, not necessarily AixPlugin::BASE_URL if it's just "/"
                    let base_url_for_attachments = Url::parse("https://aix.ssu.ac.kr")
                        .map_err(|e| PluginError::custom::<AixPlugin>("Config".to_string(), format!("Static base URL for attachments is invalid: {}",e)))?;

                    let attachment_url = base_url_for_attachments.join(href)
                        .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse attachment URL '{}' with base '{}': {}", href, base_url_for_attachments, e)))?;
                    attachments.push(Attachment::from_guess(name, attachment_url.to_string()));
                }
            }
        }


        Ok((content_html, attachments))
    }
}

impl ssufid::core::SsufidPlugin for AixPlugin {
    const TITLE: &'static str = "숭실대학교 AI융합학부";
    const IDENTIFIER: &'static str = "aix";
    const DESCRIPTION: &'static str = "숭실대학교 AI융합학부 공지사항을 제공합니다.";
    // BASE_URL should be the page where the list of notices is, or a common prefix.
    // For resolving relative URLs from the notice list page, this should be "https://aix.ssu.ac.kr/"
    // For displaying as the source of the feed, "https://aix.ssu.ac.kr/notice.html" might be more specific.
    // Let's use the directory for now, as individual post links are relative to this.
    const BASE_URL: &'static str = "https://aix.ssu.ac.kr/";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let notice_list_url = "https://aix.ssu.ac.kr/notice.html";
        let limit = posts_limit.min(MAX_POSTS_LIMIT);

            let res = self.client.get(notice_list_url).send().await
                .map_err(|e| PluginError::request::<AixPlugin>(e.to_string()))?;
            if !res.status().is_success() {
                return Err(PluginError::request::<AixPlugin>(format!(
                    "Failed to fetch notice list: {}",
                    res.status()
                )));
            }
            let body = res.text().await
                .map_err(|e| PluginError::parse::<AixPlugin>(e.to_string()))?;

            // Define TempPostData struct locally or make it more general if used elsewhere
            struct TempPostData {
                post_url_str: String,
                post_id: String,
                title: String,
                created_at: time::Date,
            }

            // Synchronous parsing scope
            let temp_posts_data: Vec<TempPostData> = {
                let document = Html::parse_document(&body);
                let page_base_url = Url::parse(notice_list_url) // notice_list_url is &str, fine to use here
                    .map_err(|e| PluginError::custom::<AixPlugin>("Config".to_string(), format!("Notice list URL is invalid: {}",e)))?;

                // Selectors are Send + Sync, can be created once
                let row_selector = Selector::parse("div.table-responsive > table > tbody > tr")
                    .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse row selector: {}", e)))?;
                let cell_selector = Selector::parse("td")
                    .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse cell selector: {}", e)))?;
                let link_selector = Selector::parse("a")
                    .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse link selector: {}", e)))?;

                let date_format = format_description!("[year].[month].[day]");
                let mut collected_data = Vec::new();

                for row in document.select(&row_selector) {
                    if collected_data.len() >= limit as usize { break; }

                    let cells: Vec<_> = row.select(&cell_selector).collect();
                    if cells.len() < 4 {
                        log::warn!("Skipping row with insufficient cells: {:?}", row.inner_html());
                        continue;
                    }

                    let title_element = cells[0].select(&link_selector).next();
                    let title = title_element.map_or_else(
                        || cells[0].text().collect::<String>().trim().to_string(),
                        |el| el.text().collect::<String>().trim().to_string()
                    );

                    let relative_url = match title_element.and_then(|a| a.value().attr("href")) {
                        Some(href) if href.starts_with("notice_view.html") => href,
                        _ => {
                            cells[0].select(&link_selector)
                                .find(|a| a.value().attr("href").map_or(false, |h| h.starts_with("notice_view.html")))
                                .and_then(|a| a.value().attr("href"))
                                .ok_or_else(|| PluginError::parse::<AixPlugin>(format!("Skipping row with no suitable notice_view.html link in title: '{}'", title)))?
                        }
                    };

                    let post_url = page_base_url.join(relative_url)
                        .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to join post URL '{}' with base '{}': {}", relative_url, page_base_url, e)))?;

                    let post_id = post_url.query_pairs().find(|(key, _)| key == "idx").map(|(_, val)| val.into_owned())
                        .ok_or_else(|| PluginError::parse::<AixPlugin>(format!("Could not parse idx from post URL: {}", post_url)))?;

                    let date_str = cells[2].text().collect::<String>().trim().to_string();
                    let created_at_date = Date::parse(&date_str, &date_format)
                        .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse date '{}': {}", date_str, e)))?;

                    collected_data.push(TempPostData {
                        post_url_str: post_url.to_string(),
                        post_id,
                        title,
                        created_at: created_at_date,
                    });
                }
                collected_data // Return from the block, document is dropped here
            };

            let mut posts = Vec::new();
            for temp_data in temp_posts_data {
                if posts.len() >= limit as usize { break; }

                let (content, attachments) = self.fetch_post_content(&temp_data.post_url_str).await?;

                let description_text = Html::parse_fragment(&content).root_element().text().collect::<String>();
                let description = if description_text.len() > 100 {
                    description_text.chars().take(100).collect::<String>() + "..."
                } else {
                    description_text
                };

                posts.push(SsufidPost {
                    id: temp_data.post_id,
                    url: temp_data.post_url_str,
                    author: None, // Author is not available from the page
                    title: temp_data.title,
                    description: Some(description),
                    category: vec!["공지사항".to_string()],
                    created_at: temp_data.created_at.with_time(time::macros::time!(0:0:0)).assume_utc(),
                    updated_at: None,
                    thumbnail: None,
                    content, // Full HTML content
                    attachments,
                    metadata: None,
                });
            }
            Ok(posts)
    }
}

impl Default for AixPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssufid::core::SsufidPlugin; // Added import
    use time::macros::datetime;
    use time::OffsetDateTime; // Required for datetime! macro

    // Helper function to parse post content and attachments from HTML string
    // This mirrors the relevant parts of AixPlugin::fetch_post_content
    fn parse_post_html(html_content: &str) -> Result<(String, Vec<Attachment>), PluginError> {
        let document = Html::parse_document(html_content);

        let content_selector_str = "div.sub_notice_view > table > tbody > tr > td";
        let content_selector = Selector::parse(content_selector_str)
            .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse content selector: {}", e)))?;

        let mut post_html_content = String::new();
        if let Some(content_element) = document.select(&content_selector).nth(2) {
            post_html_content = content_element.inner_html();
        } else {
            // For posts with no attachments, content might be in a different td index if the structure changes.
            // The provided mock_post_1626_html has an empty <td> for attachments, so index 2 for content is consistent.
            log::warn!("Could not find content element for post with selector: {}", content_selector_str);
        }

        let attachment_selector_str = "div.sub_notice_view > table > tbody > tr > td > li > a";
        let attachment_selector = Selector::parse(attachment_selector_str)
            .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse attachment selector: {}", e)))?;

        let mut attachments = Vec::new();
        // Attachments are in the 2nd <td> element selected by content_selector (index 1)
        if let Some(attachment_container_element) = document.select(&content_selector).nth(1) {
            for element in attachment_container_element.select(&attachment_selector) {
                if let Some(href) = element.value().attr("href") {
                    let name = element.text().collect::<String>().trim().to_string();
                    let base_url_for_attachments = Url::parse("https://aix.ssu.ac.kr")
                        .map_err(|e| PluginError::custom::<AixPlugin>("Config".to_string(), format!("Static base URL for attachments is invalid: {}",e)))?;
                    let attachment_url = base_url_for_attachments.join(href)
                        .map_err(|e| PluginError::parse::<AixPlugin>(format!("Failed to parse attachment URL '{}': {}", href, e)))?;
                    attachments.push(Attachment::from_guess(name, attachment_url.to_string()));
                }
            }
        }
        Ok((post_html_content, attachments))
    }


    #[tokio::test]
    async fn test_parse_notice_list_and_fetch_content() {
        // Mock HTML for notice.html
        let mock_notice_list_html = r#"
        <!DOCTYPE html>
        <html><body>
        <div class="table-responsive">
            <table>
                <tbody>
                    <tr> <th>제목</th><th>작성자</th><th>발행일자</th><th>조회수</th> </tr>
                    <tr> <td> [공지] <a href="notice_view.html?category=1&idx=1592">세미나실 예약 방법 안내(형남 424호)</a></td> <td></td> <td>2025.03.12</td> <td>67155</td> </tr>
                    <tr> <td><a href="notice_view.html?category=1&idx=1626">[숭실대학일자리플러스사업단] 2025학년도 온라인 직무특강_잇다 안내</a></td> <td></td> <td>2025.06.11</td> <td>2572</td> </tr>
                </tbody>
            </table>
        </div>
        </body></html>
        "#;

        // Mock HTML for notice_view.html?category=1&idx=1592
        let mock_post_1592_html = r#"
        <!DOCTYPE html>
        <html><body>
        <div class="sub_notice_view">
            <table>
                <tbody>
                    <tr><th><h4>세미나실 예약 방법 안내(형남 424호)</h4></th></tr>
                    <tr><td><span>작성자</span> :  &nbsp;｜&nbsp;<span>작성일</span> : 2025.03.12 &nbsp;｜&nbsp;<span>조회수</span>: 67172</td></tr>
                    <tr><td> <!-- Attachments -->
                        <li><a href="/lib/download.php?file_name=[최종2]-mAIn-사용-가이드.pdf&save_file=n_202503121442410.pdf">[최종2]-mAIn-사용-가이드.pdf</a></li>
                    </td></tr>
                    <tr><td> <!-- Content -->
                        <p>1. 예약 방법 : 기존 구글 캘린더 공유 및 예약 -&gt; mAIn 앱을 활용한 예약</p>
                    </td></tr>
                </tbody>
            </table>
        </div>
        </body></html>
        "#;

        // Mock HTML for notice_view.html?category=1&idx=1626
        let mock_post_1626_html = r#"
        <!DOCTYPE html>
        <html><body>
        <div class="sub_notice_view">
            <table>
                <tbody>
                <tr><th><h4>[숭실대학일자리플러스사업단] 2025학년도 온라인 직무특강_잇다 안내</h4></th></tr>
                <tr><td><span>작성일</span> : 2025.06.11</td></tr>
                <tr><td></td></tr> <!-- No Attachments -->
                <tr><td><p>온라인 직무특강 내용입니다.</p></td></tr>
                </tbody>
            </table>
        </div>
        </body></html>
        "#;

        assert_eq!(AixPlugin::IDENTIFIER, "aix");
        assert_eq!(AixPlugin::TITLE, "숭실대학교 AI융합학부");

        // --- Step 1: Parse notice list HTML to get TempPostData-like info ---
        let list_document = Html::parse_document(mock_notice_list_html);
        let row_selector = Selector::parse("div.table-responsive > table > tbody > tr").unwrap();
        let cell_selector = Selector::parse("td").unwrap();
        let link_selector = Selector::parse("a").unwrap();

        let date_format = format_description!("[year].[month].[day]");
        let page_base_url = Url::parse("https://aix.ssu.ac.kr/notice.html").unwrap();

        // Define a structure similar to TempPostData for testing
        #[derive(Debug)]
        struct TestPostListItem {
            id: String,
            url: String,
            title: String,
            created_at_date: Date,
            mock_html_content: String, // To associate with the correct mock post HTML
        }

        let mut test_post_list_items = Vec::new();
        for row in list_document.select(&row_selector) {
            let cells: Vec<_> = row.select(&cell_selector).collect();
            if cells.len() < 4 { continue; } // Skip header or malformed rows

            let title_cell = &cells[0];
            let title_element = title_cell.select(&link_selector).next();

            // Ensure the full title including prefixes like "[공지]" is captured
            let title = title_cell.text().collect::<String>().trim().to_string();

            let relative_url = title_element.and_then(|a| a.value().attr("href"))
                .expect("Failed to find href in title cell");

            let post_url_obj = page_base_url.join(relative_url).unwrap();
            let post_url_str = post_url_obj.to_string();

            let post_id = post_url_obj.query_pairs().find(|(key, _)| key == "idx")
                .map(|(_, val)| val.into_owned())
                .expect("Could not parse idx from post URL");

            let date_str = cells[2].text().collect::<String>().trim().to_string();
            let created_at_date = Date::parse(&date_str, &date_format)
                .expect("Failed to parse date");

            let mock_html_content = match post_id.as_str() {
                "1592" => mock_post_1592_html.to_string(),
                "1626" => mock_post_1626_html.to_string(),
                _ => panic!("No mock HTML for post ID {}", post_id),
            };

            test_post_list_items.push(TestPostListItem {
                id: post_id,
                url: post_url_str,
                title,
                created_at_date,
                mock_html_content,
            });
        }

        assert_eq!(test_post_list_items.len(), 2);
        assert_eq!(test_post_list_items[0].id, "1592");
        // The title includes the "[공지]" prefix.
        assert_eq!(test_post_list_items[0].title, "[공지] 세미나실 예약 방법 안내(형남 424호)");
        assert_eq!(test_post_list_items[0].url, "https://aix.ssu.ac.kr/notice_view.html?category=1&idx=1592");
        assert_eq!(test_post_list_items[0].created_at_date, Date::from_calendar_date(2025, time::Month::March, 12).unwrap());

        assert_eq!(test_post_list_items[1].id, "1626");
        assert_eq!(test_post_list_items[1].title, "[숭실대학일자리플러스사업단] 2025학년도 온라인 직무특강_잇다 안내");


        // --- Step 2 & 3: For each item, parse its mock HTML and construct SsufidPost ---
        let mut actual_posts: Vec<SsufidPost> = Vec::new();
        for item in test_post_list_items {
            let (post_content_html, attachments) = parse_post_html(&item.mock_html_content).unwrap();

            let description_text = Html::parse_fragment(&post_content_html).root_element().text().collect::<String>();
            let description = if description_text.len() > 100 {
                description_text.chars().take(100).collect::<String>() + "..."
            } else {
                description_text
            };

            actual_posts.push(SsufidPost {
                id: item.id,
                url: item.url,
                author: None,
                title: item.title,
                description: Some(description),
                category: vec!["공지사항".to_string()],
                created_at: item.created_at_date.with_time(time::macros::time!(0:0:0)).assume_utc(),
                updated_at: None,
                thumbnail: None,
                content: post_content_html,
                attachments,
                metadata: None,
            });
        }

        // --- Step 4: Assertions for SsufidPost fields ---
        assert_eq!(actual_posts.len(), 2);

        // Post 1: 1592
        let post1 = &actual_posts[0];
        assert_eq!(post1.id, "1592");
        assert_eq!(post1.url, "https://aix.ssu.ac.kr/notice_view.html?category=1&idx=1592");
        assert_eq!(post1.title, "[공지] 세미나실 예약 방법 안내(형남 424호)");
        assert_eq!(post1.created_at, datetime!(2025-03-12 00:00:00 UTC));
        let expected_content1 = "<!-- Content -->\n                        <p>1. 예약 방법 : 기존 구글 캘린더 공유 및 예약 -&gt; mAIn 앱을 활용한 예약</p>";
        assert_eq!(post1.content.trim(), expected_content1.trim()); // .trim() to handle potential whitespace differences from inner_html()

        let expected_description1_full = "1. 예약 방법 : 기존 구글 캘린더 공유 및 예약 -> mAIn 앱을 활용한 예약";
        let expected_description1 = if expected_description1_full.len() > 100 {
             expected_description1_full.chars().take(100).collect::<String>() + "..."
        } else {
            expected_description1_full.to_string()
        };
        assert_eq!(post1.description.as_ref().unwrap(), &expected_description1);

        assert_eq!(post1.attachments.len(), 1);
        assert_eq!(post1.attachments[0].name, Some("[최종2]-mAIn-사용-가이드.pdf".to_string()));
        // Ensure URL encoding is correctly handled/expected for query parameters.
        // The main code's `Url::join` and `to_string` will percent-encode characters in the path, but not typically in query string values unless they are already encoded or contain specific chars.
        // The href was "/lib/download.php?file_name=[최종2]-mAIn-사용-가이드.pdf&save_file=n_202503121442410.pdf"
        // Url::join should handle this correctly. The `file_name` parameter will likely be percent-encoded by the Url crate if it contains special characters.
        // `[`, `]` are special.
        let expected_attachment_url1 = "https://aix.ssu.ac.kr/lib/download.php?file_name=%5B%EC%B5%9C%EC%A2%852%5D-mAIn-%EC%82%AC%EC%9A%A9-%EA%B0%80%EC%9D%B4%EB%93%9C.pdf&save_file=n_202503121442410.pdf";
        assert_eq!(post1.attachments[0].url, expected_attachment_url1);
        assert_eq!(post1.attachments[0].mime_type, Some("application/pdf".to_string()));

        // Post 2: 1626
        let post2 = &actual_posts[1];
        assert_eq!(post2.id, "1626");
        assert_eq!(post2.url, "https://aix.ssu.ac.kr/notice_view.html?category=1&idx=1626");
        assert_eq!(post2.title, "[숭실대학일자리플러스사업단] 2025학년도 온라인 직무특강_잇다 안내");
        assert_eq!(post2.created_at, datetime!(2025-06-11 00:00:00 UTC));
        let expected_content2 = "<p>온라인 직무특강 내용입니다.</p>";
        assert_eq!(post2.content.trim(), expected_content2.trim());
        let expected_description2_full = "온라인 직무특강 내용입니다.";
         let expected_description2 = if expected_description2_full.len() > 100 {
             expected_description2_full.chars().take(100).collect::<String>() + "..."
        } else {
            expected_description2_full.to_string()
        };
        assert_eq!(post2.description.as_ref().unwrap(), &expected_description2);
        assert_eq!(post2.attachments.len(), 0);
    }

    #[tokio::test]
    #[ignore] // Ignored by default to prevent running in CI without network access / against live site
    async fn test_live_url_request_and_parse() {
        let plugin = AixPlugin::new();
        let result = plugin.crawl(1).await;

        match &result {
            Ok(posts) => {
                assert_eq!(posts.len(), 1, "Should fetch exactly one post when limit is 1.");
                let post = &posts[0];

                assert!(!post.id.is_empty(), "Post ID should not be empty.");
                assert!(!post.title.is_empty(), "Post title should not be empty.");
                assert!(post.url.starts_with("https://aix.ssu.ac.kr/notice_view.html"), "Post URL should start with the correct base.");
                assert!(!post.content.is_empty(), "Post content (HTML) should not be empty.");

                // Check if created_at is a recent year (e.g., not before 2020)
                let current_year = time::OffsetDateTime::now_utc().year();
                assert!(post.created_at.year() >= 2020 && post.created_at.year() <= current_year + 1, // Allow for next year if crawling at year end
                        "Post created_at year ({}) seems invalid.", post.created_at.year());

                log::info!("Fetched live post: ID={}, Title='{}', URL='{}', CreatedAt='{}'",
                         post.id, post.title, post.url, post.created_at);

                for attachment in &post.attachments {
                    assert!(attachment.url.starts_with("https://aix.ssu.ac.kr"),
                            "Attachment URL '{}' should start with the correct base.", attachment.url);
                    log::info!("Attachment: Name='{:?}', URL='{}'", attachment.name, attachment.url);
                }
                log::info!("Successfully fetched and parsed live post. Content snippet: {}...", post.content.chars().take(100).collect::<String>());

            }
            Err(e) => {
                panic!("AixPlugin.crawl(1) failed: {:?}", e);
            }
        }
        // Ensure the test reports success if all assertions pass
        assert!(result.is_ok());
    }
}
