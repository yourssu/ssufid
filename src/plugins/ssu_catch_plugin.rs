use crate::core::{SsufidError, SsufidPlugin, SsufidPost};

pub struct SsuCatchPlugin;

impl SsufidPlugin for SsuCatchPlugin {
    const IDENTIFIER: &'static str = "SSU Catch";
    const TITLE: &'static str = "SSU Catch";
    const DESCRIPTION: &'static str = "SSU Catch plugin";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, SsufidError> {
        println!("example crawler!");
        Ok(vec![])
    }
}
