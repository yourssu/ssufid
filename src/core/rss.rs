use std::collections::BTreeMap;

use rss::{
    extension::{Extension, ExtensionBuilder},
    ChannelBuilder, ItemBuilder,
};
use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::{SsufidPost, SsufidSiteData};

const ATOM_NAMESPACE: &str = "http://www.w3.org/2005/Atom";

impl From<SsufidPost> for rss::Item {
    fn from(post: SsufidPost) -> Self {
        let mut builder = ItemBuilder::default();

        let description = post
            .content
            .char_indices()
            .nth(50)
            .map_or_else(|| post.content.clone(), |(i, _)| {
                format!("{}...", &post.content[..i])
            });

        builder
            .title(post.title)
            .link(post.url.clone())
            .pub_date(post.created_at.format(&Rfc2822).unwrap())
            .guid::<rss::Guid>(rss::Guid {
                value: post.id,
                permalink: false,
            })
            .description(description)
            .content(post.content);
        if let Some(updated_at) = post.updated_at {
            let extension = ExtensionBuilder::default()
                .name("atom:updated")
                .value(updated_at.format(&Rfc3339).unwrap())
                .build();
            builder.extension((
                ATOM_NAMESPACE.into(),
                [("atom:updated".to_string(), vec![extension])]
                    .into_iter()
                    .collect::<BTreeMap<String, Vec<Extension>>>(),
            ));
        }
        builder.build()
    }
}

impl From<SsufidSiteData> for rss::Channel {
    fn from(site: SsufidSiteData) -> Self {
        ChannelBuilder::default()
            .title(site.title)
            .link(site.source)
            .description(site.description)
            .items(
                site.items
                    .into_iter()
                    .map(SsufidPost::into)
                    .collect::<Vec<rss::Item>>(),
            )
            .namespace(("atom".to_string(), ATOM_NAMESPACE.to_string()))
            .namespace((
                "content".to_string(),
                "http://purl.org/rss/1.0/modules/content/".to_string(),
            ))
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
            updated_at: Some(datetime!(2024-03-27 12:00:00 UTC)),
            content: "Test Content".to_string(),
        };

        let rss_item: rss::Item = post.into();

        assert_eq!(rss_item.title(), Some("Test Title"));
        assert_eq!(rss_item.link(), Some("https://example.com/test"));
        assert_eq!(rss_item.pub_date(), Some("Fri, 22 Mar 2024 12:00:00 +0000"));
        assert_eq!(rss_item.guid().unwrap().value(), "test-id");
        assert!(!rss_item.guid().unwrap().is_permalink());
        assert_eq!(
            rss_item
                .extensions()
                .get("http://www.w3.org/2005/Atom")
                .and_then(|m| m.get("atom:updated"))
                .and_then(|v| v.first())
                .and_then(|e| e.value()),
            Some("2024-03-27T12:00:00Z")
        );
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
        assert_eq!(rss_item.description(), Some("Test Content"));
        assert_eq!(rss_item.content(), Some("Test Content"));
    }
}
