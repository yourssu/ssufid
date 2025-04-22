use std::collections::BTreeMap;

use rss::{
    Category, ChannelBuilder, Enclosure, ItemBuilder,
    extension::{Extension, ExtensionBuilder},
};
use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::{SsufidPost, SsufidSiteData};

const ATOM_NAMESPACE: &str = "http://www.w3.org/2005/Atom";

impl From<SsufidPost> for rss::Item {
    fn from(post: SsufidPost) -> Self {
        let mut builder = ItemBuilder::default();

        let description = post.description.clone().unwrap_or_else(|| {
            post.content.char_indices().nth(50).map_or_else(
                || post.content.clone(),
                |(i, _)| format!("{}...", &post.content[..i]),
            )
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

        if let Some(author) = post.author {
            builder.author(author);
        }

        if !post.category.is_empty() {
            builder.categories(
                post.category
                    .into_iter()
                    .map(|c| Category {
                        name: c,
                        domain: None,
                    })
                    .collect::<Vec<Category>>(),
            );
        }

        if let Some(thumbnail_url) = post.thumbnail {
            let mime_type = mime_guess::from_path(&thumbnail_url)
                .first()
                .map(|m| m.to_string()) // 추론 실패 시 기본값 사용
                .unwrap_or("image/*".to_string());
            builder.enclosure(Enclosure {
                url: thumbnail_url,
                length: "0".to_string(), // Length is often unknown
                mime_type,
            });
        }
        // TODO: use media extension to iterate over attachments
        // for attachment in post.attachments {
        // }

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
    use crate::core::Attachment; // Import Attachment

    #[test]
    fn test_ssufid_post_to_rss_item_basic() {
        let post = SsufidPost {
            id: "test-id-basic".to_string(),
            url: "https://example.com/basic".to_string(),
            author: Some("Basic Author".to_string()),
            title: "Basic Title".to_string(),
            description: None, // Description is None, should fallback to content
            category: vec!["Basic Category".to_string()],
            created_at: datetime!(2024-03-22 12:00:00 UTC),
            updated_at: Some(datetime!(2024-03-27 12:00:00 UTC)),
            thumbnail: Some("https://example.com/basic_thumb.jpg".to_string()),
            content: "Basic Content".to_string(),
            attachments: vec![], // No attachments
            metadata: None,
        };

        let rss_item: rss::Item = post.into();

        assert_eq!(rss_item.title(), Some("Basic Title"));
        assert_eq!(rss_item.link(), Some("https://example.com/basic"));
        assert_eq!(rss_item.pub_date(), Some("Fri, 22 Mar 2024 12:00:00 +0000"));
        assert_eq!(rss_item.guid().unwrap().value(), "test-id-basic");
        assert!(!rss_item.guid().unwrap().is_permalink());
        assert_eq!(
            rss_item
                .extensions()
                .get(ATOM_NAMESPACE)
                .and_then(|m| m.get("atom:updated"))
                .and_then(|v| v.first())
                .and_then(|e| e.value()),
            Some("2024-03-27T12:00:00Z")
        );
        assert_eq!(rss_item.content(), Some("Basic Content"));
        // Check description (should use content if description is None)
        assert_eq!(rss_item.description(), Some("Basic Content"));
        // Check author
        assert_eq!(rss_item.author(), Some("Basic Author"));
        // Check categories
        assert_eq!(rss_item.categories().len(), 1);
        assert_eq!(rss_item.categories()[0].name(), "Basic Category");
        // Check enclosure (thumbnail)
        let enclosure = rss_item.enclosure();
        assert!(enclosure.is_some()); // Only thumbnail
        assert_eq!(
            enclosure.unwrap().url(),
            "https://example.com/basic_thumb.jpg"
        );
        assert_eq!(enclosure.unwrap().mime_type(), "image/*");
    }

    #[test]
    fn test_ssufid_post_to_rss_item_full() {
        let post = SsufidPost {
            id: "test-id-full".to_string(),
            url: "https://example.com/full".to_string(),
            author: None, // Test None author
            title: "Full Title".to_string(),
            description: Some("This is a specific description.".to_string()), // Specific description
            category: vec!["Category A".to_string(), "Category B".to_string()],
            created_at: datetime!(2024-03-23 10:00:00 UTC),
            updated_at: None, // Test None updated_at
            thumbnail: None,  // Test None thumbnail
            content: "This is a longer content that should not be used for the description."
                .to_string(),
            attachments: vec![
                Attachment {
                    url: "https://example.com/attachment1.pdf".to_string(),
                    name: Some("Document 1".to_string()), // Name is not used in RSS enclosure
                    mime_type: Some("application/pdf".to_string()),
                },
                Attachment {
                    url: "https://example.com/attachment2.zip".to_string(),
                    name: None,
                    mime_type: None, // Test None mime_type, should default
                },
            ],
            metadata: Some(
                // Metadata is not used in RSS conversion
                [("key".to_string(), "value".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
            ),
        };

        let rss_item: rss::Item = post.into();

        assert_eq!(rss_item.title(), Some("Full Title"));
        assert_eq!(rss_item.link(), Some("https://example.com/full"));
        assert_eq!(rss_item.pub_date(), Some("Sat, 23 Mar 2024 10:00:00 +0000"));
        assert_eq!(rss_item.guid().unwrap().value(), "test-id-full");
        // Check description (should use the provided description)
        assert_eq!(
            rss_item.description(),
            Some("This is a specific description.")
        );
        // Check author (should be None)
        assert!(rss_item.author().is_none());
        // Check categories
        assert_eq!(rss_item.categories().len(), 2);
        assert_eq!(rss_item.categories()[0].name(), "Category A");
        assert_eq!(rss_item.categories()[1].name(), "Category B");
        // Check updated_at (should be None)
        assert!(rss_item.extensions().get(ATOM_NAMESPACE).is_none());
        // Check content
        assert_eq!(
            rss_item.content(),
            Some("This is a longer content that should not be used for the description.")
        );
    }

    #[test]
    fn test_ssufid_site_data_to_rss_channel() {
        let post1 = SsufidPost {
            // Post with full details
            id: "site-post-1".to_string(),
            url: "https://example.com/post1".to_string(),
            author: Some("Site Author 1".to_string()),
            title: "Site Post 1".to_string(),
            description: Some("Site Post Description 1".to_string()),
            category: vec!["Site Category 1".to_string()],
            created_at: datetime!(2024-03-24 09:00:00 UTC),
            updated_at: Some(datetime!(2024-03-25 09:00:00 UTC)),
            thumbnail: Some("https://example.com/site_thumb1.png".to_string()),
            content: "Site Content 1".to_string(),
            attachments: vec![Attachment {
                url: "https://example.com/site_attach1.txt".to_string(),
                name: None,
                mime_type: Some("text/plain".to_string()),
            }],
            metadata: None,
        };
        let post2 = SsufidPost {
            // Post with minimal details
            id: "site-post-2".to_string(),
            url: "https://example.com/post2".to_string(),
            author: None,
            title: "Site Post 2".to_string(),
            description: None,
            category: vec![],
            created_at: datetime!(2024-03-26 11:00:00 UTC),
            updated_at: None,
            thumbnail: None,
            content: "Site Content 2".to_string(),
            attachments: vec![],
            metadata: None,
        };

        let site_data = SsufidSiteData {
            title: "Test Site".to_string(),
            source: "https://example.com".to_string(),
            description: "Test Site Description".to_string(),
            items: vec![post1, post2], // Include both posts
        };

        let rss_channel: rss::Channel = site_data.into();

        assert_eq!(rss_channel.title(), "Test Site");
        assert_eq!(rss_channel.link(), "https://example.com");
        assert_eq!(rss_channel.description(), "Test Site Description");
        assert_eq!(rss_channel.items().len(), 2); // Check item count

        // --- Assertions for Post 1 ---
        let rss_item1 = &rss_channel.items()[0];
        assert_eq!(rss_item1.title(), Some("Site Post 1"));
        assert_eq!(rss_item1.link(), Some("https://example.com/post1"));
        assert_eq!(
            rss_item1.pub_date(),
            Some("Sun, 24 Mar 2024 09:00:00 +0000")
        );
        assert_eq!(rss_item1.guid().unwrap().value(), "site-post-1");
        assert_eq!(rss_item1.description(), Some("Site Post Description 1"));
        assert_eq!(rss_item1.content(), Some("Site Content 1"));
        assert_eq!(rss_item1.author(), Some("Site Author 1"));
        assert_eq!(rss_item1.categories().len(), 1);
        assert_eq!(rss_item1.categories()[0].name(), "Site Category 1");
        assert_eq!(
            rss_item1
                .extensions()
                .get(ATOM_NAMESPACE)
                .and_then(|m| m.get("atom:updated"))
                .and_then(|v| v.first())
                .and_then(|e| e.value()),
            Some("2024-03-25T09:00:00Z")
        );

        // --- Assertions for Post 2 ---
        let rss_item2 = &rss_channel.items()[1];
        assert_eq!(rss_item2.title(), Some("Site Post 2"));
        assert_eq!(rss_item2.link(), Some("https://example.com/post2"));
        assert_eq!(
            rss_item2.pub_date(),
            Some("Tue, 26 Mar 2024 11:00:00 +0000")
        );
        assert_eq!(rss_item2.guid().unwrap().value(), "site-post-2");
        assert_eq!(rss_item2.description(), Some("Site Content 2")); // Fallback description
        assert_eq!(rss_item2.content(), Some("Site Content 2"));
        assert!(rss_item2.author().is_none());
        assert!(rss_item2.categories().is_empty());
        assert!(rss_item2.extensions().get(ATOM_NAMESPACE).is_none()); // No updated_at
        assert!(rss_item2.enclosure().is_none()); // No thumbnail or attachments
    }
}
