use crate::core::{SsufidError, SsufidPlugin, SsufidPost};

pub struct SsuCatchPlugin;

impl SsufidPlugin for SsuCatchPlugin {
    const IDENTIFIER: &'static str = "SSU Catch";

    async fn crawl(&self) -> Result<Vec<SsufidPost>, SsufidError> {
        println!("example crawler!");
        Ok(vec![])
    }
}
