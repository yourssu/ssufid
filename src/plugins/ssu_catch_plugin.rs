use scraper::{Html, Selector};

use crate::core::{SsufidError, SsufidPlugin, SsufidPost};
use time::{Date, format_description, macros::offset};

pub struct SsuCatchPlugin;

impl SsufidPlugin for SsuCatchPlugin {
    const IDENTIFIER: &'static str = "SSU Catch";
    const TITLE: &'static str = "SSU Catch";
    const DESCRIPTION: &'static str = "SSU Catch plugin";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, SsufidError> {
        let url = "https://scatch.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

        // HTTP 요청 보내기
        let response = reqwest::get(url)
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| SsufidError::PluginError(e.to_string()))?;

        // HTML 파싱
        let document = Html::parse_document(&html);

        let notice_selector = Selector::parse(".notice-lists").unwrap();
        let li_selector = Selector::parse("li").unwrap();
        let span_selector = Selector::parse("span").unwrap();
        let date_selector = Selector::parse(".notice_col1 div").unwrap();
        let category_title_selector = Selector::parse(".notice_col3 a span").unwrap();
        let url_selector = Selector::parse(".notice_col3 a").unwrap();

        let notice_list = document.select(&notice_selector).next().unwrap();
        // 첫 번째 li 요소(헤더)는 건너뛰기 위해 skip(1)을 사용
        let posts = notice_list
            .select(&li_selector)
            .skip(1)
            .map(|li| {
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

                let category_title_span = li.select(&category_title_selector).next().unwrap();

                let spans = category_title_span
                    .select(&span_selector)
                    .map(|span| span.text().collect::<String>())
                    .collect::<Vec<String>>();

                let category = spans[0].clone();
                let title = spans[1].clone();

                SsufidPost {
                    id: "".to_string(),
                    title,
                    category,
                    url,
                    created_at: offset_datetime,
                    updated_at: None,
                    content: "".to_string(),
                }
            })
            .collect();

        println!("{:?}", posts);

        Ok(posts)
    }
}
