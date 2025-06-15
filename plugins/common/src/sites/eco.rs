use std::sync::LazyLock;

use scraper::Selector;
use url::Url;

use crate::common::gnuboard::{
    GnuboardMetadataError,
    metadata::{GnuboardMetadata, GnuboardMetadataResolver},
};

crate::gnuboard_plugin!(
    EcoPlugin,
    "eco.ssu.ac.kr",
    "숭실대학교 경제학과 공지사항",
    "숭실대학교 경제학과 홈페이지의 공지사항을 제공합니다.",
    "https://eco.ssu.ac.kr/bbs/board.php?bo_table=notice",
    EcoGnuboardMetadataResolver
);

struct EcoGnuboardMetadataResolver;

impl GnuboardMetadataResolver for EcoGnuboardMetadataResolver {
    fn resolve<'a>(
        element: scraper::ElementRef<'a>,
    ) -> Result<GnuboardMetadata, GnuboardMetadataError> {
        static CATEGORY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse(".bo_cate_link").expect("Failed to parse category selector")
        });

        static URL_SELECTOR: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse(".bo_tit > a").expect("Failed to parse URL selector"));

        static AUTHOR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse(".sv_member").expect("Failed to parse author selector")
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
