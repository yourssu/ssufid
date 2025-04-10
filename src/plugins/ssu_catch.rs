use std::borrow::Cow;

use log::{info, warn};
use scraper::{Html, Selector};
use url::Url;

use crate::{
    core::{SsufidPlugin, SsufidPost},
    error::PluginError,
};
use time::{Date, format_description, macros::offset};
struct Selectors {
    notice: Selector,
    li: Selector,
    url: Selector,
    title: Selector,
    created_at: Selector,
    category: Selector,
    content: Selector,
    #[allow(dead_code)]
    last_page: Selector,
}

pub struct SsuCatchPlugin {
    selectors: Selectors,
}

impl Selectors {
    fn new() -> Self {
        Self {
            notice: Selector::parse(".notice-lists").unwrap(),
            li: Selector::parse("li").unwrap(),
            url: Selector::parse(".notice_col3 a").unwrap(),
            last_page: Selector::parse(".next-btn-last").unwrap(),
            title: Selector::parse("div.bg-white h2").unwrap(),
            category: Selector::parse("div.bg-white span.label").unwrap(),
            created_at: Selector::parse("div.bg-white > div.clearfix > div.float-left.mr-4")
                .unwrap(),
            content: Selector::parse("div.bg-white > div:not(.clearfix)").unwrap(),
        }
    }
}

#[derive(Debug)]
struct SsuCatchMetadata {
    id: String,
    url: String,
}

impl Default for SsuCatchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SsuCatchPlugin {
    const POSTS_PER_PAGE: u32 = 15; // 페이지당 게시글 수
    const DATE_FORMAT: &'static str = "[year]년 [month padding:none]월 [day padding:none]일";

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_page_posts_metadata(
        &self,
        page: u32,
    ) -> Result<Vec<SsuCatchMetadata>, PluginError> {
        let page_url = format!("{}/page/{}", Self::BASE_URL, page);

        let response = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list = document.select(&self.selectors.notice).next().unwrap();

        // 첫 번째 li 요소(헤더)는 건너뛰기 위해 skip(1)을 사용
        let posts_metadata = notice_list
            .select(&self.selectors.li)
            .skip(1)
            .filter_map(|li| {
                let url = li
                    .select(&self.selectors.url)
                    .next()
                    .and_then(|element| element.value().attr("href"))
                    .unwrap_or_default()
                    .to_string();

                let id = Url::parse(&url)
                    .unwrap()
                    .query_pairs()
                    .find_map(
                        |(key, value)| {
                            if key == "slug" { Some(value) } else { None }
                        },
                    )
                    .unwrap_or(Cow::Borrowed(""))
                    .to_string();

                if id.is_empty() {
                    warn!("ID is empty for URL: {}", url);
                    return None;
                }

                Some(SsuCatchMetadata { id, url })
            })
            .collect();

        Ok(posts_metadata)
    }

    async fn fetch_post(
        &self,
        post_metadata: &SsuCatchMetadata,
    ) -> Result<SsufidPost, PluginError> {
        let response = reqwest::get(&post_metadata.url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let title = document
            .select(&self.selectors.title)
            .next()
            .map(|element| element.text().collect::<String>())
            .unwrap_or_default();

        let category = document
            .select(&self.selectors.category)
            .next()
            .map(|element| element.text().collect::<String>())
            .unwrap_or_default();

        let date_format = format_description::parse(Self::DATE_FORMAT).unwrap();
        let date_string = document
            .select(&self.selectors.created_at)
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let created_at = Date::parse(&date_string, &date_format)
            .unwrap()
            .midnight()
            .assume_offset(offset!(+09:00));

        let raw_content = document
            .select(&self.selectors.content)
            .next()
            .map(|div| div.text().collect::<String>())
            .unwrap_or_default();

        let content = raw_content
            // &nbsp 제거
            .replace('\u{a0}', " ")
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join("\n");

        Ok(SsufidPost {
            id: post_metadata.id.clone(),
            title,
            category,
            url: post_metadata.url.clone(),
            created_at,
            updated_at: None,
            content,
        })
    }

    #[allow(dead_code)]
    fn get_last_page_number(&self, html: &str) -> u32 {
        let document = Html::parse_document(html);

        let last_page_url = document
            .select(&self.selectors.last_page)
            .next()
            .and_then(|element| element.value().attr("href"))
            .unwrap_or_default();

        let parsed_last_page_url = Url::parse(last_page_url).unwrap();

        parsed_last_page_url
            .path_segments()
            .unwrap()
            .skip_while(|&segment| segment != "page")
            .nth(1)
            .and_then(|segment| segment.parse().ok())
            .unwrap_or(1)
    }
}

impl SsufidPlugin for SsuCatchPlugin {
    const IDENTIFIER: &'static str = "scatch.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 공식 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let pages = posts_limit / Self::POSTS_PER_PAGE + 1;

