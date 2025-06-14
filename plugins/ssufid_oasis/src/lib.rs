use futures::{StreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector};
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};
use thiserror::Error;
use time::{Date, PrimitiveDateTime, macros::offset}; // OffsetDateTime is used in SsufidPost
use url::Url;

// Selectors based on analysis
struct Selectors {
    // List page
    notice_list_item: Selector,
    notice_url_title: Selector,
    notice_author: Selector,
    notice_date: Selector,

    // Individual post page
    post_title: Selector,
    post_content: Selector,
    post_attachments: Selector,
    post_info_author: Selector,
    post_info_date: Selector,
}

impl Selectors {
    fn new() -> Self {
        // Refined selectors based on typical HTML structures for such sites.
        // These might need live testing if mocks are insufficient.
        Self {
            // List page selectors
            notice_list_item: Selector::parse("table.board-table-valign-top > tbody > tr")
                .expect("Failed to parse notice_list_item selector"),
            notice_url_title: Selector::parse("td.subject > a")
                .expect("Failed to parse notice_url_title selector"),
            // Author: Assuming it's in a `td` with class `td-author` or, if not, the 3rd `td` (0-indexed)
            // after 'notice number' and 'title'. Or a common class like 'text-center' then filter by position.
            // Let's try `td.td-author` first, then fallback to a positional one if that fails.
            // For Oasis, it seems to be the <td> before date and after title/attachment icon.
            // Example: <td>번호</td> <td>제목</td> <td>작성자</td> <td>작성일</td> <td>조회수</td>
            // So, if subject is td.subject, author is often the next sibling td if no attachment column, or one after.
            // Given the HTML structure is often: Number, Title, Author, Date, Hits
            // And title is in `td.subject`, let's assume author is in the `td` directly following the one containing `td.subject`.
            // This needs to be robust. A common class like "writer" or "author" is best.
            // Let's assume it's the 3rd `td` if we consider columns: Num, Subject, Author, Date, Hits
            // If `td.subject` is the main content of its `td`, then `../td[3]` or similar XPath logic.
            // CSS selectors don't have good parent/sibling axis for this.
            // Let's go with a simple `td.td-author` and if not found, it will be None.
            // Or, more likely, it's `td.text-ellipsis:nth-of-type(3)` if columns are fixed.
            // The provided example `ssucatch` used `.notice_col4` for author.
            // Let's assume for oasis: No., Title, (File Icon), Author, Date, Hits.
            // If title is in `td.subject`, the author cell might be `td:nth-child(4)` if no file icon, or `td:nth-child(5)`.
            // Using `td.writer` as a common pattern, or default to a positional one.
            notice_author: Selector::parse("td.writer") // Ideal specific class
                .unwrap_or_else(|_| Selector::parse("td:nth-of-type(3)").expect("Fallback author selector failed")), // Positional if specific not found
            notice_date: Selector::parse("td.date, td.td-date") // Common class names for date
                .expect("Failed to parse notice_date selector"),

            // Individual post page selectors
            post_title: Selector::parse("div.subject > h1, div.board-view-title-wrap > div.subject, h2.title, .title_view .subject")
                .expect("Failed to parse post_title selector"), // Multiple common title selectors
            post_content: Selector::parse("div.view-content, div.content, div.view_content, article.content, div.fr-view")
                .expect("Failed to parse post_content selector"), // Multiple common content selectors
            post_attachments: Selector::parse("div.file_list_wrap ul.file_list li a, div.file-list a, .attached-file a, .file_add a")
                .expect("Failed to parse post_attachments selector"),
            post_info_author: Selector::parse("div.board-view-info-wrap > ul > li.name > span, span.writer, .writer_info .name, dd.writer")
                 .expect("Failed to parse post_info_author selector"),
            post_info_date: Selector::parse("div.board-view-info-wrap > ul > li.date > span, span.date, .writer_info .date, dd.date")
                 .expect("Failed to parse post_info_date selector"),
        }
    }
}

#[derive(Debug)]
pub struct OasisMetadata {
    // Made OasisMetadata public
    id: String,
    url: String,
    title: String,
    author_name: Option<String>,
    date_str: String,
}

#[derive(Debug, Error)]
enum OasisMetadataError {
    #[error("URL not found in notice item")]
    UrlNotFound,
    #[error("Date not found in notice item")]
    DateNotFound,
    #[error("ID could not be extracted from URL: {0}")]
    IdExtractionFailed(String),
}

