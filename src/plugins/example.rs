use crate::core::SsufidPlugin;

pub struct ExamplePlugin;

impl SsufidPlugin for ExamplePlugin {
    const TITLE: &'static str = "Example";
    const IDENTIFIER: &'static str = "example";
    const DESCRIPTION: &'static str = "Example plugin";

    async fn crawl(
        &self,
        #[allow(unused_variables)] posts_limit: u32,
    ) -> Result<Vec<crate::core::SsufidPost>, crate::core::SsufidError> {
        println!("example crawler!");
        Ok(vec![])
    }
}
