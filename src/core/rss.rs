use rss::{ChannelBuilder, ItemBuilder};
use time::format_description::well_known::Rfc2822;

use super::{SsufidPost, SsufidSiteData};

impl From<SsufidPost> for rss::Item {
    fn from(value: SsufidPost) -> Self {
        ItemBuilder::default()
            .title(value.title)
            .link(value.url)
            .pub_date(value.created_at.format(&Rfc2822).unwrap())
            .guid::<rss::Guid>(rss::Guid {
                value: value.id,
                permalink: false,
            })
            .content(value.content)
            .build()
    }
}

impl From<SsufidSiteData> for rss::Channel {
    fn from(value: SsufidSiteData) -> Self {
        ChannelBuilder::default()
            .title(value.title)
            .link(value.source)
            .description(value.description)
            .items(
                value
                    .items
                    .into_iter()
                    .map(SsufidPost::into)
                    .collect::<Vec<rss::Item>>(),
            )
            .build()
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn test_ssufid_post_to_rss_item() {
        let post = SsufidPost {
            id: "test-id".to_string(),
            title: "Test Title".to_string(),
            category: "Test Category".to_string(),
            url: "https://example.com/test".to_string(),
            created_at: datetime!(2024-03-22 12:00:00 UTC),
            updated_at: None,
            content: "Test Content".to_string(),
        };

        let rss_item: rss::Item = post.into();

        assert_eq!(rss_item.title(), Some("Test Title"));
        assert_eq!(rss_item.link(), Some("https://example.com/test"));
        assert_eq!(rss_item.pub_date(), Some("Fri, 22 Mar 2024 12:00:00 +0000"));
        assert_eq!(rss_item.guid().unwrap().value(), "test-id");
        assert!(!rss_item.guid().unwrap().is_permalink());
        assert_eq!(rss_item.content(), Some("Test Content"));
    }

    #[test]
    fn test_ssufid_site_data_to_rss_channel() {
        let post = SsufidPost {
            id: "test-id".to_string(),
            title: "Test Post".to_string(),
            category: "Test Category".to_string(),
            url: "https://example.com/post".to_string(),
            created_at: datetime!(2024-03-22 12:00:00 UTC),
            updated_at: None,
            content: "Test Content".to_string(),
        };

        let site_data = SsufidSiteData {
            title: "Test Site".to_string(),
            source: "https://example.com".to_string(),
            description: "Test Description".to_string(),
            items: vec![post],
        };

        let rss_channel: rss::Channel = site_data.into();

        assert_eq!(rss_channel.title(), "Test Site");
        assert_eq!(rss_channel.link(), "https://example.com");
        assert_eq!(rss_channel.description(), "Test Description");
        assert_eq!(rss_channel.items().len(), 1);

        let rss_item = &rss_channel.items()[0];
        assert_eq!(rss_item.title(), Some("Test Post"));
        assert_eq!(rss_item.link(), Some("https://example.com/post"));
        assert_eq!(rss_item.pub_date(), Some("Fri, 22 Mar 2024 12:00:00 +0000"));
        assert_eq!(rss_item.guid().unwrap().value(), "test-id");
        assert_eq!(rss_item.content(), Some("Test Content"));
    }
}
