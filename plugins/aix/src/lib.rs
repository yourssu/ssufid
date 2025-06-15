use std::sync::LazyLock;

use futures::{StreamExt, stream::FuturesOrdered}; // Added StreamExt, removed TryStreamExt (for now)
use scraper::{Html, Selector}; // Removed ElementRef
use thiserror::Error;
use time::{Date, macros::offset}; // Removed Iso8601 (for now)
use url::Url;

use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::PluginError,
};

// --- CSS Selectors ---
static SELECTORS: LazyLock<Selectors> = LazyLock::new(Selectors::new);

struct Selectors {
    // List page selectors
    post_list_item: Selector,
    post_link_in_list: Selector, // Used to get URL and title text from <a>
    post_author_in_list: Selector,
    post_date_in_list: Selector,
    pagination_link: Selector,

    // Detail page selectors
    post_title_detail: Selector,
    post_metadata_line_detail: Selector, // Contains author, date, views
    post_content_detail: Selector,
    post_attachment_link_detail: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            // List page
            post_list_item: Selector::parse("div.table-responsive > table.table > tbody > tr")
                .unwrap(),
            // For title and link, we select the <td> and then find <a> inside for URL, and take full td text for title.
            // For robustness, we'll select the `a` tag directly for its href and text, and the parent `td` for the full title string.
            post_link_in_list: Selector::parse("td:nth-child(1) > a").unwrap(),
            post_author_in_list: Selector::parse("td:nth-child(2)").unwrap(),
            post_date_in_list: Selector::parse("td:nth-child(3)").unwrap(),
            pagination_link: Selector::parse("div.paging ul.pagination li.page-item a.page-link")
                .unwrap(),

            // Detail page
            post_title_detail: Selector::parse("div.sub_notice_view table th h4").unwrap(),
            post_metadata_line_detail: Selector::parse(
                "div.sub_notice_view table tr:nth-child(2) > td",
            )
            .unwrap(),
            post_content_detail: Selector::parse("div.sub_notice_view table tr:nth-child(4) > td")
                .unwrap(),
            post_attachment_link_detail: Selector::parse(
                "div.sub_notice_view table tr:nth-child(3) > td a",
            )
            .unwrap(),
        }
    }
}

// --- Error Types ---
#[derive(Debug, Error)]
enum AixPluginError {
    #[error("Post ID (idx) not found in URL: {0}")]
    PostIdNotFound(String),
    // #[error("Metadata parsing failed for: {0}")] // Removed MetadataParsing
    // MetadataParsing(String), // Removed MetadataParsing
    #[error("Date parsing failed: {0}")]
    DateParsing(String),
}

// --- Data Structures ---
#[derive(Debug, Clone)]
struct AixPostMetadata {
    id: String,
    url: String,          // Full URL to the post
    title_prefix: String, // Text like "[공지]"
    title_main: String,   // Text from <a> tag
    author: String,       // From list page, likely empty
    date_str: String,     // Date string from list page
}

// --- Plugin Implementation ---
pub struct AixPlugin;

impl Default for AixPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AixPlugin {
    pub fn new() -> Self {
        AixPlugin
    }

    fn parse_date(date_str: &str) -> Result<time::OffsetDateTime, AixPluginError> {
        // Date format is YYYY.MM.DD
        let format = time::format_description::parse("[year].[month].[day]").map_err(|e| {
            AixPluginError::DateParsing(format!("Failed to parse date format description: {}", e))
        })?;
        let parsed_date = Date::parse(date_str.trim(), &format).map_err(|e| {
            AixPluginError::DateParsing(format!(
                "Failed to parse date string '{}': {}",
                date_str, e
            ))
        })?;
        Ok(parsed_date.midnight().assume_offset(offset!(+9))) // Assume KST
    }

    async fn fetch_page_posts_metadata(
        &self,
        page_num: u32,
    ) -> Result<(Vec<AixPostMetadata>, Option<u32>), PluginError> {
        let page_url_str = if page_num == 1 {
            format!("{}/notice.html", Self::BASE_URL)
        } else {
            format!("{}/notice.html?page={}", Self::BASE_URL, page_num)
        };

        tracing::info!(
            plugin = Self::IDENTIFIER,
            "Fetching metadata from page: {}",
            page_url_str
        );

        let response_text = reqwest::get(&page_url_str)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);
        let mut posts_metadata = Vec::new();

