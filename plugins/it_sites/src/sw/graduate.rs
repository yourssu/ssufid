use ssufid::{
    core::{SsufidPlugin, SsufidPost},
    PluginError,
};

use crate::common::it_crawler::ItCrawler;

pub struct SwGraduatePlugin {
    crawler: ItCrawler<Self>,
}

impl SsufidPlugin for SwGraduatePlugin {
    const IDENTIFIER: &'static str = "sw.ssu.ac.kr/graduate";
    const TITLE: &'static str = "숭실대학교 소프트웨어 대학원 공지사항";
    const DESCRIPTION: &'static str =
        "숭실대학교 컴퓨터학부 홈페이지의 대학원 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://sw.ssu.ac.kr/bbs/board.php?bo_table=gra_notice";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        self.crawler.crawl(posts_limit).await
    }
}

impl Default for SwGraduatePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SwGraduatePlugin {
    fn new() -> Self {
        Self {
            crawler: ItCrawler::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl() {
        let posts_limit = 100;
        let plugin = SwGraduatePlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
