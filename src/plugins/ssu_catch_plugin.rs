use std::borrow::Cow;

use scraper::{Html, Selector};
use url::Url;

use crate::core::{SsufidError, SsufidPlugin, SsufidPost};
use time::{Date, format_description, macros::offset};

pub struct SsuCatchPlugin;

impl SsufidPlugin for SsuCatchPlugin {
    const IDENTIFIER: &'static str = "SSU Catch";
    const TITLE: &'static str = "SSU Catch";
    const DESCRIPTION: &'static str = "SSU Catch plugin";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, SsufidError> {
        let url = "https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

        let notice_selector = Selector::parse(".notice-lists").unwrap();
        let li_selector = Selector::parse("li").unwrap();
        let span_selector = Selector::parse("span").unwrap();
        let date_selector = Selector::parse(".notice_col1 div").unwrap();
        let category_title_selector = Selector::parse(".notice_col3 a span").unwrap();
        let url_selector = Selector::parse(".notice_col3 a").unwrap();
        let last_page_selector = Selector::parse(".next-btn-last").unwrap();

        // HTTP 요청 보내기
        let response = reqwest::get(url)
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let last_page_number: u32 = {
            // HTML 파싱
            let document = Html::parse_document(&html);

            let last_page_url = document
                .select(&last_page_selector)
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
        };

        let mut all_posts = Vec::new();

        for page in 1..=last_page_number {
            let page_url = format!("{}/page/{}", url, page);

            let response = reqwest::get(page_url)
                .await
                .map_err(|e| SsufidError::PluginError(e.to_string()))?;

            let html = response
                .text()
                .await
                .map_err(|e| SsufidError::PluginError(e.to_string()))?;

            {
                let document = Html::parse_document(&html);

                let notice_list = document.select(&notice_selector).next().unwrap();

                // 첫 번째 li 요소(헤더)는 건너뛰기 위해 skip(1)을 사용
                notice_list.select(&li_selector).skip(1).for_each(|li| {
                    let date_format = format_description::parse("[year].[month].[day]").unwrap();
                    let date_string = li
                        .select(&date_selector)
                        .next()
                        .unwrap()
                        .text()
                        .collect::<String>();
                    let date = Date::parse(&date_string, &date_format).unwrap();
                    let offset_datetime = date.midnight().assume_offset(offset!(+09:00));

                    let url = li
                        .select(&url_selector)
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

                    let category_title_span = li.select(&category_title_selector).next().unwrap();

                    let spans = category_title_span
                        .select(&span_selector)
                        .map(|span| span.text().collect::<String>())
                        .collect::<Vec<String>>();

                    let category = spans[0].clone();
                    let title = spans[1].clone();

                    all_posts.push(SsufidPost {
                        id,
                        title,
                        category,
                        url,
                        created_at: offset_datetime,
                        updated_at: None,
                        content: "".to_string(),
                    });
                });
            };
        }

        for post in &all_posts {
            println!("{:?}", post);
        }

        Ok(all_posts)
    }
}
