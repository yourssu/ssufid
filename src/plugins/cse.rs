use scraper::{Html, Selector};
use time::{
    Date, OffsetDateTime, format_description,
    macros::{format_description, offset},
    parsing::Parsed,
};
use url::Url;

use crate::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub struct CsePlugin {
    selectors: Selectors,
}

// impl Default for CsePlugin {
//     fn default() -> Self {
//         CsePlugin::new()
//     }
// }

impl SsufidPlugin for CsePlugin {
    const IDENTIFIER: &'static str = "cse.ssu.ac.kr/bachelor";
    const TITLE: &'static str = "숭실대학교 컴퓨터학부 학사 공지사항";
    const DESCRIPTION: &'static str =
        "숭실대학교 컴퓨터학부 홈페이지의 학사 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://cse.ssu.ac.kr/bbs/board.php?bo_table=notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        Ok(vec![])
    }
}

impl CsePlugin {
    fn new() -> Self {
        Self {
            selectors: Selectors::new(),
        }
    }

    async fn fetch_page_posts_metadata(&self, page: u32) -> Result<Vec<CseMetadata>, PluginError> {
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
            .ok_or_else(|| PluginError::parse::<Self>("TODO".to_string()))?
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

                Some(CseMetadata { id, url })
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
            .map(|span| span.text().collect::<String>())
            .unwrap_or_else(|| "No Title".to_string())
            .trim()
            .to_string();

        let created_at = {
            let date_string = document
                .select(&self.selectors.created_at)
                .next()
                .map(|element| element.text().collect::<String>())
                .unwrap_or_default()
                .trim()
                .to_string();

            let date_format = format_description!(
                "작성일 [year repr:last_two]-[month]-[day] [hour repr:24]:[minute]"
            );
            let mut parsed = Parsed::new();
            parsed
                .parse_items(date_string.as_bytes(), date_format)
                .unwrap();
            let year = parsed.year_last_two().unwrap() as i32 + 2000;
            parsed.set_year(year).unwrap();

            OffsetDateTime::try_from(parsed).unwrap()
        };

        let content = document
            .select(&self.selectors.content)
            .next()
            .unwrap()
            .child_elements()
            .map(|p| p.text().collect::<String>())
            .collect::<Vec<String>>()
            .join("\n");

        Ok(SsufidPost {
            id: metadata.id.clone(),
            title,
            category: "공지".to_string(), // TODO?
            url: metadata.url.clone(),
            created_at,
            updated_at: None,
            content,
        })
    }
}

#[derive(Debug)]
struct CseMetadata {
    id: String,
    url: String,
}

struct Selectors {
    table: Selector,
    tr: Selector,
    url: Selector,
    title: Selector,
    created_at: Selector,
    content: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            table: Selector::parse("#bo_list > div.notice_list > table > tbody").unwrap(),
            tr: Selector::parse("tr").unwrap(),
            url: Selector::parse("td.td_subject > div > a").unwrap(),
            title: Selector::parse("#bo_v_title > span").unwrap(),
            created_at: Selector::parse(
                "#bo_v_info > div.profile_info > div.profile_info_ct > strong.if_date",
            )
            .unwrap(),
            content: Selector::parse("#bo_v_con").unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use scraper::{Html, Selector};
    use time::{
        Date,
        macros::{format_description, offset},
        parsing::Parsed,
    };
    use url::Url;

    use crate::{core::SsufidPlugin, plugins::cse::CsePlugin};

    #[tokio::test]
    async fn test_fetch_page() {
        let plugin = CsePlugin::new();
        let metadata_list = plugin.fetch_page_posts_metadata(1).await.unwrap();
        for metadata in metadata_list {
            let post = plugin.fetch_post(&metadata).await.unwrap();
            println!("{:?}", post);
        }
    }

    #[tokio::test]
    async fn test_cse() {
        let page_url = format!("{}/&page={}", CsePlugin::BASE_URL, 1);

        let response = reqwest::get(page_url).await.unwrap();
        let html = response.text().await.unwrap();
        let document = Html::parse_document(&html);

        let table_selector = Selector::parse("#bo_list > div.notice_list > table > tody").unwrap();
        let tr_selector = Selector::parse("tr").unwrap();
        let table = document.select(&table_selector).next().unwrap();

        for a in table.select(&tr_selector) {
            println!("{:?}", a);
        }
    }

    #[tokio::test]
    async fn test_content() {
        let html = reqwest::get("https://cse.ssu.ac.kr/bbs/board.php?bo_table=notice&wr_id=4796")
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let document = Html::parse_document(&html);
        let created_at_selector =
            Selector::parse("#bo_v_info > div.profile_info > div.profile_info_ct > strong.if_date")
                .unwrap();

        let created_at = {
            let date_string = document
                .select(&created_at_selector)
                .next()
                .map(|element| element.text().collect::<String>())
                .unwrap_or_default()
                .trim()
                .to_string();
            println!("date string: {}", date_string);
            let date_format = format_description!("작성일 [year]-[month]-[day] [hour]:[minute]");
            Date::parse(&date_string, &date_format)
                .unwrap()
                .midnight()
                .assume_offset(offset!(+09:00))
        };
        println!("{:?}", created_at)
    }

    #[test]
    fn test_format() {
        let date_format = format_description!("[year repr:last_two]-[month]-[day]");
        let date = "03-05-26";

        let mut parsed = Parsed::new();
        parsed.parse_items(date.as_bytes(), date_format).unwrap();
        let year = parsed.year_last_two().unwrap() as i32 + 2000;
        parsed.set_year(year).unwrap();
        let res = Date::try_from(parsed).unwrap();
        println!("{:?}", res);
    }
}
