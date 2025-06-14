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

macro_rules! test_sites {
    ($($test_name:ident($plugin:ty)),+ $(,)?) => {
        #[cfg(test)]
        mod tests {
            use super::*;
            use ssufid::core::SsufidPlugin;

            $(
                #[tokio::test]
                async fn $test_name() {
                    let posts_limit = 100;
                    let plugin = <$plugin>::new();
                    let posts = plugin.crawl(posts_limit).await.unwrap();
                    assert!(posts.len() <= posts_limit as usize);
                    assert!(!posts.is_empty(), "No posts found for {}", <$plugin>::IDENTIFIER);
                }
            )+
        }
    };
}

pub(crate) use gnuboard_plugin;
pub(crate) use test_sites;
pub(crate) use wordpress_plugin;
