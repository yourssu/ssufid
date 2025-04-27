use crate::plugins::common::it_crawler::ITCrawler;
use crate::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub struct CseBachelorPlugin {
    crawler: ITCrawler<Self>,
}

impl SsufidPlugin for CseBachelorPlugin {
    const IDENTIFIER: &'static str = "cse.ssu.ac.kr/bachelor";
    const TITLE: &'static str = "숭실대학교 컴퓨터학부 학사 공지사항";
    const DESCRIPTION: &'static str =
        "숭실대학교 컴퓨터학부 홈페이지의 학사 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://cse.ssu.ac.kr/bbs/board.php?bo_table=notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for CseBachelorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl CseBachelorPlugin {
    fn new() -> Self {
        Self {
            crawler: ITCrawler::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl() {
        let posts_limit = 100;
        let plugin = CseBachelorPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