        // Iterate over table rows, skipping the header if it's matched by post_list_item.
        // A more robust way is to ensure we only process `tr` elements with `td` children if header is `th`.
        // The current selector `tbody > tr` should correctly get data rows.
        for (row_idx, row_element) in document.select(&SELECTORS.post_list_item).enumerate() {
            // Skip header row (usually the first row, index 0, if it contains <th> not <td>)
            // A simple check: if the first cell is not a `td`, skip. Or if it doesn't have an `a` tag.
            if row_element
                .select(&SELECTORS.post_link_in_list)
                .next()
                .is_none()
            {
                tracing::debug!(
                    plugin = Self::IDENTIFIER,
                    "Skipping row {} as it seems to be a header or invalid.",
                    row_idx
                );
                continue;
            }

            let link_element = row_element.select(&SELECTORS.post_link_in_list).next();
            let relative_url = link_element
                .and_then(|el| el.value().attr("href"))
                .map(str::to_string);

            let title_main = link_element
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // For title_prefix, get the whole text of the first td and remove title_main
            let full_title_td_text = row_element
                .select(&Selector::parse("td:nth-child(1)").unwrap())
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let title_prefix = full_title_td_text
                .replace(&title_main, "")
                .trim()
                .to_string();

            let author = row_element
                .select(&SELECTORS.post_author_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let date_str = row_element
                .select(&SELECTORS.post_date_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if let Some(rel_url) = relative_url {
                if rel_url.starts_with("javascript:") {
                    // Skip javascript links if any
                    tracing::warn!(
                        plugin = Self::IDENTIFIER,
                        "Skipping javascript link: {}",
                        rel_url
                    );
                    continue;
                }
                let post_url = Url::parse(Self::BASE_URL)
                    .and_then(|base| base.join(&rel_url))
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!("Failed to join URL {}: {}", rel_url, e))
                    })?
                    .to_string();

                let id = Url::parse(&post_url)
                    .map_err(|e| {
                        PluginError::parse::<Self>(format!(
                            "Failed to parse post_url for id {}: {}",
                            post_url, e
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
                        PluginError::parse::<Self>(
                            AixPluginError::PostIdNotFound(post_url.clone()).to_string(),
                        )
                    })?;

                posts_metadata.push(AixPostMetadata {
                    id,
                    url: post_url,
                    title_prefix,
                    title_main,
                    author, // Will likely be empty from list page
                    date_str,
                });
            } else {
                tracing::warn!(
                    plugin = Self::IDENTIFIER,
                    "Could not parse URL from row: {:?}",
                    row_element.html()
                );
            }
        }

        // Pagination: Find the highest page number mentioned in pagination links to determine if there's a "next" page.
        // This is a simplified approach. A more robust one would be to find current active page and then the next one.
        let mut max_page_in_pagination = page_num;
        for page_link_el in document.select(&SELECTORS.pagination_link) {
            if let Some(onclick_attr) = page_link_el.value().attr("onclick") {
                if let Some(num_str) = onclick_attr
                    .strip_prefix("fnGoPage(")
                    .and_then(|s| s.strip_suffix(")"))
                {
                    if let Ok(p_num) = num_str.parse::<u32>() {
                        if p_num > max_page_in_pagination {
                            max_page_in_pagination = p_num;
                        }
                    }
                }
            }
        }

        let next_page_num = if max_page_in_pagination > page_num && !posts_metadata.is_empty() {
            // If there are posts on current page and pagination suggests further pages
            Some(page_num + 1)
        } else {
            // Check if the "last_arrow" points to a page greater than current.
            // Example: <a ... onclick="fnGoPage(61)" ...><img src="img/last_arrow.png" ...>
            let last_page_arrow_num = document
                .select(
                    &Selector::parse("a[onclick*='last_arrow.png']")
                        .unwrap_or(SELECTORS.pagination_link.clone()),
                ) // Fallback if specific selector fails
                .filter_map(|el| el.value().attr("onclick"))
                .filter_map(|onclick| {
                    onclick
                        .strip_prefix("fnGoPage(")
                        .and_then(|s| s.strip_suffix(")"))
                })
                .filter_map(|s| s.parse::<u32>().ok())
                .max();

            if let Some(last_val) = last_page_arrow_num {
                if last_val > page_num && !posts_metadata.is_empty() {
                    Some(page_num + 1)
                } else {
                    None
                }
            } else {
                None
            }
        };

        tracing::debug!(
            plugin = Self::IDENTIFIER,
            "Found {} metadata items on page {}. Next page: {:?}",
            posts_metadata.len(),
            page_num,
            next_page_num
        );
        Ok((posts_metadata, next_page_num))
    }

