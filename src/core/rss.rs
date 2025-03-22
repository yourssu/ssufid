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