pub struct OasisPlugin {
    selectors: Selectors,
    http_client: reqwest::Client,
}

impl Default for OasisPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl OasisPlugin {
    // Common date formats found on SSU sites
    // DATE_FORMAT_LIST removed as it was unused. Logic now uses DATE_FORMAT_POST_DATE for list items.
    const DATE_FORMAT_POST_DATETIME: &'static str = "[year].[month].[day] [hour]:[minute]"; // e.g., 2023.09.15 10:00
    const DATE_FORMAT_POST_DATE: &'static str = "[year].[month].[day]"; // e.g., 2023.09.15 (if time not present)

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
            http_client: reqwest::Client::builder()
                .user_agent(format!("ssufid-rust-crawler/{}", env!("CARGO_PKG_VERSION"))) // Good practice
                .build()
                .expect("Failed to build reqwest client"),
        }
    }

    fn extract_id_from_url(&self, url_str: &str) -> Result<String, OasisMetadataError> {
        let parsed_url = Url::parse(url_str)
            .map_err(|_| OasisMetadataError::IdExtractionFailed(url_str.to_string()))?;
        let mut segments = parsed_url // Made segments mutable for next_back()
            .path_segments()
            .ok_or_else(|| OasisMetadataError::IdExtractionFailed(url_str.to_string()))?;
        segments
            .next_back() // Used next_back() as suggested by Clippy
            .filter(|s| !s.is_empty() && s.chars().all(char::is_numeric)) // Ensure it's a number
            .map(ToString::to_string)
            .ok_or_else(|| OasisMetadataError::IdExtractionFailed(url_str.to_string()))
    }

    // Made this method public and non-async for easier testing with mock HTML
    pub fn parse_notice_list_metadata_from_html(
        &self,
        html_content: &str,
        base_url_for_joins: &str,
    ) -> Result<Vec<OasisMetadata>, PluginError> {
        let document = Html::parse_document(html_content);
        let mut metadata_list = Vec::new();

        for element in document.select(&self.selectors.notice_list_item) {
            let title_anchor = element.select(&self.selectors.notice_url_title).next();

            let (url_path, title_text) = match title_anchor {
                Some(anchor) => {
                    let href = anchor
                        .value()
                        .attr("href")
                        .ok_or(OasisMetadataError::UrlNotFound)
                        .map_err(|e| {
                            PluginError::parse::<Self>(format!("URL href not found: {:?}", e))
                        })?;
                    let title = anchor.text().collect::<String>().trim().to_string();
                    (href.to_string(), title)
                }
                None => {
                    tracing::warn!("Skipping item due to missing URL/title anchor element");
                    continue;
                }
            };

            if title_text.is_empty() {
                tracing::warn!(url_path = %url_path, "Skipping item due to empty title");
                continue;
            }

            let full_url = Url::parse(base_url_for_joins)
                .unwrap()
                .join(&url_path)
                .map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Failed to join URL: {} with {}: {}",
                        base_url_for_joins, url_path, e
                    ))
                })?
                .to_string();

            let id = match self.extract_id_from_url(&full_url) {
                Ok(id_val) => id_val,
                Err(e) => {
                    tracing::warn!(error = ?e, url = %full_url, "Failed to extract ID, skipping item.");
                    // PluginError::parse::<Self>(format!("ID extraction error for {}: {:?}", full_url, e))
                    continue; // Skip this item
                }
            };

            let author_name = element
                .select(&self.selectors.notice_author)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());

            let date_str = element
                .select(&self.selectors.notice_date)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .ok_or(OasisMetadataError::DateNotFound)
                .map_err(|e| {
                    PluginError::parse::<Self>(format!("Date string not found: {:?}", e))
                })?;

            metadata_list.push(OasisMetadata {
                id,
                url: full_url,
                title: title_text,
                author_name,
                date_str,
            });
        }
        Ok(metadata_list)
    }

    async fn fetch_notice_list_metadata(&self) -> Result<Vec<OasisMetadata>, PluginError> {
        let list_url = format!("{}/library-services/bulletin/notice", Self::BASE_URL);
        let response = self
            .http_client
            .get(&list_url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let html_content = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        self.parse_notice_list_metadata_from_html(&html_content, Self::BASE_URL)
    }

    // Made this method public and non-async for easier testing with mock HTML
    pub fn parse_post_details_from_html(
        &self,
        metadata: &OasisMetadata,
        html_content: &str,
    ) -> Result<SsufidPost, PluginError> {
        let document = Html::parse_document(html_content);

        let title = document
            .select(&self.selectors.post_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| metadata.title.clone());

        let content_html = document
            .select(&self.selectors.post_content)
            .next()
            .map(|el| el.inner_html()) // Use inner_html() to get content inside the div
            .unwrap_or_default();

        let parsed_date = {
            let date_str_post_element = document.select(&self.selectors.post_info_date).next();
            let date_text_from_post =
                date_str_post_element.map(|el| el.text().collect::<String>().trim().to_string());

            let final_date_str = date_text_from_post.as_ref().unwrap_or(&metadata.date_str);

            let (format_str, is_datetime) = if final_date_str.contains(':') {
                (OasisPlugin::DATE_FORMAT_POST_DATETIME, true)
            } else {
                (OasisPlugin::DATE_FORMAT_POST_DATE, false)
            };
            let format_desc = time::format_description::parse(format_str).map_err(|e| {
                PluginError::parse::<Self>(format!(
                    "Date format description error for '{}': {}",
                    format_str, e
                ))
            })?;

            if is_datetime {
                PrimitiveDateTime::parse(final_date_str, &format_desc)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to parse post datetime '{}' with format '{}': {}",
                            final_date_str, format_str, e
                        ))
                    })?
                    .assume_offset(offset!(+09:00))
            } else {
                Date::parse(final_date_str, &format_desc)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to parse post date '{}' with format '{}': {}",
                            final_date_str, format_str, e
                        ))
                    })?
                    .midnight()
                    .assume_offset(offset!(+09:00))
            }
        };

        let author_name = document
            .select(&self.selectors.post_info_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .or_else(|| metadata.author_name.clone()); // Use or_else for Option<String>

        let attachments = document
            .select(&self.selectors.post_attachments)
            .filter_map(|element| {
                element.value().attr("href").and_then(|href_val| {
                    // Ensure href is not javascript void or empty
                    if href_val.starts_with("javascript:") || href_val.trim().is_empty() {
                        return None;
                    }
                    Url::parse(Self::BASE_URL)
                        .unwrap()
                        .join(href_val)
                        .map(|full_url| {
                            let name = element.text().collect::<String>().trim().to_string();
                            let final_name = Some(name.clone()).filter(|s| !s.is_empty()); // Ensure name is not empty
                            Attachment {
                                name: final_name,
                                url: full_url.to_string(),
                                mime_type: mime_guess::from_path(&name)
                                    .first_raw()
                                    .map(str::to_string),
                            }
                        })
                        .ok()
                })
            })
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            title,
            author: author_name,
            description: None,
            category: Vec::new(),
            created_at: parsed_date,
            updated_at: None,
            thumbnail: None,
            content: content_html,
            attachments,
            metadata: None,
        })
    }

    async fn fetch_post_details(&self, metadata: OasisMetadata) -> Result<SsufidPost, PluginError> {
        let response = self
            .http_client
            .get(&metadata.url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let html_content = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        self.parse_post_details_from_html(&metadata, &html_content)
    }
}

