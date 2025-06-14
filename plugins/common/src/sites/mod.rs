pub mod cse {
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
}
pub mod docs {
    crate::wordpress_plugin!(
        DocsPlugin,
        "docs.ssu.ac.kr",
        "숭실대학교 기독교학과 공지사항",
        "숭실대학교 기독교학과 홈페이지의 공지사항을 제공합니다.",
        "https://docs.ssu.ac.kr/%ED%95%99%EA%B3%BC%EC%82%AC%EB%AC%B4%EC%8B%A4%EC%95%8C%EB%A6%BC/%EA%B3%B5%EC%A7%80%EC%82%AC%ED%95%AD"
    );
}
pub mod eco {
    crate::gnuboard_plugin!(
        EcoPlugin,
        "eco.ssu.ac.kr",
        "숭실대학교 경제학과 공지사항",
        "숭실대학교 경제학과 홈페이지의 공지사항을 제공합니다.",
        "https://eco.ssu.ac.kr/bbs/board.php?bo_table=notice"
    );
}
pub mod sec {
    crate::gnuboard_plugin!(
        SecPlugin,
        "sec.ssu.ac.kr",
        "숭실대학교 정보보호학과 공지사항",
        "숭실대학교 정보보호학과 홈페이지의 공지사항을 제공합니다.",
        "https://sec.ssu.ac.kr/bbs/board.php?bo_table=notice"
    );
}
pub mod sw {
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
}

pub use cse::CseBachelorPlugin;
pub use cse::CseEmploymentPlugin;
pub use cse::CseGraduatePlugin;
pub use docs::DocsPlugin;
pub use eco::EcoPlugin;
pub use sec::SecPlugin;
pub use sw::SwBachelorPlugin;
pub use sw::SwGraduatePlugin;

crate::test_sites! {
  test_crawl_cse_bachelor(CseBachelorPlugin),
  test_crawl_cse_employment(CseEmploymentPlugin),
  test_crawl_cse_graduate(CseGraduatePlugin),
  test_crawl_docs(DocsPlugin),
  test_crawl_eco(EcoPlugin),
  test_crawl_sec(SecPlugin),
  test_crawl_sw_bachelor(SwBachelorPlugin),
  test_crawl_sw_graduate(SwGraduatePlugin),
}
