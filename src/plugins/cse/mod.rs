use log::{info, warn};
use scraper::{Html, Selector};
use thiserror::Error;
use time::{
    Date,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};
use url::Url;

use crate::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub mod bachelor;
pub mod employment;
pub mod graduate;

#[derive(Debug)]
struct CseMetadata {
    category: Option<String>,
    id: String,
    url: String,
    author: String,
    created_at: time::OffsetDateTime,
}

struct CseSelectors {
    // in the notice list page
    table: Selector,
    tr: Selector,
    url: Selector,
    author: Selector,
    created_at: Selector,
    category: Selector,

    // in the content page
    title: Selector,
    thumbnail: Selector,
    content: Selector,
    attachments: Selector,
}

impl CseSelectors {
    fn new() -> Self {
        Self {
            table: Selector::parse("#bo_list > div.notice_list > table > tbody").unwrap(),
            tr: Selector::parse("tr").unwrap(),
            url: Selector::parse("td.td_subject > div > a").unwrap(),
            author: Selector::parse("td.td_name.sv_use > span").unwrap(),
            created_at: Selector::parse("td.td_datetime").unwrap(),
            category: Selector::parse("td.td_num2 > p").unwrap(),
            title: Selector::parse("#bo_v_title > span").unwrap(),
            thumbnail: Selector::parse("#bo_v_con img").unwrap(),
            content: Selector::parse("#bo_v_con").unwrap(),
            attachments: Selector::parse("#bo_v_file > ul > li > a").unwrap(),
        }
    }
}

#[derive(Error, Debug)]
enum CseMetadataError {
    #[error("URL not found error")]
    UrlNotFound,
    #[error("URL parse failed for {0}")]
    UrlParseError(String),
    #[error("ID is empty for URL: {0}")]
    IdEmpty(String),
    #[error("Author not found error for ID: {0}")]
    AuthorNotFound(String),
    #[error("Date element not found for ID: {0}")]
    DateNotFound(String),
    #[error("Date parse failed for {0}")]
    DateParseError(String),
}

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

struct CseCrawler<T: SsufidPlugin> {
    selectors: CseSelectors,
    _marker: std::marker::PhantomData<T>,
}

