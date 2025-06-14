use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

use crate::common::gnuboard::GnuboardCrawler;

pub struct SecPlugin {
    crawler: GnuboardCrawler<Self>,
}

impl SsufidPlugin for SecPlugin {
    const IDENTIFIER: &'static str = "sec.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 정보보호학과 공지사항";
    const DESCRIPTION: &'static str = "숭실대학교 정보보호학과 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://sec.ssu.ac.kr/bbs/board.php?bo_table=notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for SecPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SecPlugin {
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
        let plugin = SecPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert!(posts.len() <= posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