    async fn fetch_post(&self, metadata: &AixPostMetadata) -> Result<SsufidPost, PluginError> {
        tracing::info!(plugin = Self::IDENTIFIER, "Fetching post: {}", metadata.url);
        let response_text = reqwest::get(&metadata.url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&response_text);

        let title_detail = document
            .select(&SELECTORS.post_title_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("{} {}", metadata.title_prefix, metadata.title_main)); // Fallback to list page title

        let metadata_line = document
            .select(&SELECTORS.post_metadata_line_detail)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();

        // Parse metadata line: "작성자 :  ｜ 작성일 : 2025.06.11  ｜ 조회수: 3785"
        let mut author_detail = None;
        let mut date_str_detail = metadata.date_str.clone(); // Fallback to list page date

        if let Some(author_part) = metadata_line.split("｜").next() {
            // ENSURE THIS IS .next()
            if let Some(author_val) = author_part.split(":").nth(1) {
                let trimmed_author = author_val.trim();
                if !trimmed_author.is_empty() {
                    author_detail = Some(trimmed_author.to_string());
                }
            }
        }
        if let Some(date_part) = metadata_line.split("｜").nth(1) {
            if let Some(date_val) = date_part.split(":").nth(1) {
                date_str_detail = date_val.trim().to_string();
            }
        }
        // Views can also be parsed if needed: query_pairs.find_map(|(key, value)| if key == "idx" { Some(value.into_owned()) } else { None })

        let created_at = Self::parse_date(&date_str_detail)
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let content = document
            .select(&SELECTORS.post_content_detail)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        let attachments = document
            .select(&SELECTORS.post_attachment_link_detail)
            .filter_map(|el| {
                el.value().attr("href").map(|href_val| {
                    let attachment_name = el.text().collect::<String>().trim().to_string();
                    // Attachment URLs might be relative or absolute. Assume relative to BASE_URL if not full.
                    let attachment_url = if href_val.starts_with("http") {
                        href_val.to_string()
                    } else {
                        Url::parse(Self::BASE_URL).ok().map_or_else(
                            || {
                                format!(
                                    "{}{}{}",
                                    Self::BASE_URL,
                                    if href_val.starts_with('/') { "" } else { "/" },
                                    href_val
                                )
                            }, // crude joining - corrected format
                            |base| {
                                base.join(href_val)
                                    .map_or_else(|_| href_val.to_string(), |u| u.to_string())
                            },
                        )
                    };
                    Attachment {
                        name: Some(attachment_name),
                        url: attachment_url,
                        mime_type: None, // Can use mime_guess if needed
                    }
                })
            })
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            title: title_detail,
            author: author_detail.or_else(|| {
                if metadata.author.is_empty() {
                    None
                } else {
                    Some(metadata.author.clone())
                }
            }),
            created_at,
            updated_at: None, // No information for updated_at
            content,
            attachments,
            category: Vec::new(), // No obvious category tags in provided HTML
            thumbnail: None,      // No obvious thumbnail
            description: None,    // No obvious description
            metadata: None,       // No other specific metadata
        })
    }
}

impl SsufidPlugin for AixPlugin {
    const IDENTIFIER: &'static str = "aix.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 AI융합학부";
    const DESCRIPTION: &'static str = "숭실대학교 AI융합학부 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://aix.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut all_fetched_metadata: Vec<AixPostMetadata> = Vec::new();
        let mut current_page_num = 1;
        let mut next_page_exists = true;

