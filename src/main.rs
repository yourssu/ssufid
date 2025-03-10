use core::Core;

use plugins::ssu_catch_plugin::SSUCatchCrawler;

mod common;
mod core;
mod plugins;

fn main() {
    // Core 인스턴스 생성
    let mut core = Core::new();

    // Plugin 생성 및 등록
    let ssu_catch_crawler = SSUCatchCrawler;
    core.register_plugin(Box::new(ssu_catch_crawler));

    // 크롤링 실행
    if let Ok(result) = core.run_crawler(SSUCatchCrawler::NAME) {
        println!("{:?}", result);
    } else {
        println!("Failed to run crawler");
    }
}