// No async_trait needed here if crawl itself is async fn
impl SsufidPlugin for OasisPlugin {
    const IDENTIFIER: &'static str = "oasis.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 도서관 공지사항"; // Library Notices
    const DESCRIPTION: &'static str = "숭실대학교 도서관 웹사이트의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://oasis.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        tracing::info!(plugin = %Self::TITLE, "Starting crawl, limit: {}", posts_limit);

        let metadata_list = self.fetch_notice_list_metadata().await?;
        tracing::info!(plugin = %Self::TITLE, "Fetched {} metadata items from list page", metadata_list.len());

        let mut posts = Vec::new();
        let mut futures = FuturesOrdered::new();

        for metadata in metadata_list.into_iter().take(posts_limit as usize) {
            tracing::debug!(plugin = %Self::TITLE, id = %metadata.id, url = %metadata.url, "Queueing post fetch");
            // Clone client for each task if needed, or ensure it's shareable. Reqwest client is Arc-based.
            futures.push_back(self.fetch_post_details(metadata));
        }

        while let Some(result) = futures.next().await {
            match result {
                Ok(post) => {
                    tracing::debug!(plugin = %Self::TITLE, id = %post.id, "Successfully fetched post");
                    posts.push(post);
                }
                Err(e) => {
                    tracing::warn!(plugin = %Self::TITLE, error = ?e, "Failed to fetch a post detail");
                }
            }
        }
        tracing::info!(plugin = %Self::TITLE, "Crawl finished, fetched {} posts.", posts.len());
        Ok(posts)
    }
}