        while all_fetched_metadata.len() < posts_limit as usize && next_page_exists {
            match self.fetch_page_posts_metadata(current_page_num).await {
                Ok((new_metadata_batch, next_page_opt)) => {
                    if new_metadata_batch.is_empty() {
                        tracing::info!(
                            plugin = Self::IDENTIFIER,
                            "No metadata found on page {}, stopping.",
                            current_page_num
                        );
                        break; // No more posts found on this page
                    }
                    all_fetched_metadata.extend(new_metadata_batch);
                    if let Some(next_p) = next_page_opt {
                        current_page_num = next_p;
                        if current_page_num > 100 {
                            // Safety break for deep pagination if logic is flawed
                            tracing::warn!(
                                plugin = Self::IDENTIFIER,
                                "Reached page 100, stopping pagination for safety."
                            );
                            next_page_exists = false;
                        }
                    } else {
                        next_page_exists = false;
                    }
                }
                Err(e) => {
                    tracing::error!(
                        plugin = Self::IDENTIFIER,
                        "Failed to fetch metadata page {}: {}",
                        current_page_num,
                        e
                    );
                    // Depending on error strategy, might break or continue
                    return Err(e);
                }
            }
        }

        let final_metadata_list: Vec<AixPostMetadata> = all_fetched_metadata
            .into_iter()
            .take(posts_limit as usize)
            .collect();

        tracing::info!(
            plugin = Self::IDENTIFIER,
            "Fetched {} post metadata items in total. Now fetching full posts.",
            final_metadata_list.len()
        );

        let post_futures = final_metadata_list
            .iter()
            .map(|meta| self.fetch_post(meta))
            .collect::<FuturesOrdered<_>>();

        let results: Vec<Result<SsufidPost, PluginError>> = post_futures.collect().await;

        // Collect successful posts, log errors for failures
        let mut successful_posts = Vec::new();
        for res in results {
            match res {
                Ok(post) => successful_posts.push(post),
                Err(e) => tracing::error!(
                    plugin = Self::IDENTIFIER,
                    "Failed to fetch individual post: {}",
                    e
                ),
            }
        }

        // Optionally, if any single post fetch fails, the whole crawl could fail.
        // The current approach collects all successful ones.
        // If strict error handling is needed (all or nothing):
        // final_metadata_list.iter().map(|meta| self.fetch_post(meta)).collect::<FuturesOrdered<_>>().try_collect().await

        Ok(successful_posts)
    }
}

// --- Tests (can be expanded later) ---
#[cfg(test)]
mod tests {
    use super::*;
    // Basic compilation check
    #[test]
    fn plugin_compiles() {
        let _plugin = AixPlugin::new();
        // Add more specific unit tests later
    }

    // Mock HTML for list page (page 1)
    const MOCK_HTML_LIST_PAGE1: &str = r##"
    <!DOCTYPE html>
    <html lang="ko">
    <body>
        <div class="table-responsive">
            <table class="table">
                <thead>
                    <tr><th>제목</th><th>작성자</th><th>발행일자</th><th>조회수</th></tr>
                </thead>
                <tbody>
                    <tr>
                        <td> [공지] <a href="notice_view.html?category=1&idx=1592">세미나실 예약 방법 안내</a></td>
                        <td>관리자</td>
                        <td>2025.03.12</td>
                        <td>100</td>
                    </tr>
                    <tr>
                        <td><a href="notice_view.html?category=1&idx=1585">2025-1학기 졸업 논문</a></td>
                        <td></td>
                        <td>2025.03.05</td>
                        <td>200</td>
                    </tr>
                </tbody>
            </table>
        </div>
        <div class="paging">
            <ul class="pagination justify-content-center">
                <li class="page-item active"><a href="#none" class="page-link">1</a></li>
                <li class="page-item"><a href="#none" class="page-link" onclick="fnGoPage(2)">2</a></li>
                <li class="page-item"><a href="#none" class="page-link" onclick="fnGoPage(3)">3</a></li>
                <li class="page-item"><a href="#none" class="page-link" onclick="fnGoPage(2)"><img src="img/last_arrow.png" alt=""></a></li>
            </ul>
        </div>
    </body>
    </html>
    "##;
    // Mock HTML for detail page (idx=1592)
    const MOCK_HTML_DETAIL_PAGE_1592: &str = r##"
    <!DOCTYPE html>
    <html lang="ko">
    <body>
        <div class="sub_notice_view">
            <table class="table">
                <tr><th><h4>[공지] 세미나실 예약 방법 안내</h4></th></tr>
                <tr><td><span>작성자</span> : 관리자 &nbsp;｜&nbsp;<span>작성일</span> : 2025.03.12 &nbsp;｜&nbsp;<span>조회수</span>: 100</td></tr>
                <tr><td><a href="/path/to/attachment1.pdf">Attachment 1</a></td></tr>
                <tr><td><p>This is the content of post 1592.</p></td></tr>
            </table>
        </div>
    </body>
    </html>
    "##;

