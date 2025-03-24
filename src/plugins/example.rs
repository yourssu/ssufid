use crate::core::SsufidPlugin;

pub struct ExamplePlugin;

impl SsufidPlugin for ExamplePlugin {
    const TITLE: &'static str = "Example";
    const IDENTIFIER: &'static str = "example";
    const DESC: &'static str = "Example plugin";

    async fn crawl(&self) -> Result<Vec<crate::core::SsufidPost>, crate::core::SsufidError> {
        println!("example crawler!");
        Ok(vec![])
    }
}
