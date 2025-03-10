use std::collections::HashMap;

use crate::common::{CrawlResult, CrawlerPlugin};

pub struct Core {
    plugins: HashMap<String, Box<dyn CrawlerPlugin>>,
}

impl Core {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn CrawlerPlugin>) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, plugin);
    }

    pub fn run_crawler(&self, plugin_name: &str) -> Result<CrawlResult, String> {
        match self.plugins.get(plugin_name) {
            Some(plugin) => plugin.crawl(),
            None => Err(format!("Plugin '{}' not found", plugin_name)),
        }
    }
}
