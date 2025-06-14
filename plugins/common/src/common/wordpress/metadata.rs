use ssufid::{PluginError, core::SsufidPlugin};
use time::{
    Date,
    macros::{format_description, offset},
};

#[allow(dead_code)]
pub(crate) struct WordpressMetadata<T: SsufidPlugin> {
    pub is_announcement: bool,
    pub title: String,
    pub url: String,
    pub created_at: time::OffsetDateTime,
    pub _marker: std::marker::PhantomData<T>,
}

pub(crate) trait WordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>];

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
            .skip(1)
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date element".into()))?;

        let is_announcement = number_element
            .text()
            .next()
            .map_or(false, |text| text.contains("공지"));

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

pub(crate) struct DefaultWordpressMetadataResolver;

impl WordpressMetadataResolver for DefaultWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]-[month]-[day]");
}

pub(crate) struct DotDateWordpressMetadataResolver;
impl WordpressMetadataResolver for DotDateWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year].[month].[day]");
}

pub(crate) struct KorDateWordpressMetadataResolver;
impl WordpressMetadataResolver for KorDateWordpressMetadataResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]년 [month padding:none]월 [day padding:none]일");
}
