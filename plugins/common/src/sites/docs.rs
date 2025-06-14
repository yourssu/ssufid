use crate::wordpress_plugin;

wordpress_plugin!(
    DocsPlugin,
    "docs.ssu.ac.kr",
    "숭실대학교 기독교학과 공지사항",
    "숭실대학교 기독교학과 홈페이지의 공지사항을 제공합니다.",
    "https://docs.ssu.ac.kr/%ED%95%99%EA%B3%BC%EC%82%AC%EB%AC%B4%EC%8B%A4%EC%95%8C%EB%A6%BC/%EA%B3%B5%EC%A7%80%EC%82%AC%ED%95%AD"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_docs() {
        let posts_limit = 100;
        let plugin = DocsPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert!(posts.len() <= posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
