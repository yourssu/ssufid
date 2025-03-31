use std::{io::BufWriter, path::Path, sync::Arc};

use futures::future::join_all;
use ssufid::{
    core::{SsufidCore, SsufidPlugin},
    plugins::ssu_catch::SsuCatchPlugin,
};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let core = Arc::new(SsufidCore::new("./.ssufid/cache"));

    let out_dir = Path::new("./out");

    let tasks = vec![save_run(core.clone(), out_dir, SsuCatchPlugin::default())];

    join_all(tasks).await.into_iter().for_each(|result| {
        if let Err(err) = result {
            eprintln!("{:?}", err);
        }
    });

    core.save_cache().await?;
    Ok(())
}

async fn save_run<T: SsufidPlugin>(
    core: Arc<SsufidCore>,
    base_out_dir: &Path,
    plugin: T,
) -> eyre::Result<()> {
    let site = core.run(plugin, SsufidCore::POST_COUNT_LIMIT).await?;
    let json = serde_json::to_string_pretty(&site)?;

    // Use synchronous BufWriter to write pretty xml string.
    let buf = site
        .to_rss()
        .pretty_write_to(BufWriter::new(Vec::new()), b' ', 2)?;
    let rss = String::from_utf8(buf.into_inner()?)?;

    let out_dir = base_out_dir.join(T::IDENTIFIER);
    tokio::fs::create_dir_all(&out_dir).await?;

    let mut json_file = tokio::fs::File::create(out_dir.join("data.json")).await?;
    json_file.write_all(json.as_bytes()).await?;

    let mut rss_file = tokio::fs::File::create(out_dir.join("rss.xml")).await?;
    rss_file.write_all(rss.as_bytes()).await?;
    Ok(())
}
