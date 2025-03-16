use crate::core::SsufidPlugin;

pub struct ExamplePlugin;

impl SsufidPlugin for ExamplePlugin {
    const IDENTIFIER: &'static str = "example";

    async fn crawl(&self) -> Result<Vec<crate::core::SsufidPost>, crate::core::SsufidError> {
        println!("example crawler!");
        Ok(vec![])
    }
}