        // 모든 페이지 크롤링이 완료될 때까지 대기
        let metadata_results = futures::future::join_all((1..=pages).map(|page| {
            info!("Crawling post metadata from page: {}/{}", page, pages);
            self.fetch_page_posts_metadata(page)
        }))
        .await;

        let all_metadata = metadata_results
            .into_iter()
            .collect::<Result<Vec<_>, PluginError>>()?
            .into_iter()
            .flatten()
            .take(posts_limit as usize)
            .collect::<Vec<SsuCatchMetadata>>();

        // 모든 포스트 크롤링이 완료될 때까지 대기
        let post_results = futures::future::join_all(all_metadata.iter().map(|metadata| {
            info!("Crawling post content for ID: {}", metadata.id);
            self.fetch_post(metadata)
        }))
        .await;

        let all_posts = post_results
            .into_iter()
            .collect::<Result<Vec<SsufidPost>, PluginError>>()?;

        Ok(all_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_page_posts_metadata() {
        let ssu_catch_plugin = SsuCatchPlugin::default();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let posts_metadata = ssu_catch_plugin
            .fetch_page_posts_metadata(1)
            .await
            .expect("Failed to fetch page posts metadata");

        assert!(
            !posts_metadata.is_empty(),
            "Posts metadata should not be empty"
        );

        let first_post_metadata = &posts_metadata[0];

        println!("First post metadata: {:?}", first_post_metadata);

        // ID, URL이 올바르게 추출되었는지 확인
        assert!(!first_post_metadata.id.is_empty(), "ID should not be empty");
        assert!(
            !first_post_metadata.url.is_empty(),
            "URL should not be empty"
        );
    }

    #[tokio::test]
    async fn test_fetch_post() {
        let ssu_catch_plugin = SsuCatchPlugin::default();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let posts_metadata = ssu_catch_plugin
            .fetch_page_posts_metadata(1)
            .await
            .expect("Failed to fetch page posts metadata");

        assert!(
            !posts_metadata.is_empty(),
            "Posts metadata should not be empty"
        );

        let first_post_metadata = &posts_metadata[0];

        // 실제 게시물 가져오기
        let post = ssu_catch_plugin
            .fetch_post(&first_post_metadata)
            .await
            .expect("Failed to fetch post");

        // 제목, 카테고리, 내용 등이 올바르게 추출되었는지 확인
        assert!(!post.title.is_empty(), "Title should not be empty");
        assert!(!post.category.is_empty(), "Category should not be empty");
        assert!(post.url.starts_with("https"), "URL should start with https");

        // 날짜 형식 검증
        assert!(
            post.created_at.year() >= 2025,
            "Created date should be recent"
        );
    }

    #[tokio::test]
    async fn test_get_last_page_number() {
        let ssu_catch_plugin = SsuCatchPlugin::default();

        // 실제 페이지 HTML 가져오기
        let response =
            reqwest::get("https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad")
                .await
                .expect("Failed to fetch HTML");

        let html = response.text().await.expect("Failed to get HTML text");

        // 마지막 페이지 번호 가져오기
        let last_page = ssu_catch_plugin.get_last_page_number(&html);

        println!("Last page number: {}", last_page);

        // 페이지 번호가 1 이상인지 확인
        assert!(last_page >= 1, "Last page number should be at least 1");
    }
}
