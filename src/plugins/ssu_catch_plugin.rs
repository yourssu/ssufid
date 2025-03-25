use std::borrow::Cow;

use scraper::{Html, Selector};
use url::Url;

use crate::core::{SsufidError, SsufidPlugin, SsufidPost};
use time::{Date, format_description, macros::offset};

pub struct SsuCatchPlugin;

struct Selectors {
    notice: Selector,
    li: Selector,
    span: Selector,
    date: Selector,
    category_title: Selector,
    url: Selector,
    content: Selector,
    last_page: Selector,
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

impl SsuCatchPlugin {
    const BASE_URL: &'static str = "https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

    async fn fetch_page_posts(
        &self,
        page: u32,
        selectors: &Selectors,
    ) -> Result<Vec<SsufidPost>, SsufidError> {
        let page_url = format!("{}/page/{}", SsuCatchPlugin::BASE_URL, page);

        let response = reqwest::get(page_url)
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list = document.select(&selectors.notice).next().unwrap();

        // 첫 번째 li 요소(헤더)는 건너뛰기 위해 skip(1)을 사용
        let posts = notice_list
            .select(&selectors.li)
            .skip(1)
            .map(|li| {
                let date_format = format_description::parse("[year].[month].[day]").unwrap();
                let date_string = li
                    .select(&selectors.date)
                    .next()
                    .unwrap()
                    .text()
                    .collect::<String>();
                let date = Date::parse(&date_string, &date_format).unwrap();
                let offset_datetime = date.midnight().assume_offset(offset!(+09:00));

                let url = li
                    .select(&selectors.url)
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

                let category_title_span = li.select(&selectors.category_title).next().unwrap();

                let spans = category_title_span
                    .select(&selectors.span)
                    .map(|span| span.text().collect::<String>())
                    .collect::<Vec<String>>();

                let category = spans[0].clone();
                let title = spans[1].clone();

                SsufidPost {
                    id,
                    title,
                    category,
                    url,
                    created_at: offset_datetime,
                    updated_at: None,
                    content: "".to_string(),
                }
            })
            .collect();

        Ok(posts)
    }

    async fn fetch_post_content(
        &self,
        post_url: &str,
        selectors: &Selectors,
    ) -> Result<String, SsufidError> {
        let response = reqwest::get(post_url)
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let document = Html::parse_document(&html);

        let raw_content = document
            .select(&selectors.content)
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

    fn get_last_page_number(&self, html: &str, selectors: &Selectors) -> u32 {
        let document = Html::parse_document(html);

        let last_page_url = document
            .select(&selectors.last_page)
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
    const IDENTIFIER: &'static str = "ssu_catch";
    const TITLE: &'static str = "숭실대학교 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 공식 홈페이지의 공지사항을 제공합니다.";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, SsufidError> {
        let selectors = Selectors::new();

        let mut all_posts = Vec::new();

        for page in 1..=posts_limit {
            let mut posts = self.fetch_page_posts(page, &selectors).await?;

            for post in &mut posts {
                let content = self.fetch_post_content(&post.url, &selectors).await?;
                post.content = content;
            }

            all_posts.extend(posts);
        }

        for post in &all_posts {
            println!("{:?}", post);
        }

        Ok(all_posts)
    }
}
