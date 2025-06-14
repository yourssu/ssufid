use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

use crate::common::gnuboard::GnuboardCrawler;

pub struct CseEmploymentPlugin {
    crawler: GnuboardCrawler<Self>,
}

impl SsufidPlugin for CseEmploymentPlugin {
    const IDENTIFIER: &'static str = "cse.ssu.ac.kr/employment";
    const TITLE: &'static str = "숭실대학교 컴퓨터학부 취업정보";
    const DESCRIPTION: &'static str = "숭실대학교 컴퓨터학부 홈페이지의 취업정보를 제공합니다.";
    const BASE_URL: &'static str = "https://cse.ssu.ac.kr/bbs/board.php?bo_table=employment";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for CseEmploymentPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl CseEmploymentPlugin {
    fn new() -> Self {
        Self {
            crawler: GnuboardCrawler::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl() {
        let posts_limit = 100;
        let plugin = CseEmploymentPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
