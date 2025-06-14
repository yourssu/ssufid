mod common;
pub mod sites;

macro_rules! gnuboard_plugin {
    ($name:ident, $identifier:expr, $title:expr, $description:expr, $base_url:expr) => {
        pub struct $name {
            crawler: $crate::common::gnuboard::GnuboardCrawler<Self>,
        }

        impl ssufid::core::SsufidPlugin for $name {
            const IDENTIFIER: &'static str = $identifier;
            const TITLE: &'static str = $title;
            const DESCRIPTION: &'static str = $description;
            const BASE_URL: &'static str = $base_url;

            async fn crawl(
                &self,
                posts_limit: u32,
            ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
                self.crawler.crawl(posts_limit).await
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    crawler: $crate::common::gnuboard::GnuboardCrawler::new(),
                }
            }
        }
    };
}

macro_rules! wordpress_plugin {
    ($name:ident, $identifier:expr, $title:expr, $description:expr, $base_url:expr) => {
        pub struct $name {
            crawler: $crate::common::wordpress::WordpressCrawler<Self>,
        }

        impl ssufid::core::SsufidPlugin for $name {
            const IDENTIFIER: &'static str = $identifier;
            const TITLE: &'static str = $title;
            const DESCRIPTION: &'static str = $description;
            const BASE_URL: &'static str = $base_url;

            async fn crawl(
                &self,
                posts_limit: u32,
            ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
                self.crawler.crawl(posts_limit).await
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    crawler: $crate::common::wordpress::WordpressCrawler::new(),
                }
            }
        }
    };
}

pub(crate) use gnuboard_plugin;
pub(crate) use wordpress_plugin;
