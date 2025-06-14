use std::sync::LazyLock;

use scraper::Selector;
use url::Url;

use crate::common::gnuboard::GnuboardMetadataError;

#[derive(Debug)]
pub(crate) struct GnuboardMetadata {
    pub category: Option<String>,
    pub id: String,
    pub url: String,
    pub author: Option<String>,
}

pub(crate) trait GnuboardMetadataResolver {
    fn resolve<'a>(
        element: scraper::ElementRef<'a>,
    ) -> Result<GnuboardMetadata, GnuboardMetadataError>;
}

pub(crate) struct ItGnuboardMetadataResolver;

impl GnuboardMetadataResolver for ItGnuboardMetadataResolver {
    fn resolve<'a>(
        element: scraper::ElementRef<'a>,
    ) -> Result<GnuboardMetadata, GnuboardMetadataError> {
        static CATEGORY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse("td.td_num2 > p").expect("Failed to parse category selector")
        });

        static URL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse("td.td_subject > div > a").expect("Failed to parse URL selector")
        });

        static AUTHOR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse("td.td_name.sv_use > span").expect("Failed to parse author selector")
        });

        let category = element
            .select(&CATEGORY_SELECTOR)
            .next()
            .map(|p| p.text().collect::<String>().trim().to_string());

        let url = element
            .select(&URL_SELECTOR)
            .next()
            .and_then(|a| a.value().attr("href"))
            .ok_or(GnuboardMetadataError::UrlNotFound)?
            .to_string();

        let id = Url::parse(&url)
            .map_err(|_| GnuboardMetadataError::UrlParseError(url.clone()))?
            .query_pairs()
            .find(|(key, value)| key == "wr_id" && !value.is_empty())
            .map(|(_, value)| value.to_string())
            .ok_or(GnuboardMetadataError::IdEmpty(url.clone()))?;

        let author = element
            .select(&AUTHOR_SELECTOR)
            .next()
            .map(|span| span.text().collect::<String>().trim().to_string());

        Ok(GnuboardMetadata {
            category,
            id,
            url,
            author,
        })
    }
}
