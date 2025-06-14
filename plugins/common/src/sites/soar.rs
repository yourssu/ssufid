use ssufid::{PluginError, core::SsufidPlugin};
use time::{
    Date,
    macros::{format_description, offset},
};

use crate::common::wordpress::{
    KorDateWordpressPostResolver,
    metadata::{WordpressMetadata, WordpressMetadataResolver},
};

crate::wordpress_plugin!(
    SoarPlugin,
    "soar.ssu.ac.kr",
    "숭실대학교 건축학부 공지사항",
    "숭실대학교 건축학부 홈페이지의 공지사항을 제공합니다.",
    "https://soar.ssu.ac.kr/news/notice",
    SoarWordpressMetadataResolver,
    KorDateWordpressPostResolver
);

struct SoarWordpressMetadataResolver;

impl WordpressMetadataResolver for SoarWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]년 [month padding:none]월 [day padding:none]일");

    fn resolve<T: SsufidPlugin>(
        element: scraper::ElementRef<'_>,
    ) -> Result<WordpressMetadata<T>, PluginError> {
        if element.child_elements().count() == 1 {
            return Err(PluginError::custom::<T>(
                "NO_ENTRY".into(),
                "No entry found in this page. please handle this error in your plugin".into(),
            ));
        }
        let mut childrens = element.child_elements().peekable();

        let title_element = childrens
            .next()
            .and_then(|el| el.child_elements().next())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find title element".into()))?;

        let date_element = childrens
            .skip(1)
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date element".into()))?;

        let is_announcement = title_element
            .attr("class")
            .map_or(false, |class| class.contains("notice_color"));

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
        dbg!(&date_text);
        let created_at = Date::parse(&date_text, Self::DATE_FORMAT)
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
