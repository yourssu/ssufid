crate::gnuboard_plugin!(
    SwBachelorPlugin,
    "sw.ssu.ac.kr/bachelor",
    "숭실대학교 소프트웨어학부 학사 공지사항",
    "숭실대학교 소프트웨어 홈페이지의 학사 공지사항을 제공합니다.",
    "https://sw.ssu.ac.kr/bbs/board.php?bo_table=notice"
);

crate::gnuboard_plugin!(
    SwGraduatePlugin,
    "sw.ssu.ac.kr/graduate",
    "숭실대학교 소프트웨어 대학원 공지사항",
    "숭실대학교 컴퓨터학부 홈페이지의 대학원 공지사항을 제공합니다.",
    "https://sw.ssu.ac.kr/bbs/board.php?bo_table=gra_notice"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_sw_bachelor() {
        let posts_limit = 100;
        let plugin = SwBachelorPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }

    #[tokio::test]
    async fn test_crawl_sw_graduate() {
        let posts_limit = 100;
        let plugin = SwGraduatePlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
        // println!("{:#?}", posts);
    }
}
