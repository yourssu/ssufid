use scraper::{Html, Selector};
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

#[derive(Debug)]
struct CseMetadata {
    id: String,
    url: String,
    author: String,
    created_at: time::OffsetDateTime,
}

struct Selectors {
    // in the notice list page
    table: Selector,
    tr: Selector,
    url: Selector,
    author: Selector,
    created_at: Selector,

    // in the content page
    title: Selector,
    thumbnail: Selector,
    content: Selector,
    attachments: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            table: Selector::parse("#bo_list > div.notice_list > table > tbody").unwrap(),
            tr: Selector::parse("tr").unwrap(),
            url: Selector::parse("td.td_subject > div > a").unwrap(),
            author: Selector::parse("td.td_name.sv_use > span").unwrap(),
            created_at: Selector::parse("td.td_datetime").unwrap(),
            title: Selector::parse("#bo_v_title > span").unwrap(),
            thumbnail: Selector::parse("#bo_v_con img").unwrap(),
            content: Selector::parse("#bo_v_con").unwrap(),
            attachments: Selector::parse("#bo_v_file > ul > li > a").unwrap(),
        }
    }
}

pub struct CsePlugin {
    selectors: Selectors,
}

impl SsufidPlugin for CsePlugin {
    const IDENTIFIER: &'static str = "cse.ssu.ac.kr/bachelor";
    const TITLE: &'static str = "숭실대학교 컴퓨터학부 학사 공지사항";
    const DESCRIPTION: &'static str =
        "숭실대학교 컴퓨터학부 홈페이지의 학사 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://cse.ssu.ac.kr/bbs/board.php?bo_table=notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut remain = posts_limit as usize;
        let mut page = 1;
        let mut ret = vec![];

        while remain > 0 {
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
}

impl Default for CsePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl CsePlugin {
    const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_metadata(&self, page: u32) -> Result<Vec<CseMetadata>, PluginError> {
        let page_url = format!("{}/&page={}", Self::BASE_URL, page);

        let html = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list = document
            .select(&self.selectors.table)
            .next()
            .ok_or_else(|| PluginError::parse::<Self>("Table element not found".to_string()))?
            .select(&self.selectors.tr);

        let posts_metadata = notice_list
            .filter_map(|tr| {
                let url = tr
                    .select(&self.selectors.url)
                    .next()
                    .and_then(|a| a.value().attr("href"))?
                    .to_string();

                let id = Url::parse(&url)
                    .ok()?
                    .query_pairs()
                    .find(|(key, _)| key == "wr_id")
                    .map(|(_, value)| value.to_string())?;

                let author = tr
                    .select(&self.selectors.author)
                    .next()
                    .map(|span| span.text().collect::<String>().trim().to_string())?;

                let created_at = {
                    let date = tr
                        .select(&self.selectors.created_at)
                        .next()
                        .map(|element| element.text().collect::<String>().trim().to_string())?;
                    Date::parse(&date, Self::DATE_FORMAT)
                        .ok()?
                        .midnight()
                        .assume_offset(offset!(+09:00))
                };
                Some(CseMetadata {
                    id,
                    url,
                    author,
                    created_at,
                })
            })
            .collect::<Vec<CseMetadata>>();

        Ok(posts_metadata)
    }

    async fn fetch_post(&self, metadata: &CseMetadata) -> Result<SsufidPost, PluginError> {
        let html = reqwest::get(&metadata.url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let title = document
            .select(&self.selectors.title)
            .next()
            .map(|span| span.text().collect::<String>().trim().to_string())
            .ok_or_else(|| PluginError::parse::<Self>("Title element not found".to_string()))?;

        let thumbnail = document
            .select(&self.selectors.thumbnail)
            .next()
            .and_then(|img| img.value().attr("src"))
            .unwrap_or_default()
            .to_string();

        let content = document
            .select(&self.selectors.content)
            .next()
            .ok_or_else(|| PluginError::parse::<Self>("Content element not found".to_string()))?
            .child_elements()
            .map(|p| p.text().collect::<String>().replace('\u{a0}', " "))
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
            category: vec![],
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
    use super::*;

    #[tokio::test]
    async fn test_fetch_metadata() {
        let plugin = CsePlugin::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = plugin.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        for metadata in &metadata_list {
            println!("{:?}", metadata);
        }

        let first_metadata = &metadata_list[0];
        assert!(!first_metadata.id.is_empty());
        // assert!(!first_metadata.url.trim().starts_with("https"));
        assert!(
            first_metadata.created_at.year() >= 2025,
            "Created date should be recent"
        );
    }

    #[tokio::test]
    async fn test_fetch_post() {
        let plugin = CsePlugin::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = plugin.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        let first_metadata = &metadata_list[0];

        let post = plugin.fetch_post(&first_metadata).await.unwrap();
        assert!(!post.title.is_empty());
    }

    #[tokio::test]
    async fn test_crawl() {
        let posts_limit = 100;
        let plugin = CsePlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
