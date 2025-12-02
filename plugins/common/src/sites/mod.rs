pub mod accounting {
    crate::wordpress_plugin!(
        AccountingPlugin,
        "accounting.ssu.ac.kr",
        "숭실대학교 회계학과 공지사항",
        "숭실대학교 회계학과 홈페이지의 공지사항을 제공합니다.",
        "https://accounting.ssu.ac.kr/%ea%b2%8c%ec%8b%9c%ed%8c%90/%ed%96%89%ec%a0%95%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use accounting::AccountingPlugin;

pub mod actx {
    crate::wordpress_plugin!(
        ActxPlugin,
        "actx.ssu.ac.kr",
        "숭실대학교 회계세무학과 공지사항",
        "숭실대학교 회계세무학과 홈페이지의 공지사항을 제공합니다.",
        "https://actx.ssu.ac.kr/%ed%95%99%ec%82%ac%ec%95%88%eb%82%b4/m2sub1"
    );
}
pub use actx::ActxPlugin;

pub mod bioinfo;
pub use bioinfo::BioinfoPlugin;

pub mod chem {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        ChemPlugin,
        "chem.ssu.ac.kr",
        "숭실대학교 화학과 공지사항",
        "숭실대학교 화학과 홈페이지의 공지사항을 제공합니다.",
        "https://chem.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use chem::ChemPlugin;

pub mod chilan {
    crate::wordpress_plugin!(
        ChilanPlugin,
        "chilan.ssu.ac.kr",
        "숭실대학교 중어중문학과 공지사항",
        "숭실대학교 중어중문학과 홈페이지의 공지사항을 제공합니다.",
        "https://chilan.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use chilan::ChilanPlugin;

pub mod counsel {
    crate::wordpress_plugin!(
        CounselPlugin,
        "counsel.ssu.ac.kr",
        "숭실대학교 상담심리학과 공지사항",
        "숭실대학교 상담심리학과 홈페이지의 공지사항을 제공합니다.",
        "https://counsel.ssu.ac.kr/%ec%95%8c%eb%a6%bc%eb%a7%88%eb%8b%b9/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use counsel::CounselPlugin;

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
pub use cse::CseBachelorPlugin;
pub use cse::CseEmploymentPlugin;
pub use cse::CseGraduatePlugin;

pub mod docs {
    crate::wordpress_plugin!(
        DocsPlugin,
        "docs.ssu.ac.kr",
        "숭실대학교 기독교학과 공지사항",
        "숭실대학교 기독교학과 홈페이지의 공지사항을 제공합니다.",
        "https://docs.ssu.ac.kr/%ED%95%99%EA%B3%BC%EC%82%AC%EB%AC%B4%EC%8B%A4%EC%95%8C%EB%A6%BC/%EA%B3%B5%EC%A7%80%EC%82%AC%ED%95%AD"
    );
}
pub use docs::DocsPlugin;

pub mod eco;
pub use eco::EcoPlugin;

pub mod englan {
    crate::wordpress_plugin!(
        EnglanPlugin,
        "englan.ssu.ac.kr",
        "숭실대학교 영어영문학과 공지사항",
        "숭실대학교 영어영문학과 홈페이지의 공지사항을 제공합니다.",
        "https://englan.ssu.ac.kr/%eb%8c%80%ed%95%99%ec%9b%90/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use englan::EnglanPlugin;

pub mod ensb {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        EnsbPlugin,
        "ensb.ssu.ac.kr",
        "숭실대학교 벤처중소기업학과 공지사항",
        "숭실대학교 벤처중소기업학과 홈페이지의 공지사항을 제공합니다.",
        "https://ensb.ssu.ac.kr/%ed%95%99%eb%b6%80%ec%95%8c%eb%a6%bc/%ed%95%99%eb%b6%80%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use ensb::EnsbPlugin;

pub mod finance {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        FinancePlugin,
        "finance.ssu.ac.kr",
        "숭실대학교 금융학부 공지사항",
        "숭실대학교 금융학부 홈페이지의 공지사항을 제공합니다.",
        "https://finance.ssu.ac.kr/%ea%b2%8c%ec%8b%9c%ed%8c%90/%ed%96%89%ec%a0%95%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use finance::FinancePlugin;

pub mod france {
    crate::wordpress_plugin!(
        FrancePlugin,
        "france.ssu.ac.kr",
        "숭실대학교 불어불문학과 공지사항",
        "숭실대학교 불어불문학과 홈페이지의 공지사항을 제공합니다.",
        "https://france.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use france::FrancePlugin;

pub mod gerlan {
    crate::wordpress_plugin!(
        GerlanPlugin,
        "gerlan.ssu.ac.kr",
        "숭실대학교 독어독문학과 공지사항",
        "숭실대학교 독어독문학과 홈페이지의 공지사항을 제공합니다.",
        "https://gerlan.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use gerlan::GerlanPlugin;

pub mod gtrade {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        GtradePlugin,
        "gtrade.ssu.ac.kr",
        "숭실대학교 글로벌통상학과 공지사항",
        "숭실대학교 글로벌통상학과 홈페이지의 공지사항을 제공합니다.",
        "https://gtrade.ssu.ac.kr/%ed%95%99%eb%b6%80%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use gtrade::GtradePlugin;

pub mod history {
    crate::wordpress_plugin!(
        HistoryPlugin,
        "history.ssu.ac.kr",
        "숭실대학교 사학과 공지사항",
        "숭실대학교 사학과 홈페이지의 공지사항을 제공합니다.",
        "https://history.ssu.ac.kr/%ED%95%99%EA%B3%BC%EC%86%8C%EC%8B%9D/%ED%95%99%EA%B3%BC%EA%B3%B5%EC%A7%80"
    );
}
pub use history::HistoryPlugin;

pub mod iise {
    use crate::common::wordpress::{
        KorDateWordpressPostResolver, metadata::KorDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        IisePlugin,
        "iise.ssu.ac.kr",
        "숭실대학교 산업정보시스템공학과 공지사항",
        "숭실대학교 산업정보시스템공학과 홈페이지의 공지사항을 제공합니다.",
        "https://iise.ssu.ac.kr/cummunity/notice/",
        KorDateWordpressMetadataResolver,
        KorDateWordpressPostResolver
    );
}
pub use iise::IisePlugin;

pub mod itrans {
    use crate::common::wordpress::{
        KorDateWordpressPostResolver, metadata::KorDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        ItransPlugin,
        "itrans.ssu.ac.kr",
        "숭실대학교 국제무역학과 공지사항",
        "숭실대학교 국제무역학과 홈페이지의 공지사항을 제공합니다.",
        "https://itrans.ssu.ac.kr/community/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        KorDateWordpressMetadataResolver,
        KorDateWordpressPostResolver
    );
}
pub use itrans::ItransPlugin;

pub mod japanstu {
    crate::wordpress_plugin!(
        JapanstuPlugin,
        "japanstu.ssu.ac.kr",
        "숭실대학교 일어일문학과 공지사항",
        "숭실대학교 일어일문학과 홈페이지의 공지사항을 제공합니다.",
        "https://japanstu.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use japanstu::JapanstuPlugin;

pub mod korlan {
    crate::wordpress_plugin!(
        KorlanPlugin,
        "korlan.ssu.ac.kr",
        "숭실대학교 국어국문학과 공지사항",
        "숭실대학교 국어국문학과 홈페이지의 공지사항을 제공합니다.",
        "https://korlan.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use korlan::KorlanPlugin;

pub mod law {
    crate::wordpress_plugin!(
        LawPlugin,
        "law.ssu.ac.kr",
        "숭실대학교 법학과 공지사항",
        "숭실대학교 법학과 홈페이지의 공지사항을 제공합니다.",
        "https://law.ssu.ac.kr/menu5/m5sub3"
    );
}
pub use law::LawPlugin;

pub mod lawyer {
    use crate::common::wordpress::{
        KorDateWordpressPostResolver, metadata::KorDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        LawyerPlugin,
        "lawyer.ssu.ac.kr",
        "숭실대학교 국제법무학과 공지사항",
        "숭실대학교 국제법무학과 홈페이지의 공지사항을 제공합니다.",
        "https://lawyer.ssu.ac.kr/%ED%95%99%EA%B3%BC-%EC%86%8C%EC%8B%9D/%ED%95%99%EA%B3%BC-%EA%B3%B5%EC%A7%80/",
        KorDateWordpressMetadataResolver,
        KorDateWordpressPostResolver
    );
}

pub use lawyer::LawyerPlugin;

pub mod lifelongedu {
    crate::wordpress_plugin!(
        LifelongEduPlugin,
        "lifelongedu.ssu.ac.kr",
        "숭실대학교 평생교육학과 공지사항",
        "숭실대학교 평생교육학과 홈페이지의 공지사항을 제공합니다.",
        "https://lifelongedu.ssu.ac.kr/%ED%95%99%EA%B3%BC%EC%82%AC%EB%AC%B4%EC%8B%A4%EC%95%8C%EB%A6%BC/%EA%B3%B5%EC%A7%80%EC%82%AC%ED%95%AD"
    );
}

pub use lifelongedu::LifelongEduPlugin;

pub mod masscom {
    use crate::common::wordpress::{
        KorDateWordpressPostResolver, metadata::KorDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        MasscomPlugin,
        "masscom.ssu.ac.kr",
        "숭실대학교 언론홍보학과 공지사항",
        "숭실대학교 언론홍보학과 홈페이지의 공지사항을 제공합니다.",
        "https://masscom.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad/%ed%95%99%eb%b6%80%ea%b3%b5%ec%a7%80",
        KorDateWordpressMetadataResolver,
        KorDateWordpressPostResolver
    );
}
pub use masscom::MasscomPlugin;

pub mod math {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        MathPlugin,
        "math.ssu.ac.kr",
        "숭실대학교 수학과 공지사항",
        "숭실대학교 수학과 홈페이지의 공지사항을 제공합니다.",
        "https://math.ssu.ac.kr/?page_id=977",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use math::MathPlugin;

pub mod mgmt;
pub use mgmt::MgmtPlugin;

pub mod mysoongsil {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        MysoongsilPlugin,
        "mysoongsil.ssu.ac.kr",
        "숭실대학교 사회복지학부 공지사항",
        "숭실대학교 사회복지학부 홈페이지의 공지사항을 제공합니다.",
        "https://mysoongsil.ssu.ac.kr/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad-2/%ed%95%99%eb%b6%80-%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use mysoongsil::MysoongsilPlugin;

pub mod philo {
    crate::wordpress_plugin!(
        PhiloPlugin,
        "philo.ssu.ac.kr",
        "숭실대학교 철학과 공지사항",
        "숭실대학교 철학과 홈페이지의 공지사항을 제공합니다.",
        "https://philo.ssu.ac.kr/%ed%95%99%ea%b3%bc%ec%82%ac%eb%ac%b4%ec%8b%a4%ec%95%8c%eb%a6%bc/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use philo::PhiloPlugin;

pub mod physics {
    crate::wordpress_plugin!(
        PhysicsPlugin,
        "physics.ssu.ac.kr",
        "숭실대학교 물리학과 공지사항",
        "숭실대학교 물리학과 홈페이지의 공지사항을 제공합니다.",
        "https://physics.ssu.ac.kr/%ea%b2%8c%ec%8b%9c%ed%8c%90/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use physics::PhysicsPlugin;

pub mod politics {
    crate::wordpress_plugin!(
        PoliticsPlugin,
        "politics.ssu.ac.kr",
        "숭실대학교 정치외교학과 공지사항",
        "숭실대학교 정치외교학과 홈페이지의 공지사항을 제공합니다.",
        "https://politics.ssu.ac.kr/%ed%95%99%ea%b3%bc%ea%b3%b5%ec%a7%80/%ed%95%99%eb%b6%80-%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad"
    );
}
pub use politics::PoliticsPlugin;

pub mod pubad {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        PubadPlugin,
        "pubad.ssu.ac.kr",
        "숭실대학교 행정학부 공지사항",
        "숭실대학교 행정학부 홈페이지의 공지사항을 제공합니다.",
        "https://pubad.ssu.ac.kr/%ec%a0%95%eb%b3%b4%ea%b4%91%ec%9e%a5/%ed%95%99%eb%b6%80-%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use pubad::PubadPlugin;

pub mod sec {
    crate::gnuboard_plugin!(
        SecPlugin,
        "sec.ssu.ac.kr",
        "숭실대학교 정보보호학과 공지사항",
        "숭실대학교 정보보호학과 홈페이지의 공지사항을 제공합니다.",
        "https://sec.ssu.ac.kr/bbs/board.php?bo_table=notice"
    );
}
pub use sec::SecPlugin;

pub mod sls {
    use crate::common::wordpress::{
        KorDateWordpressPostResolver, metadata::KorDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        SlsPlugin,
        "sls.ssu.ac.kr",
        "숭실대학교 자유전공학부 공지사항",
        "숭실대학교 자유전공학부 홈페이지의 공지사항을 제공합니다.",
        "https://sls.ssu.ac.kr/%ed%95%99%eb%b6%80%ec%86%8c%ec%8b%9d/%ea%b3%b5%ec%a7%80%ec%82%ac%ed%95%ad",
        KorDateWordpressMetadataResolver,
        KorDateWordpressPostResolver
    );
}
pub use sls::SlsPlugin;

pub mod soar;
pub use soar::SoarPlugin;

pub mod sports {
    use crate::common::wordpress::{
        DotDateWordpressPostResolver, metadata::DotDateWordpressMetadataResolver,
    };

    crate::wordpress_plugin!(
        SportsPlugin,
        "sports.ssu.ac.kr",
        "숭실대학교 스포츠학부 공지사항",
        "숭실대학교 스포츠학부 홈페이지의 공지사항을 제공합니다.",
        "http://sports.ssu.ac.kr/%ec%9e%90%eb%a3%8c%ec%8b%a4/%ec%8b%a0%ec%95%99%ec%9e%90%eb%a3%8c%ec%8b%a4",
        DotDateWordpressMetadataResolver,
        DotDateWordpressPostResolver
    );
}
pub use sports::SportsPlugin;

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
pub use sw::SwBachelorPlugin;
pub use sw::SwGraduatePlugin;

crate::test_sites! {
    test_crawl_accounting(AccountingPlugin),
    test_crawl_actx(ActxPlugin),
    test_crawl_bioinfo(BioinfoPlugin),
    test_crawl_chem(ChemPlugin),
    test_crawl_chilan(ChilanPlugin),
    test_crawl_counsel(CounselPlugin),
    test_crawl_cse_bachelor(CseBachelorPlugin),
    test_crawl_cse_employment(CseEmploymentPlugin),
    test_crawl_cse_graduate(CseGraduatePlugin),
    test_crawl_docs(DocsPlugin),
    test_crawl_eco(EcoPlugin),
    test_crawl_englan(EnglanPlugin),
    test_crawl_ensb(EnsbPlugin),
    test_crawl_finance(FinancePlugin),
    test_crawl_france(FrancePlugin),
    test_crawl_gerlan(GerlanPlugin),
    test_crawl_gtrade(GtradePlugin),
    test_crawl_history(HistoryPlugin),
    test_crawl_iise(IisePlugin),
    test_crawl_itrans(ItransPlugin),
    test_crawl_japanstu(JapanstuPlugin),
    test_crawl_korlan(KorlanPlugin),
    test_crawl_law(LawPlugin),
    test_crawl_lifelongedu(LifelongEduPlugin),
    test_crawl_masscom(MasscomPlugin),
    test_crawl_math(MathPlugin),
    test_crawl_mgmt(MgmtPlugin),
    test_crawl_mysoongsil(MysoongsilPlugin),
    test_crawl_philo(PhiloPlugin),
    test_crawl_physics(PhysicsPlugin),
    test_crawl_politics(PoliticsPlugin),
    test_crawl_pubad(PubadPlugin),
    test_crawl_sec(SecPlugin),
    test_crawl_sls(SlsPlugin),
    test_crawl_soar(SoarPlugin),
    test_crawl_sports(SportsPlugin),
    test_crawl_sw_bachelor(SwBachelorPlugin),
    test_crawl_sw_graduate(SwGraduatePlugin),
}
