use std::borrow::Cow;

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
    span: Selector,
    date: Selector,
    category_title: Selector,
    url: Selector,
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
            span: Selector::parse("span").unwrap(),
            date: Selector::parse(".notice_col1 div").unwrap(),
            category_title: Selector::parse(".notice_col3 a span").unwrap(),
            url: Selector::parse(".notice_col3 a").unwrap(),
            content: Selector::parse("div.bg-white.p-4.mb-5 > div:not(.clearfix)").unwrap(),
            last_page: Selector::parse(".next-btn-last").unwrap(),
        }
    }
}

#[derive(Debug)]
struct SsuCatchMetadata {
    id: String,
    title: String,
    category: String,
    url: String,
    created_at: time::OffsetDateTime,
}

impl Default for SsuCatchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SsuCatchPlugin {
    const BASE_URL: &'static str = "https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

    pub fn new() -> Self {
        Self {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_page_posts(&self, page: u32) -> Result<Vec<SsuCatchMetadata>, PluginError> {
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
        let posts = notice_list
            .select(&self.selectors.li)
            .skip(1)
            .map(|li| {
                let date_format = format_description::parse("[year].[month].[day]").unwrap();
                let date_string = li
                    .select(&self.selectors.date)
                    .next()
                    .unwrap()
                    .text()
                    .collect::<String>();
                let date = Date::parse(&date_string, &date_format).unwrap();
                let offset_datetime = date.midnight().assume_offset(offset!(+09:00));

                let url = li
                    .select(&self.selectors.url)
                    .next()
                    .unwrap()
                    .value()
                    .attr("href")
                    .unwrap_or("")
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

                let category_title_span = li.select(&self.selectors.category_title).next().unwrap();

                let spans = category_title_span
                    .select(&self.selectors.span)
                    .map(|span| span.text().collect::<String>())
                    .collect::<Vec<String>>();

                let category = spans[0].clone();
                let title = spans[1].clone();

                SsuCatchMetadata {
                    id,
                    title,
                    category,
                    url,
                    created_at: offset_datetime,
                }
            })
            .collect();

        Ok(posts)
    }

    async fn fetch_post_content(&self, post_url: &str) -> Result<String, PluginError> {
        let response = reqwest::get(post_url)
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let raw_content = document
            .select(&self.selectors.content)
            .next()
            .map(|div| div.text().collect::<String>())
            .unwrap_or("".to_string());

        let content = raw_content
            // &nbsp 제거
            .replace('\u{a0}', " ")
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join(" ");

        Ok(content)
    }

    #[allow(dead_code)]
    fn get_last_page_number(&self, html: &str) -> u32 {
        let document = Html::parse_document(html);

        let last_page_url = document
            .select(&self.selectors.last_page)
            .next()
            .unwrap()
            .value()
            .attr("href")
            .unwrap();
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

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let mut all_posts = Vec::new();

        for page in 1..=posts_limit {
            let metadata_items = self.fetch_page_posts(page).await?;

            for metadata in metadata_items {
                let content = self.fetch_post_content(&metadata.url).await?;

                let post = SsufidPost {
                    id: metadata.id,
                    title: metadata.title,
                    category: metadata.category,
                    url: metadata.url,
                    created_at: metadata.created_at,
                    updated_at: None,
                    content,
                };

                all_posts.push(post);
            }
        }

        for post in &all_posts {
            println!("{:?}", post);
        }

        Ok(all_posts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_page_posts() {
        let ssu_catch_plugin = SsuCatchPlugin::default();

        // 실제 API에 요청하여 1페이지 데이터 가져오기
        let posts = ssu_catch_plugin
            .fetch_page_posts(1)
            .await
            .expect("Failed to fetch page posts");

        assert!(!posts.is_empty(), "Posts should not be empty");

        let first_post = &posts[0];

        println!("First post: {:?}", first_post);

        // 제목, 카테고리, ID, URL 등이 올바르게 추출되었는지 확인
        assert!(!first_post.title.is_empty(), "Title should not be empty");
        assert!(
            !first_post.category.is_empty(),
            "Category should not be empty"
        );
        assert!(!first_post.id.is_empty(), "ID should not be empty");
        assert!(!first_post.url.is_empty(), "URL should not be empty");
        assert!(
            first_post.url.starts_with("https"),
            "URL should start with https"
        );

        // 날짜 형식 검증
        assert!(
            first_post.created_at.year() >= 2025,
            "Created date should be recent"
        );
    }

    #[tokio::test]
    async fn test_fetch_post_content() {
        let ssu_catch_plugin = SsuCatchPlugin::default();

        // 먼저 게시물 목록을 가져와서 첫 번째 게시물의 URL 사용
        let posts = ssu_catch_plugin
            .fetch_page_posts(1)
            .await
            .expect("Failed to fetch page posts");

        assert!(!posts.is_empty(), "Posts should not be empty");

        let first_post_url = &posts[0].url;

        // 실제 게시물 내용 가져오기
        let content = ssu_catch_plugin
            .fetch_post_content(first_post_url)
            .await
            .expect("Failed to fetch post content");

        println!(
            "Content preview: {}",
            content.chars().take(200).collect::<String>()
        );

        // 내용이 비어있지 않은지 확인
        assert!(!content.is_empty(), "Content should not be empty");

        // 내용에 불필요한 공백 문자가 정리되었는지 확인
        assert!(
            !content.contains("\n\n"),
            "Content should not contain consecutive newlines"
        );
        assert!(!content.contains("\t"), "Content should not contain tabs");
        assert!(
            !content.contains("\u{a0}"),
            "Content should not contain non-breaking spaces"
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