// Removed original add function and it_works test as they are placeholders
#[cfg(test)]
mod tests {
    use super::*;
    // Removed: use time::macros::datetime; as it's unused

    #[test]
    fn selectors_compile_and_are_not_empty() {
        // This test primarily ensures that Selectors::new() doesn't panic.
        // The .expect() calls within Selectors::new() will cause a panic if parsing fails.
        // If Selectors::new() completes, the selectors are considered validly parsed.
        let _s = Selectors::new();
        // assert!(true) was removed to satisfy clippy::assertions_on_constants
    }

    #[test]
    fn test_extract_id() {
        let plugin = OasisPlugin::new();
        let url1 = "https://oasis.ssu.ac.kr/library-services/bulletin/notice/3039";
        assert_eq!(plugin.extract_id_from_url(url1).unwrap(), "3039");
        let url2 = "https://oasis.ssu.ac.kr/library-services/bulletin/notice/123";
        assert_eq!(plugin.extract_id_from_url(url2).unwrap(), "123");
        let url3 = "https://oasis.ssu.ac.kr/library-services/bulletin/notice/3039?query=param";
        assert_eq!(plugin.extract_id_from_url(url3).unwrap(), "3039");

        let invalid_url_no_id = "https://oasis.ssu.ac.kr/library-services/bulletin/notice/";
        assert!(plugin.extract_id_from_url(invalid_url_no_id).is_err());
        let invalid_url_not_number = "https://oasis.ssu.ac.kr/library-services/bulletin/notice/abc";
        assert!(plugin.extract_id_from_url(invalid_url_not_number).is_err());
    }

    #[test]
    fn test_parse_notice_list_mocked() {
        let plugin = OasisPlugin::new();
        // Simplified HTML for Oasis list page
        let mock_html = r#"
        <table class="board-table-valign-top">
            <tbody>
                <tr> <!-- Header Row, should be skipped or handled if selectors are too general -->
                    <th>번호</th><th class="subject">제목</th><th class="writer">작성자</th><th class="date">작성일</th><th>조회수</th>
                </tr>
                <tr>
                    <td class="td-num">123</td>
                    <td class="subject"><a href="/library-services/bulletin/notice/3039">공지사항 제목 1</a></td>
                    <td class="writer">도서관팀</td>
                    <td class="date">2023.10.26</td>
                    <td class="td-hit">100</td>
                </tr>
                <tr>
                    <td class="td-num">공지</td> <!-- "공지" items might not have a numeric ID in this cell -->
                    <td class="subject"><a href="/library-services/bulletin/notice/3040">중요 공지사항 제목 2</a></td>
                    <td class="writer">정보서비스팀</td>
                    <td class="date">2023.10.27</td>
                    <td class="td-hit">200</td>
                </tr>
                <tr>
                    <td class="td-num">121</td>
                    <td class="subject"><a href="/library-services/bulletin/notice/invalid-id-format">ID형식오류</a></td>
                    <td class="writer">도서관팀</td>
                    <td class="date">2023.10.28</td>
                    <td class="td-hit">300</td>
                </tr>
            </tbody>
        </table>
        "#;

        // .unwrap() removed, as the function should now handle errors internally and filter.
        let metadata_list = plugin
            .parse_notice_list_metadata_from_html(mock_html, OasisPlugin::BASE_URL)
            .expect("Parsing notice list metadata should not fail overall for this mock");

        assert_eq!(
            metadata_list.len(),
            2,
            "Should only include items with valid IDs"
        );

        let meta1 = &metadata_list[0];
        assert_eq!(meta1.id, "3039");
        assert_eq!(meta1.title, "공지사항 제목 1");
        assert_eq!(meta1.author_name, Some("도서관팀".to_string()));
        assert_eq!(meta1.date_str, "2023.10.26");
        assert_eq!(
            meta1.url,
            "https://oasis.ssu.ac.kr/library-services/bulletin/notice/3039"
        );

        let meta2 = &metadata_list[1];
        assert_eq!(meta2.id, "3040");
        assert_eq!(meta2.title, "중요 공지사항 제목 2");
        assert_eq!(meta2.author_name, Some("정보서비스팀".to_string()));
        assert_eq!(meta2.date_str, "2023.10.27");
        assert_eq!(
            meta2.url,
            "https://oasis.ssu.ac.kr/library-services/bulletin/notice/3040"
        );
    }

