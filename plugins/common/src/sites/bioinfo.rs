use ssufid::{PluginError, core::SsufidPlugin};
use time::{
    Date,
    macros::{format_description, offset},
};

use crate::common::wordpress::{
    DotDateWordpressPostResolver,
    metadata::{WordpressMetadata, WordpressMetadataResolver},
};

crate::wordpress_plugin!(
    BioinfoPlugin,
    "bioinfo.ssu.ac.kr",
    "숭실대학교 의생명시스템학부 공지사항",
    "숭실대학교 의생명시스템학부 홈페이지의 공지사항을 제공합니다.",
    "https://bioinfo.ssu.ac.kr/%ea%b2%8c%ec%8b%9c%ed%8c%90/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
    BioinfoWordpressMetadataResolver,
    DotDateWordpressPostResolver
);

struct BioinfoWordpressMetadataResolver;

impl WordpressMetadataResolver for BioinfoWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year].[month].[day]");

    fn resolve<T: SsufidPlugin>(
        element: scraper::ElementRef<'_>,
    ) -> Result<WordpressMetadata<T>, PluginError> {
        let mut childrens = element.child_elements().peekable();

        let number_element = childrens.next().ok_or_else(|| {
            PluginError::parse::<T>("Failed to find number element in the board item".into())
        })?;

        if childrens.peek().is_none() {
            return Err(PluginError::custom::<T>(
                "NO_ENTRY".into(),
                "No entry found in this page. please handle this error in your plugin".into(),
            ));
        }

        let title_element = childrens
            .next()
            .and_then(|el| el.child_elements().next())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find title element".into()))?;

        let date_element = childrens
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date element".into()))?;

        let is_announcement = number_element
            .text()
            .next()
            .is_some_and(|text| text.contains("공지"));

        let title = title_element.text().collect::<String>();

        let url = title_element
            .value()
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
