crate::gnuboard_plugin!(
    CseBachelorPlugin,
    "cse.ssu.ac.kr/bachelor",
    "숭실대학교 컴퓨터학부 학사 공지사항",
    "숭실대학교 컴퓨터학부 홈페이지의 학사 공지사항을 제공합니다.",
    "https://cse.ssu.ac.kr/bbs/board.php?bo_table=notice"
);

crate::gnuboard_plugin!(
    CseEmploymentPlugin,
    "cse.ssu.ac.kr/employment",
    "숭실대학교 컴퓨터학부 취업정보",
    "숭실대학교 컴퓨터학부 홈페이지의 취업정보를 제공합니다.",
    "https://cse.ssu.ac.kr/bbs/board.php?bo_table=employment"
);

crate::gnuboard_plugin!(
    CseGraduatePlugin,
    "cse.ssu.ac.kr/graduate",
    "숭실대학교 컴퓨터학부 대학원 공지사항",
    "숭실대학교 컴퓨터학부 홈페이지의 대학원 공지사항을 제공합니다.",
    "https://cse.ssu.ac.kr/bbs/board.php?bo_table=gra_notice"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_cse_bachelor() {
        let posts_limit = 100;
        let plugin = CseBachelorPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
    }

    #[tokio::test]
    async fn test_crawl_cse_employment() {
        let posts_limit = 100;
        let plugin = CseEmploymentPlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
    }

    #[tokio::test]
    async fn test_crawl_cse_graduate() {
        let posts_limit = 100;
        let plugin = CseGraduatePlugin::new();
        let posts = plugin.crawl(posts_limit).await.unwrap();
        assert_eq!(posts.len(), posts_limit as usize);
    }
}