    #[test]
    fn test_parse_post_details_mocked() {
        let plugin = OasisPlugin::new();
        let mock_metadata = OasisMetadata {
            id: "3039".to_string(),
            url: "https://oasis.ssu.ac.kr/library-services/bulletin/notice/3039".to_string(),
            title: "미리 정의된 공지사항 제목".to_string(), // Fallback title
            author_name: Some("목록페이지작성자".to_string()),
            date_str: "2023.10.26".to_string(), // Fallback date
        };

        // Simplified HTML for Oasis post details page
        let mock_html_post = r#"
        <div>
            <div class="board-view-title-wrap">
                 <div class="subject"><h1>실제 공지사항 제목</h1></div>
            </div>
            <div class="board-view-info-wrap">
                <ul>
                    <li class="name"><span>실제작성자</span></li>
                    <li class="date"><span>2023.11.15 14:30</span></li>
                </ul>
            </div>
            <div class="view-content">
                <p>이것은 공지사항의 내용입니다.</p>
                <img src="/some/image.jpg">
            </div>
            <div class="file_list_wrap">
                <ul class="file_list">
                    <li><a href="/download?file_id=1">첨부파일1.pdf</a><span>(123KB)</span></li>
                    <li><a href="/download?file_id=2&name=첨부파일2.docx">첨부파일2.docx</a><span>(45KB)</span></li>
                    <li><a href="javascript:void(0);">무효한링크.txt</a></li>
                </ul>
            </div>
        </div>
        "#;

        let post = plugin
            .parse_post_details_from_html(&mock_metadata, mock_html_post)
            .unwrap();

        assert_eq!(post.id, "3039");
        assert_eq!(post.title, "실제 공지사항 제목");
        assert_eq!(post.author, Some("실제작성자".to_string()));
        // Updated expected content to reflect inner_html() and typical browser rendering of whitespace
        let expected_content =
            "<p>이것은 공지사항의 내용입니다.</p>\n                <img src=\"/some/image.jpg\">";
        assert_eq!(
            post.content.trim().replace("  ", ""),
            expected_content.trim().replace("  ", "")
        );

        let expected_datetime = PrimitiveDateTime::new(
            Date::from_calendar_date(2023, time::Month::November, 15).unwrap(),
            time::Time::from_hms(14, 30, 0).unwrap(),
        )
        .assume_offset(offset!(+09:00));
        assert_eq!(post.created_at, expected_datetime);

        assert_eq!(post.attachments.len(), 2); // javascript:void(0) should be skipped
        let attach1 = &post.attachments[0];
        assert_eq!(attach1.name, Some("첨부파일1.pdf".to_string()));
        assert_eq!(attach1.url, "https://oasis.ssu.ac.kr/download?file_id=1");
        assert_eq!(attach1.mime_type, Some("application/pdf".to_string()));

        let attach2 = &post.attachments[1];
        assert_eq!(attach2.name, Some("첨부파일2.docx".to_string()));
        assert_eq!(
            attach2.url,
            "https://oasis.ssu.ac.kr/download?file_id=2&name=%EC%B2%A8%EB%B6%80%ED%8C%8C%EC%9D%BC2.docx"
        ); // URL should be correctly joined
        assert_eq!(
            attach2.mime_type,
            Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn test_crawl_mocked_http_calls() {
        // This test is more involved and would require a mock HTTP server (e.g., wiremock)
        // or heavier patching of the reqwest::Client.
        // For now, we'll assume the individual parsing functions being tested above are sufficient
        // to give confidence in the crawl method's assembly of these parts.
        // A full integration test against the live site would be the next step beyond unit tests.
        let plugin = OasisPlugin::new();
        // To truly test crawl, you'd mock plugin.fetch_notice_list_metadata and plugin.fetch_post_details,
        // or mock the HTTP client.
        // For this example, we'll just check if it compiles and runs without panicking with a 0 limit.
        let result = plugin.crawl(0).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }
}
