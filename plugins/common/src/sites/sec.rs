crate::gnuboard_plugin!(
    SecPlugin,
    "sec.ssu.ac.kr",
    "숭실대학교 정보보호학과 공지사항",
    "숭실대학교 정보보호학과 홈페이지의 공지사항을 제공합니다.",
    "https://sec.ssu.ac.kr/bbs/board.php?bo_table=notice"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_sec() {
        let posts_limit = 100;
        let plugin = SecPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert!(posts.len() <= posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
