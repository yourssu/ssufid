use std::path::Path;

use ssufid::{core::SsufidCore, plugins::example::ExamplePlugin};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let mut core = SsufidCore::new("./.ssufid/cache");

    let site = core.run(ExamplePlugin).await?;

    let out_dir = Path::new("./out/example");
    tokio::fs::create_dir_all(out_dir).await?;

    let mut example_file = tokio::fs::File::create_new(out_dir.join("data.json")).await?;
    let example_json = serde_json::to_string_pretty(&site)?;
    example_file.write_all(example_json.as_bytes()).await?;
    Ok(())
}
