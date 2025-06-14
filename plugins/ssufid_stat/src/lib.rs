use std::sync::Arc;
use std::time::Duration;

use futures::{StreamExt, stream::FuturesOrdered};
use reqwest::cookie::Jar;
// USER_AGENT from reqwest::header is not directly used, using a const string instead.
use scraper::{ElementRef, Html, Selector};
use thiserror::Error;
use time::{Date, OffsetDateTime, PrimitiveDateTime, macros::offset};
use tokio::time::sleep;
use url::Url;

use encoding_rs::EUC_KR;
use ssufid::{
    core::{Attachment, SsufidPlugin, SsufidPost},
    error::{PluginError, PluginErrorKind}, // Added PluginErrorKind
};

// --- Constants ---
const USER_AGENT_STRING: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
const REQUEST_DELAY: Duration = Duration::from_millis(500);

// --- Selectors ---
struct Selectors {
    notice_item: Selector,
    item_id_and_url_link: Selector,
    item_author: Selector,
    item_date: Selector,
    pagination_link: Selector,
    post_title: Selector,
    post_date_info: Selector,
    post_content: Selector,
    attachment_item: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            notice_item: Selector::parse("div.table-responsive.notice-table > table.table > tbody > tr").unwrap(),
            item_id_and_url_link: Selector::parse("td:nth-child(2) > a").unwrap(),
            item_author: Selector::parse("td:nth-child(3)").unwrap(),
            item_date: Selector::parse("td:nth-child(4)").unwrap(),
            pagination_link: Selector::parse("div.paging > ul.pagination a.page-link").unwrap(),
            post_title: Selector::parse("div.table-responsive.noticeView-table > table.table tr:first-child > th").unwrap(),
            post_date_info: Selector::parse("div.table-responsive.noticeView-table > table.table tr:nth-child(2) > td:first-child").unwrap(),
            post_content: Selector::parse("div.table-responsive.noticeView-table > table.table tr > td.content").unwrap(),
            attachment_item: Selector::parse("div.table-responsive.noticeView-table > table.table tr td:has(i.fas.fa-file-upload) > a[href^=\"/include/fileDownload.php\"]").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
struct SsuStatPostMetadata {
    id: String,
    url: String,
    title_stub: String,
    author: String,
    date_str: String,
}

#[derive(Debug, Error)]
enum SsuStatError {
    #[error("HTML parsing error: {0}")]
    ParseError(String),
    #[error("Metadata extraction failed for field: {0}")]
    MetadataExtractionFailed(&'static str),
    #[error("Post ID (idx) not found in URL: {0}")]
    PostIdNotFound(String),
    #[error("Date parsing failed: {0}")]
    DateParseFailed(String),
    #[error("Content not found on detail page: {0}")]
    ContentNotFound(String),
    #[error("Title not found on detail page: {0}")]
    TitleNotFound(String),
    #[error("Security page detected, could not bypass: {0}")]
    SecurityPageDetected(String),
}

impl From<SsuStatError> for PluginError {
    fn from(err: SsuStatError) -> Self {
        PluginError::parse::<SsuStatPlugin>(err.to_string())
    }
}

pub struct SsuStatPlugin {
    client: reqwest::Client,
    selectors: Arc<Selectors>,
}

impl Default for SsuStatPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SsuStatPlugin {
    pub fn new() -> Self {
        let cookie_jar = Arc::new(Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(cookie_jar)
            .user_agent(USER_AGENT_STRING)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");
        Self {
            client,
            selectors: Arc::new(Selectors::new()),
        }
    }

    fn extract_text(element: &ElementRef, selector: &Selector) -> Option<String> {
        element
            .select(selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_html(element: &ElementRef, selector: &Selector) -> Option<String> {
        element
            .select(selector)
            .next()
            .map(|el| el.inner_html().trim().to_string())
    }

    #[allow(dead_code)]
    fn extract_attr(element: &ElementRef, selector: &Selector, attr: &str) -> Option<String> {
        element
            .select(selector)
            .next()
            .and_then(|el| el.value().attr(attr).map(str::to_string))
    }

    async fn fetch_html_content(&self, url: &str) -> Result<String, PluginError> {
        tracing::debug!("Attempt 1: Fetching {}", url);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let response_bytes = response
            .bytes()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let (html_cow, _, _) = EUC_KR.decode(&response_bytes);
        let mut html = html_cow.into_owned();

        if html.contains("자동등록방지를 위해 보안절차를 거치고 있습니다")
            || html.contains("Please prove that you are human")
        {
            tracing::warn!(
                "Security page detected on first attempt for {}. Retrying with cookie jar only.",
                url
            );
            sleep(REQUEST_DELAY).await;
            tracing::debug!("Attempt 2: Re-Fetching {}", url);
            let response2 = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
            let response_bytes2 = response2
                .bytes()
                .await
                .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
            let (html_cow2, _, _) = EUC_KR.decode(&response_bytes2);
            html = html_cow2.into_owned();

            if html.contains("자동등록방지를 위해 보안절차를 거치고 있습니다")
                || html.contains("Please prove that you are human")
            {
                let ck_url = if url.contains('?') {
                    format!("{}&ckattempt=1", url)
                } else {
                    format!("{}?ckattempt=1", url)
                };
                tracing::warn!(
                    "Security page still detected. Attempt 3: Fetching with ckattempt=1: {}",
                    ck_url
                );
                sleep(REQUEST_DELAY).await;
                let response3 = self
                    .client
                    .get(&ck_url)
                    .send()
                    .await
                    .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
                let response_bytes3 = response3
                    .bytes()
                    .await
                    .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
                let (html_cow3, _, _) = EUC_KR.decode(&response_bytes3);
                html = html_cow3.into_owned();
                if html.contains("자동등록방지를 위해 보안절차를 거치고 있습니다")
                    || html.contains("Please prove that you are human")
                {
                    tracing::error!(
                        "Failed to bypass security page for {} after multiple attempts.",
                        url
                    );
                    return Err(SsuStatError::SecurityPageDetected(url.to_string()).into());
                }
            }
        }
        Ok(html)
    }

    fn get_last_page_number(&self, document: &Html) -> Result<u32, PluginError> {
        let mut max_page = 1;
        for element in document.select(&self.selectors.pagination_link) {
            if let Some(onclick_attr) = element.value().attr("onclick") {
                if onclick_attr.starts_with("fnGoPage(") {
                    if let Some(num_str) = onclick_attr.split(&['(', ')']).nth(1) {
                        if let Ok(page_num) = num_str.parse::<u32>() {
                            if page_num > max_page {
                                max_page = page_num;
                            }
                        }
                    }
                }
            } else if let Some(href_attr) = element.value().attr("href") {
                if href_attr.contains("page=") {
                    let base_for_pagination = Url::parse(Self::BASE_URL).map_err(|e| {
                        SsuStatError::ParseError(format!(
                            "Base URL for pagination parse error: {}",
                            e
                        ))
                    })?;
                    if let Ok(url_obj) = base_for_pagination.join(href_attr) {
                        if let Some(page_val) = url_obj
                            .query_pairs()
                            .find_map(|(k, v)| if k == "page" { Some(v) } else { None })
                        {
                            if let Ok(page_num) = page_val.parse::<u32>() {
                                if page_num > max_page {
                                    max_page = page_num;
                                }
                            }
                        }
                    }
                }
            }
        }
        for element in document.select(&self.selectors.pagination_link) {
            let link_text = element.text().collect::<String>();
            if link_text.contains(">>") || link_text.contains("»") {
                if let Some(onclick_attr) = element.value().attr("onclick") {
                    if let Some(num_str) = onclick_attr.split(&['(', ')']).nth(1) {
                        if let Ok(page_num) = num_str.parse::<u32>() {
                            return Ok(page_num);
                        }
                    }
                } else if let Some(href_attr) = element.value().attr("href") {
                    let base_for_pagination = Url::parse(Self::BASE_URL).map_err(|e| {
                        SsuStatError::ParseError(format!(
                            "Base URL for pagination parse error: {}",
                            e
                        ))
                    })?;
                    if let Ok(url_obj) = base_for_pagination.join(href_attr) {
                        if let Some(page_val) = url_obj
                            .query_pairs()
                            .find_map(|(k, v)| if k == "page" { Some(v) } else { None })
                        {
                            if let Ok(page_num) = page_val.parse::<u32>() {
                                return Ok(page_num);
                            }
                        }
                    }
                }
            }
        }
        Ok(max_page)
    }

    async fn fetch_page_posts_metadata(
        &self,
        page: u32,
    ) -> Result<(Vec<SsuStatPostMetadata>, bool), PluginError> {
        let page_url = format!("{}/notice.html?category=1&page={}", Self::BASE_URL, page);
        tracing::info!("Fetching metadata from page: {}", page_url);
        let html = self.fetch_html_content(&page_url).await?;
        let document = Html::parse_document(&html);
        let mut posts_metadata = Vec::new();
        for item_element in document.select(&self.selectors.notice_item) {
            let url_el_opt = item_element
                .select(&self.selectors.item_id_and_url_link)
                .next();
            if url_el_opt.is_none() {
                continue;
            }
            let url_el = url_el_opt.unwrap();
            let raw_url = url_el
                .value()
                .attr("href")
                .ok_or(SsuStatError::MetadataExtractionFailed("URL"))?
                .to_string();
            let base_page_url = Url::parse(&page_url).map_err(|e| {
                SsuStatError::ParseError(format!("Base page URL parse error: {}", e))
            })?;
            let full_url = base_page_url
                .join(&raw_url)
                .map_err(|e| SsuStatError::ParseError(format!("Joining URL error: {}", e)))?
                .to_string();
            let parsed_url = Url::parse(&full_url)
                .map_err(|_| SsuStatError::ParseError(format!("Invalid URL: {}", full_url)))?;
            let id = parsed_url
                .query_pairs()
                .find_map(|(k, v)| {
                    if k == "idx" {
                        Some(v.into_owned())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| SsuStatError::PostIdNotFound(full_url.clone()))?;
            let title_stub = url_el.text().collect::<String>().trim().to_string();
            let author = Self::extract_text(&item_element, &self.selectors.item_author)
                .ok_or(SsuStatError::MetadataExtractionFailed("author"))?;
            let date_str = Self::extract_text(&item_element, &self.selectors.item_date)
                .ok_or(SsuStatError::MetadataExtractionFailed("date"))?;
            posts_metadata.push(SsuStatPostMetadata {
                id,
                url: full_url,
                title_stub,
                author,
                date_str,
            });
        }
        let last_page_on_site = self.get_last_page_number(&document)?;
        let has_next_page = page < last_page_on_site && !posts_metadata.is_empty();
        Ok((posts_metadata, has_next_page))
    }

    fn parse_date(date_str: &str) -> Result<OffsetDateTime, SsuStatError> {
        let clean_date_str = date_str.split("ㅣ").last().unwrap_or(date_str).trim();
        let format = time::format_description::parse("[year].[month].[day]").map_err(|e| {
            SsuStatError::DateParseFailed(format!("Invalid date format setup: {}", e))
        })?;
        let parsed_date = Date::parse(clean_date_str, &format).map_err(|e| {
            SsuStatError::DateParseFailed(format!("Could not parse '{}': {}", clean_date_str, e))
        })?;
        Ok(PrimitiveDateTime::new(parsed_date, time::Time::MIDNIGHT).assume_offset(offset!(+9)))
    }

    async fn fetch_post(&self, metadata: &SsuStatPostMetadata) -> Result<SsufidPost, PluginError> {
        tracing::info!("Fetching post: {} ({})", metadata.title_stub, metadata.url);
        let html = self.fetch_html_content(&metadata.url).await?;
        let document = Html::parse_document(&html);
        let title = Self::extract_text(&document.root_element(), &self.selectors.post_title)
            .ok_or_else(|| SsuStatError::TitleNotFound(metadata.url.clone()))?;
        let date_info_str =
            Self::extract_text(&document.root_element(), &self.selectors.post_date_info)
                .unwrap_or_else(|| metadata.date_str.clone());
        let created_at = Self::parse_date(&date_info_str)?;
        let content = Self::extract_html(&document.root_element(), &self.selectors.post_content)
            .ok_or_else(|| SsuStatError::ContentNotFound(metadata.url.clone()))?;
        let mut attachments = Vec::new();
        for att_el in document.select(&self.selectors.attachment_item) {
            if let Some(href) = att_el.value().attr("href") {
                let att_name = att_el.text().collect::<String>().trim().to_string();
                let base_post_url = Url::parse(&metadata.url).map_err(|e| {
                    SsuStatError::ParseError(format!("Base post URL parse error: {}", e))
                })?;
                let att_url = base_post_url
                    .join(href)
                    .map_err(|e| {
                        SsuStatError::ParseError(format!("Joining attachment URL error: {}", e))
                    })?
                    .to_string();
                attachments.push(Attachment {
                    name: Some(att_name),
                    url: att_url,
                    mime_type: None,
                });
            }
        }
        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            title,
            author: Some(metadata.author.clone()),
            created_at,
            updated_at: None,
            content,
            attachments,
            category: vec!["공지사항".to_string()],
            thumbnail: None,
            description: None,
            metadata: None,
        })
    }
}

impl SsufidPlugin for SsuStatPlugin {
    const IDENTIFIER: &'static str = "stat.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 정보통계보험수리학과 공지사항";
    const DESCRIPTION: &'static str = "정보통계보험수리학과 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://stat.ssu.ac.kr";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut all_posts = Vec::new();
        let mut current_page = 1;
        let mut has_next_page = true;
        let mut consecutive_empty_pages = 0;
        while has_next_page && all_posts.len() < posts_limit as usize {
            if current_page > 1 {
                sleep(REQUEST_DELAY).await;
            }
            let (metadata_list, next_page_exists) =
                self.fetch_page_posts_metadata(current_page).await?;
            if metadata_list.is_empty() {
                tracing::info!("No posts found on page {}.", current_page);
                consecutive_empty_pages += 1;
                if consecutive_empty_pages >= 3 {
                    tracing::warn!("Stopping crawl after 3 consecutive empty pages.");
                    break;
                }
            } else {
                consecutive_empty_pages = 0;
            }
            has_next_page = next_page_exists;
            let mut tasks = FuturesOrdered::new();
            for metadata in metadata_list {
                if all_posts.len() + tasks.len() >= posts_limit as usize {
                    break;
                }
                let temp_plugin_instance = self.clone_client_for_task();
                let meta_clone = metadata.clone();
                tasks.push_back(async move {
                    sleep(REQUEST_DELAY).await;
                    temp_plugin_instance.fetch_post(&meta_clone).await
                });
            }
            while let Some(post_result) = tasks.next().await {
                match post_result {
                    Ok(post) => {
                        if all_posts.len() < posts_limit as usize {
                            all_posts.push(post);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch post: {}", e);
                        // Corrected matching logic for PluginError of kind Parse
                        if matches!(e.kind(), PluginErrorKind::Parse)
                            && e.message().contains("SecurityPageDetected")
                        {
                            tracing::error!(
                                "Critical: Security page detected during post fetch. Aborting crawl for this plugin."
                            );
                            return Err(e);
                        }
                    }
                }
                if all_posts.len() >= posts_limit as usize {
                    break;
                }
            }
            if all_posts.len() >= posts_limit as usize {
                tracing::info!("Post limit ({}) reached.", posts_limit);
                break;
            }
            current_page += 1;
            if current_page > 50 && posts_limit > 200 {
                tracing::warn!(
                    "Reached page 50, assuming end. Collected {} posts.",
                    all_posts.len()
                );
                break;
            }
            if !has_next_page && posts_limit > all_posts.len() as u32 {
                tracing::info!("No more pages. Collected {} posts.", all_posts.len());
                break;
            }
        }
        Ok(all_posts)
    }
}

impl SsuStatPlugin {
    fn clone_client_for_task(&self) -> Self {
        Self {
            client: self.client.clone(),
            selectors: Arc::clone(&self.selectors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn load_test_html(file_name: &str) -> String {
        if file_name == "notice_list_page.html" {
            return "<!DOCTYPE html><html lang=\"ko\"><head><meta charset=\"utf-8\"><title>List</title></head><body>\n\
<div class=\"table-responsive notice-table\">\n\
    <table class=\"table\">\n\
        <tbody>\n\
            <tr>\n\
                <td>463</td>\n\
                <td><a href=\"notice_view.html?category=1&idx=1256\">2025학년도 기과융 학부생 하계인턴십 모집</a></td>\n\
                <td>관리자</td>\n\
                <td>2025.06.02</td>\n\
                <td>78</td>\n\
            </tr>\n\
            <tr>\n\
                <td>462</td>\n\
                <td><a href=\"notice_view.html?category=1&idx=1253\">2025-1학기 졸업고사 결과</a></td>\n\
                <td>관리자</td>\n\
                <td>2025.03.18</td>\n\
                <td>398</td>\n\
            </tr>\n\
        </tbody>\n\
    </table>\n\
</div>\n\
<div class=\"paging\">\n\
    <ul class=\"pagination justify-content-center\">\n\
        <li class=\"page-item active\" ><a href=\"#none\" class=\"page-link\" >1</a></li>\n\
        <li class=\"page-item\"><a href=\"#none\" class=\"page-link\" onclick=\"fnGoPage(2)\">2</a></li>\n\
        <li class=\"page-item\"><a href=\"#none\" class=\"page-link\" onclick=\"fnGoPage(3)\">3</a></li>\n\
        <li class=\"page-item\"><a href=\"#none\" class=\"page-link\" onclick=\"fnGoPage(24)\" style='margin-left:15px;'>>></a></li>\n\
    </ul>\n\
</div>\n\
</body></html>".to_string();
        } else if file_name == "notice_detail_page_1256.html" {
            return "<!DOCTYPE html><html lang=\"ko\"><head><meta charset=\"utf-8\"><title>Detail</title></head><body>\n\
<div class=\"table-responsive noticeView-table\">\n\
    <table class=\"table\">\n\
        <tr>\n\
            <th colspan=\"2\">2025학년도 기과융 학부생 하계인턴십 모집</th>\n\
        </tr>\n\
        <tr>\n\
            <td><i class=\"far fa-calendar-alt\"></i> 날짜 ㅣ 2025.06.02 </td>\n\
            <td><i class=\"fas fa-mouse\"></i> 조회수 ㅣ 79 </td>\n\
        </tr>\n\
        <tr>\n\
            <td>\n\
                <i class=\"fas fa-file-upload\"></i>\n\
                <span>첨부파일</span> &nbsp; <a href='/include/fileDownload.php?fileName=5c767d695f571e67d44e7644df6287a0.hwp'>붙임1-IIBS_2025학년도-학부생-인턴십-신청서.hwp</a>\n\
            </td>\n\
        </tr>\n\
        <tr>\n\
            <td colspan=\"2\" class=\"content\">\n\
                <p>숭실대학교 기초과학융합연구소에서는 학부 재학생을 대상으로 연구를 직접 수행할 수 있는 기회를 제공하여 우수한 연구 인력 양성 및 대학원 진학을 돕고자 학부생 인턴십 과정을 모집합니다.</p>\n\
                <p><a href=\"https://iibs.ssu.ac.kr/%ea%b2%8c%ec%8b%9c%ed%8c%90/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad/?slug=7386\">사이트 링크</a></p>\n\
            </td>\n\
        </tr>\n\
    </table>\n\
</div>\n\
</body></html>".to_string();
        } else if file_name == "notice_detail_page_1253.html" {
            return "<!DOCTYPE html><html lang=\"ko\"><head><meta charset=\"utf-8\"><title>Detail</title></head><body>\n\
<div class=\"table-responsive noticeView-table\">\n\
    <table class=\"table\">\n\
        <tr><th colspan=\"2\">2025-1학기 졸업고사 결과</th></tr>\n\
        <tr><td><i class=\"far fa-calendar-alt\"></i> 날짜 ㅣ 2025.03.18 </td><td>조회수: 398</td></tr>\n\
        <tr><td colspan=\"2\" class=\"content\"><p> 졸업고사 결과입니다. </p>No attachments.</td></tr>\n\
    </table>\n\
</div>\n\
</body></html>".to_string();
        }
        "".to_string()
    }

    #[tokio::test]
    async fn test_parse_list_page_metadata_extraction() {
        let plugin = SsuStatPlugin::new();
        let html = load_test_html("notice_list_page.html");
        let document = Html::parse_document(&html);
        let page_url_for_joining = "http://stat.ssu.ac.kr/notice.html?category=1&page=1";
        let mut posts_metadata = Vec::new();
        for item_element in document.select(&plugin.selectors.notice_item) {
            let url_el_opt = item_element
                .select(&plugin.selectors.item_id_and_url_link)
                .next();
            if url_el_opt.is_none() {
                continue;
            }
            let url_el = url_el_opt.unwrap();
            let raw_url = url_el.value().attr("href").unwrap().to_string();
            let base_page_url =
                Url::parse(page_url_for_joining).expect("Base page URL should be valid");
            let full_url = base_page_url
                .join(&raw_url)
                .expect("Failed to join URL for list item")
                .to_string();
            let parsed_url = Url::parse(&full_url).unwrap();
            let id = parsed_url
                .query_pairs()
                .find_map(|(k, v)| {
                    if k == "idx" {
                        Some(v.into_owned())
                    } else {
                        None
                    }
                })
                .expect("idx not found in test URL");
            let title_stub = url_el.text().collect::<String>().trim().to_string();
            let author =
                SsuStatPlugin::extract_text(&item_element, &plugin.selectors.item_author).unwrap();
            let date_str =
                SsuStatPlugin::extract_text(&item_element, &plugin.selectors.item_date).unwrap();
            posts_metadata.push(SsuStatPostMetadata {
                id,
                url: full_url,
                title_stub,
                author,
                date_str,
            });
        }
        assert_eq!(posts_metadata.len(), 2, "Should parse 2 valid post items.");
        assert_eq!(posts_metadata[0].id, "1256");
        assert_eq!(
            posts_metadata[0].title_stub,
            "2025학년도 기과융 학부생 하계인턴십 모집"
        );
        assert_eq!(posts_metadata[0].author, "관리자");
        assert_eq!(posts_metadata[0].date_str, "2025.06.02");
        assert_eq!(posts_metadata[1].id, "1253");
        assert_eq!(posts_metadata[1].title_stub, "2025-1학기 졸업고사 결과");
    }

    #[tokio::test]
    async fn test_get_last_page_number_parsing() {
        let plugin = SsuStatPlugin::new();
        let html = load_test_html("notice_list_page.html");
        let document = Html::parse_document(&html);
        assert_eq!(
            plugin.get_last_page_number(&document).unwrap(),
            24,
            "Last page should be 24 from '>>' link."
        );
    }

    #[tokio::test]
    async fn test_parse_full_post_detail_1256() {
        let plugin = SsuStatPlugin::new();
        let metadata = SsuStatPostMetadata {
            id: "1256".to_string(),
            url: "http://stat.ssu.ac.kr/notice_view.html?category=1&idx=1256".to_string(),
            title_stub: "2025학년도 기과융 학부생 하계인턴십 모집".to_string(),
            author: "관리자".to_string(),
            date_str: "2025.06.02".to_string(),
        };
        let html = load_test_html("notice_detail_page_1256.html");
        let document = Html::parse_document(&html);
        let title =
            SsuStatPlugin::extract_text(&document.root_element(), &plugin.selectors.post_title)
                .unwrap();
        assert_eq!(title, "2025학년도 기과융 학부생 하계인턴십 모집");
        let date_info_str =
            SsuStatPlugin::extract_text(&document.root_element(), &plugin.selectors.post_date_info)
                .unwrap_or_else(|| metadata.date_str.clone());
        let created_at = SsuStatPlugin::parse_date(&date_info_str).unwrap();
        assert_eq!(created_at.date(), date!(2025 - 06 - 02));
        let content =
            SsuStatPlugin::extract_html(&document.root_element(), &plugin.selectors.post_content)
                .unwrap();
        assert!(content.contains("숭실대학교 기초과학융합연구소"));
        let mut attachments = Vec::new();
        for att_el in document.select(&plugin.selectors.attachment_item) {
            if let Some(href) = att_el.value().attr("href") {
                let att_name = att_el.text().collect::<String>().trim().to_string();
                let base_post_url =
                    Url::parse(&metadata.url).expect("Base post URL should be valid");
                let att_url = base_post_url
                    .join(href)
                    .expect("Failed to join attachment URL")
                    .to_string();
                attachments.push(Attachment {
                    name: Some(att_name),
                    url: att_url,
                    mime_type: None,
                });
            }
        }
        assert_eq!(attachments.len(), 1);
        assert_eq!(
            attachments[0].name.as_deref(),
            Some("붙임1-IIBS_2025학년도-학부생-인턴십-신청서.hwp")
        );
        assert_eq!(
            attachments[0].url,
            "http://stat.ssu.ac.kr/include/fileDownload.php?fileName=5c767d695f571e67d44e7644df6287a0.hwp"
        );
    }

    #[test]
    fn test_date_parsing() {
        assert_eq!(
            SsuStatPlugin::parse_date("2024.01.01").unwrap().date(),
            date!(2024 - 01 - 01)
        );
        assert_eq!(
            SsuStatPlugin::parse_date("날짜 ㅣ 2023.12.31")
                .unwrap()
                .date(),
            date!(2023 - 12 - 31)
        );
    }
}
