use std::path::Path;

use ssufid::{core::SsufidCore, plugins::example::ExamplePlugin};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let core = SsufidCore::new("./.ssufid/cache");

    // TODO: 100이라는 값을 따로 상수로 관리할지?
    let site = core.run(ExamplePlugin, 100).await?;

    let out_dir = Path::new("./out/example");
    tokio::fs::create_dir_all(out_dir).await?;

    let mut example_file = tokio::fs::File::create_new(out_dir.join("data.json")).await?;
    // let mut example_rss_file = tokio::fs::File::create_new(out_dir.join("rss.xml")).await?;
    let example_json = serde_json::to_string_pretty(&site)?;
    // let example_rss = site.to_rss()?;
    // TODO: Write rss structure to xml
    example_file.write_all(example_json.as_bytes()).await?;

    core.save_cache().await?;
    Ok(())
}