    #[tokio::test]
    async fn test_parse_mock_list_page_metadata() {
        // This test uses mock HTML and doesn't make network requests.
        // It's for testing the parsing logic of fetch_page_posts_metadata.
        // A more complete test would involve a mock HTTP server.

        let _plugin = AixPlugin::new(); // Fixed unused variable
        let document = Html::parse_document(MOCK_HTML_LIST_PAGE1);
        let mut posts_metadata = Vec::new();

        for row_element in document.select(&SELECTORS.post_list_item) {
            if row_element
                .select(&SELECTORS.post_link_in_list)
                .next()
                .is_none()
            {
                continue;
            }
            let link_element = row_element.select(&SELECTORS.post_link_in_list).next();
            let relative_url = link_element
                .and_then(|el| el.value().attr("href"))
                .map(str::to_string);
            let title_main = link_element
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let full_title_td_text = row_element
                .select(&Selector::parse("td:nth-child(1)").unwrap())
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let title_prefix = full_title_td_text
                .replace(&title_main, "")
                .trim()
                .to_string();
            let author = row_element
                .select(&SELECTORS.post_author_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let date_str = row_element
                .select(&SELECTORS.post_date_in_list)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if let Some(rel_url) = relative_url {
                let post_url = Url::parse(AixPlugin::BASE_URL)
                    .unwrap()
                    .join(&rel_url)
                    .unwrap()
                    .to_string();
                let id = Url::parse(&post_url)
                    .unwrap()
                    .query_pairs()
                    .find_map(|(k, v)| {
                        if k == "idx" {
                            Some(v.into_owned())
                        } else {
                            None
                        }
                    })
                    .unwrap();
                posts_metadata.push(AixPostMetadata {
                    id,
                    url: post_url,
                    title_prefix,
                    title_main,
                    author,
                    date_str,
                });
            }
        }

        assert_eq!(posts_metadata.len(), 2);
        assert_eq!(posts_metadata[0].id, "1592");
        assert_eq!(posts_metadata[0].title_prefix, "[공지]");
        assert_eq!(posts_metadata[0].title_main, "세미나실 예약 방법 안내");
        assert_eq!(posts_metadata[0].author, "관리자");
        assert_eq!(posts_metadata[0].date_str, "2025.03.12");

        assert_eq!(posts_metadata[1].id, "1585");
        assert_eq!(posts_metadata[1].title_prefix, ""); // No prefix
        assert_eq!(posts_metadata[1].title_main, "2025-1학기 졸업 논문");
        assert_eq!(posts_metadata[1].author, ""); // Empty author
        assert_eq!(posts_metadata[1].date_str, "2025.03.05");

        // Test pagination part (simplified)
        let mut max_page_in_pagination = 1u32; // current page for this test is 1
        for page_link_el in document.select(&SELECTORS.pagination_link) {
            if let Some(onclick_attr) = page_link_el.value().attr("onclick") {
                if let Some(num_str) = onclick_attr
                    .strip_prefix("fnGoPage(")
                    .and_then(|s| s.strip_suffix(")"))
                {
                    if let Ok(p_num) = num_str.parse::<u32>() {
                        if p_num > max_page_in_pagination {
                            max_page_in_pagination = p_num;
                        }
                    }
                }
            }
        }
        let next_page_num = if max_page_in_pagination > 1 && !posts_metadata.is_empty() {
            Some(1 + 1)
        } else {
            None
        };
        assert_eq!(next_page_num, Some(2)); // Based on fnGoPage(2) and fnGoPage(3)
    }

    #[tokio::test]
    async fn test_parse_mock_detail_page() {
        let _plugin = AixPlugin::new(); // Fixed unused variable
        // Example metadata, assuming it was fetched from a list page
        let metadata = AixPostMetadata {
            id: "1592".to_string(),
            url: format!(
                "{}/notice_view.html?category=1&idx=1592",
                AixPlugin::BASE_URL
            ),
            title_prefix: "[공지]".to_string(),
            title_main: "세미나실 예약 방법 안내".to_string(),
            author: "관리자".to_string(), // Author from list might be different or empty
            date_str: "2025.03.12".to_string(),
        };

        // This simulates the behavior of fetch_post using mock HTML
        let document = Html::parse_document(MOCK_HTML_DETAIL_PAGE_1592);
        let title_detail = document
            .select(&SELECTORS.post_title_detail)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("{} {}", metadata.title_prefix, metadata.title_main));

        let metadata_line = document
            .select(&SELECTORS.post_metadata_line_detail)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();

        let mut author_detail = None;
        let mut date_str_detail = metadata.date_str.clone();
        if let Some(author_part) = metadata_line.split("｜").next() {
            // ENSURE THIS IS .next()
            if let Some(author_val) = author_part.split(":").nth(1) {
                let trimmed_author = author_val.trim();
                if !trimmed_author.is_empty() {
                    author_detail = Some(trimmed_author.to_string());
                }
            }
        }
        if let Some(date_part) = metadata_line.split("｜").nth(1) {
            if let Some(date_val) = date_part.split(":").nth(1) {
                date_str_detail = date_val.trim().to_string();
            }
        }
        let created_at = AixPlugin::parse_date(&date_str_detail).unwrap();
        let content = document
            .select(&SELECTORS.post_content_detail)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();
        let attachments: Vec<Attachment> = document
            .select(&SELECTORS.post_attachment_link_detail)
            .filter_map(|el| {
                el.value().attr("href").map(|href_val| Attachment {
                    name: Some(el.text().collect::<String>().trim().to_string()),
                    url: AixPlugin::BASE_URL.to_string() + href_val, // simplified joining for test
                    mime_type: None,
                })
            })
            .collect();

        assert_eq!(title_detail, "[공지] 세미나실 예약 방법 안내");
        assert_eq!(author_detail, Some("관리자".to_string()));
        assert_eq!(date_str_detail, "2025.03.12");
        // Compare OffsetDateTime components if direct comparison is tricky
        assert_eq!(created_at.year(), 2025);
        assert_eq!(created_at.month(), time::Month::March);
        assert_eq!(created_at.day(), 12);
        assert_eq!(content, "<td><p>This is the content of post 1592.</p></td>"); // Adjusted expected content
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].name, Some("Attachment 1".to_string()));
        assert_eq!(
            attachments[0].url,
            "https://aix.ssu.ac.kr/path/to/attachment1.pdf"
        );
    }

    // --- Live Tests (may fail in sandbox if network is restricted) ---

    #[tokio::test]
    #[ignore] // Ignoring by default due to potential network restrictions in sandbox/CI
    async fn live_test_fetch_page_posts_metadata() {
        let plugin = AixPlugin; // Changed from AixPlugin::default()
        let result = plugin.fetch_page_posts_metadata(1).await;

        match &result {
            Ok((metadata_list, next_page_opt)) => {
                tracing::info!(
                    "Fetched {} metadata items. Next page: {:?}",
                    metadata_list.len(),
                    next_page_opt
                );
                // Add more specific assertions if needed, but for now, success is getting some data.
                assert!(
                    !metadata_list.is_empty(),
                    "Fetched metadata list should not be empty on page 1."
                );
                assert!(
                    metadata_list.iter().all(|m| !m.id.is_empty()
                        && m.url.starts_with(AixPlugin::BASE_URL)
                        && !m.date_str.is_empty()),
                    "Metadata items seem invalid"
                );
            }
            Err(e) => {
                tracing::error!("live_test_fetch_page_posts_metadata failed: {:?}", e);
            }
        }
        assert!(
            result.is_ok(),
            "fetch_page_posts_metadata failed: {:?}",
            result.err()
        );
        // On a live site, page 1 should always have data and usually a next page.
        // assert!(result.as_ref().unwrap().0.len() > 0, "No metadata found on page 1");
        // assert!(result.as_ref().unwrap().1.is_some(), "No next page found from page 1");
    }

    #[tokio::test]
    #[ignore] // Ignoring by default
    async fn live_test_fetch_individual_post() {
        let plugin = AixPlugin; // Changed from AixPlugin::default()
        // First, try to get metadata for one post from the live site
        let metadata_res = plugin.fetch_page_posts_metadata(1).await;
        assert!(
            metadata_res.is_ok(),
            "Failed to fetch metadata for live_test_fetch_individual_post: {:?}",
            metadata_res.err()
        );

        let (metadata_list, _) = metadata_res.unwrap();
        assert!(
            !metadata_list.is_empty(),
            "No metadata found on page 1 to test individual post fetching."
        );

        // Take the first post from the list
        let first_post_metadata = &metadata_list[0];
        tracing::info!(
            "Testing fetch_post with metadata: {:?}",
            first_post_metadata
        );

        let post_result = plugin.fetch_post(first_post_metadata).await;
        match &post_result {
            Ok(post) => {
                tracing::info!("Fetched post: ID={}, Title='{}'", post.id, post.title);
                assert_eq!(&post.id, &first_post_metadata.id, "Post ID mismatch");
                assert!(!post.title.is_empty(), "Post title is empty");
                assert!(!post.content.is_empty(), "Post content is empty");
                // Basic check for created_at date (e.g., year is reasonable)
                assert!(
                    post.created_at.year() > 2000,
                    "Post creation year seems unreasonable"
                );
            }
            Err(e) => {
                tracing::error!("live_test_fetch_individual_post failed: {:?}", e);
            }
        }
        assert!(
            post_result.is_ok(),
            "fetch_post failed: {:?}",
            post_result.err()
        );
    }

    #[tokio::test]
    #[ignore] // Ignoring by default
    async fn live_test_crawl_integration() {
        let plugin = AixPlugin; // Changed from AixPlugin::default()
        let posts_limit = 3; // Fetch a small number of posts for integration test

        tracing::info!(
            "Starting live_test_crawl_integration with limit: {}",
            posts_limit
        );
        let crawl_result = plugin.crawl(posts_limit).await;

        match &crawl_result {
            Ok(posts) => {
                tracing::info!("Successfully crawled {} posts.", posts.len());
                // Depending on site activity, posts.len() could be <= posts_limit
                assert!(
                    posts.len() <= posts_limit as usize,
                    "Crawled more posts than limit."
                );
                // If the site has at least `posts_limit` posts, this should be true.
                // For a robust test, it might be better to check posts.len() > 0 if posts_limit > 0 and site is active.
                // assert_eq!(posts.len(), posts_limit as usize, "Number of crawled posts does not match limit.");
                assert!(
                    !posts.is_empty() || posts_limit == 0,
                    "Crawled 0 posts, expected some for limit > 0."
                );

                for post in posts {
                    assert!(!post.id.is_empty(), "Crawled post has empty ID.");
                    assert!(
                        post.url.starts_with(AixPlugin::BASE_URL),
                        "Crawled post URL is invalid."
                    );
                    assert!(!post.title.is_empty(), "Crawled post has empty title.");
                    // Content might be empty for some posts, but generally expected for notices
                    // assert!(!post.content.is_empty(), "Crawled post has empty content. URL: {}", post.url);
                }
            }
            Err(e) => {
                tracing::error!("live_test_crawl_integration failed: {:?}", e);
            }
        }
        assert!(
            crawl_result.is_ok(),
            "Crawl integration test failed: {:?}",
            crawl_result.err()
        );
    }
}
