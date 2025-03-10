#[derive(Debug)]
pub struct CrawledItem {
    pub url: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub published_at: String,
}

#[derive(Debug)]
pub struct CrawlResult {
    pub source: String,
    pub items: Vec<CrawledItem>,
}

pub trait CrawlerPlugin {
    fn name(&self) -> &str;
    fn crawl(&self) -> Result<CrawlResult, String>;
}
