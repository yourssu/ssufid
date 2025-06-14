use crate::common::gnuboard::GnuboardCrawler;
use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub struct SwBachelorPlugin {
    crawler: GnuboardCrawler<Self>,
}

impl SsufidPlugin for SwBachelorPlugin {
    const IDENTIFIER: &'static str = "sw.ssu.ac.kr/bachelor";
    const TITLE: &'static str = "숭실대학교 소프트웨어학부 학사 공지사항";
    const DESCRIPTION: &'static str =
        "숭실대학교 소프트웨어 홈페이지의 학사 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://sw.ssu.ac.kr/bbs/board.php?bo_table=notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for SwBachelorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SwBachelorPlugin {
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
        let plugin = SwBachelorPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
