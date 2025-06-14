use std::sync::LazyLock;

use scraper::Selector;
use ssufid::{PluginError, core::SsufidPlugin};
use time::{
    Date,
    macros::{format_description, offset},
};

use crate::common::wordpress::{
    WordpressCrawler,
    metadata::{WordpressMetadata, WordpressMetadataResolver},
};

pub struct MgmtPlugin {
    crawler: WordpressCrawler<Self, MgmtWordpressMetadataResolver>,
}

impl ssufid::core::SsufidPlugin for MgmtPlugin {
    const IDENTIFIER: &'static str = "mgmt.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 벤처경영학과 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 벤처경영학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://mgmt.ssu.ac.kr/%ed%95%99%ec%82%ac%ec%95%88%eb%82%b4/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad";

    async fn crawl(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for MgmtPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl MgmtPlugin {
    pub fn new() -> Self {
        Self {
            crawler: WordpressCrawler::card(),
        }
    }
}

struct MgmtWordpressMetadataResolver;

impl WordpressMetadataResolver for MgmtWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]-[month]-[day]");

    fn resolve<T: SsufidPlugin>(
        element: scraper::ElementRef<'_>,
    ) -> Result<WordpressMetadata<T>, PluginError> {
        static TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse(".board_tit").expect("Failed to parse title selector")
        });

        static DATE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse(".etc_info .date .date_val").expect("Failed to parse date selector")
        });

        let title_element = element
            .select(&TITLE_SELECTOR)
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find title element".into()))?;

        let date_element = element
            .select(&DATE_SELECTOR)
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date element".into()))?;

        let is_announcement = title_element
            .attr("class")
            .is_some_and(|class| class.contains("notice"));

        let title = title_element.text().collect::<String>();

        let url = element
            .attr("href")
            .ok_or_else(|| {
                PluginError::parse::<T>("Failed to find URL in the title element".into())
            })?
            .to_string();

        let date_text = date_element
            .text()
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date text".into()))?
            .trim();
        let created_at = Date::parse(date_text, Self::DATE_FORMAT)
            .map_err(|e| PluginError::parse::<T>(format!("Failed to parse date: {e:?}")))?
            .midnight()
            .assume_offset(offset!(+09:00));

        Ok(WordpressMetadata {
            is_announcement,
            title,
            url,
            created_at,
            _marker: std::marker::PhantomData,
        })
    }
}
