use crate::common::{CrawlResult, CrawledItem, CrawlerPlugin};

pub struct SSUCatchCrawler;

impl SSUCatchCrawler {
    pub const NAME: &'static str = "SSU Catch";
}

impl CrawlerPlugin for SSUCatchCrawler {
    fn name(&self) -> &str {
        SSUCatchCrawler::NAME
    }

    fn crawl(&self) -> Result<CrawlResult, String> {
        let mut items = Vec::new();

        // 더미 데이터 생성
        items.push(CrawledItem {
            url: "https://scatch.ssu.ac.kr".to_string(),
            title: "제목".to_string(),
            content: "컨텐츠".to_string(),
            category: "학사".to_string(),
            published_at: "2025-03-10".to_string(),
        });

        Ok(CrawlResult {
            source: SSUCatchCrawler::NAME.to_string(),
            items,
        })
    }
}