impl<T> CseCrawler<T>
where
    T: SsufidPlugin,
{
    fn new() -> Self {
        Self {
            selectors: CseSelectors::new(),
            _marker: std::marker::PhantomData,
        }
    }

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut remain = posts_limit as usize;
        let mut page = 1;
        let mut ret = vec![];

        while remain > 0 {
            info!("[{}] page: {}", T::IDENTIFIER, page);
            let metadata = self
                .fetch_metadata(page)
                .await?
                .into_iter()
                .take(remain)
                .collect::<Vec<CseMetadata>>();
            let mut posts = futures::future::join_all(metadata.iter().map(|m| self.fetch_post(m)))
                .await
                .into_iter()
                .collect::<Result<Vec<SsufidPost>, PluginError>>()?;

            ret.append(&mut posts);
            remain -= metadata.len();
            page += 1;
        }
        Ok(ret)
    }

    async fn fetch_metadata(&self, page: u32) -> Result<Vec<CseMetadata>, PluginError> {
        let page_url = format!("{}/&page={}", T::BASE_URL, page);

        let html = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list = document
            .select(&self.selectors.table)
            .next()
            .ok_or(PluginError::parse::<T>(
                "Table element not found".to_string(),
            ))?
            .select(&self.selectors.tr);

        let posts_metadata = notice_list
            .map(|tr| {
                let category = tr
                    .select(&self.selectors.category)
                    .next()
                    .map(|p| p.text().collect::<String>().trim().to_string());

                let url = tr
                    .select(&self.selectors.url)
                    .next()
                    .and_then(|a| a.value().attr("href"))
                    .ok_or(CseMetadataError::UrlNotFound)?
                    .to_string();

                let id = Url::parse(&url)
                    .map_err(|_| CseMetadataError::UrlParseError(url.clone()))?
                    .query_pairs()
                    .find(|(key, value)| key == "wr_id" && !value.is_empty())
                    .map(|(_, value)| value.to_string())
                    .ok_or(CseMetadataError::IdEmpty(url.clone()))?;

                let author = tr
                    .select(&self.selectors.author)
                    .next()
                    .map(|span| span.text().collect::<String>().trim().to_string())
                    .ok_or(CseMetadataError::AuthorNotFound(id.clone()))?;

                let created_at = {
                    let date = tr
                        .select(&self.selectors.created_at)
                        .next()
                        .map(|element| element.text().collect::<String>().trim().to_string())
                        .ok_or(CseMetadataError::DateNotFound(id.clone()))?;
                    Date::parse(&date, DATE_FORMAT)
                        .map_err(|_| CseMetadataError::DateParseError(date.clone()))?
                        .midnight()
                        .assume_offset(offset!(+09:00))
                };
                Ok(CseMetadata {
                    category,
                    id,
                    url,
                    author,
                    created_at,
                })
            })
            .filter_map(|result: Result<CseMetadata, CseMetadataError>| {
                // 경고 메시지 모아서 출력
                // 메타데이터 크롤링 실패 시 크롤링 대상에서 제외
                result
                    .inspect_err(|e| warn!("[{}] {:?}", T::IDENTIFIER, e))
                    .ok()
            })
            .collect::<Vec<CseMetadata>>();

        Ok(posts_metadata)
    }

    async fn fetch_post(&self, metadata: &CseMetadata) -> Result<SsufidPost, PluginError> {
        let html = reqwest::get(&metadata.url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let title = document
            .select(&self.selectors.title)
            .next()
            .map(|span| span.text().collect::<String>().trim().to_string())
            .ok_or(PluginError::parse::<T>(
                "Title element not found".to_string(),
            ))?;

        let thumbnail = document
            .select(&self.selectors.thumbnail)
            .next()
            .and_then(|img| img.value().attr("src"))
            .unwrap_or_default()
            .to_string();

        let content = document
            .select(&self.selectors.content)
            .next()
            .ok_or(PluginError::parse::<T>(
                "Content element not found".to_string(),
            ))?
            .child_elements()
            .map(|p| p.text().collect::<String>().replace('\u{a0}', " ")) // &nbsp 제거
            .collect::<Vec<String>>()
            .join("\n");

        let attachments = document
            .select(&self.selectors.attachments)
            .filter_map(|a| a.value().attr("href"))
            .map(|s| s.to_string())
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            author: metadata.author.clone(),
            title,
            category: metadata.category.clone().map_or(vec![], |c| vec![c]),
            created_at: metadata.created_at,
            updated_at: None,
            thumbnail,
            content,
            attachments,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::plugins::cse::bachelor::CseBachelorPlugin;

    use super::*;

    #[tokio::test]
    async fn test_crawler_fetch_metadata() {
        let crawler: CseCrawler<CseBachelorPlugin> = CseCrawler::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = crawler.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        for metadata in &metadata_list {
            println!("{:?}", metadata);
        }

        let first_metadata = &metadata_list[0];
        assert!(!first_metadata.id.is_empty());
        assert!(first_metadata.url.trim().starts_with("https"));
        assert!(
            first_metadata.created_at.year() >= 2025,
            "Created date should be recent"
        );

        // 학사 공지사항의 첫 게시글은 공지 카테고리 존재
        assert_eq!(first_metadata.category, Some("공지".to_string()));
    }

    #[tokio::test]
    async fn test_crawler_fetch_post() {
        let crawler: CseCrawler<CseBachelorPlugin> = CseCrawler::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = crawler.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        let first_metadata = &metadata_list[0];

        let post = crawler.fetch_post(first_metadata).await.unwrap();
        assert!(!post.title.is_empty());
    }
}
