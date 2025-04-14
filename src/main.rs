use std::{collections::HashSet, io::BufWriter, ops::Not, path::Path, sync::Arc};

use clap::Parser;
use env_logger::{Builder, Env};
use futures::future::join_all;
use log::error;
use ssufid::{
    core::{SsufidCore, SsufidPlugin},
    plugins::{
        ssu_catch::SsuCatchPlugin,
        ssu_path::{SsuPathCredential, SsuPathPlugin},
    },
};
use tokio::io::AsyncWriteExt;

#[derive(Parser, Debug)]
#[command(
    name = "ssufid",
    about = "A tool to fetch and save data from SSU sites.",
    version
)]
struct SsufidDaemonOptions {
    /// The output directory for the fetched data.
    #[arg(short = 'o', long = "out", default_value = "./out")]
    out_dir: String,

    /// The cache directory for the fetched data.
    #[arg(long = "cache", default_value = "./.cache")]
    cache_dir: String,

    /// The number of retries for fetching data.
    #[arg(short = 'r', long = "retry", default_value_t = SsufidCore::RETRY_COUNT)]
    retry_count: u32,

    /// The maximum number of posts to fetch.
    #[arg(short = 'l', long = "limit", default_value_t = SsufidCore::POST_COUNT_LIMIT)]
    posts_limit: u32,

    /// The sites to include in the fetch. By default, all sites are included.
    /// This will override the default sites.
    #[arg(short = 'i', long, value_delimiter = ',')]
    include: Vec<String>,
    #[arg(short = 'e', long, value_delimiter = ',')]
    /// The sites to exclude from the fetch.
    exclude: Vec<String>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    Builder::from_env(Env::default().filter_or("RUST_LOG", "info")).init();

    color_eyre::install()?;
    let options = SsufidDaemonOptions::parse();

    if !options.include.is_empty() && !options.exclude.is_empty() {
        eyre::bail!("You cannot use both --include and --exclude options at the same time.");
    }

    let out_dir = Path::new(&options.out_dir).to_owned();

    let core = Arc::new(SsufidCore::new(&options.cache_dir));

    let tasks = construct_tasks(core.clone(), &out_dir, options);
    let tasks_len = tasks.len();

    // Run all tasks and collect errors
    let errors: Vec<eyre::Report> = join_all(tasks)
        .await
        .into_iter()
        .filter_map(|r| r.err())
        .collect();

    core.save_cache().await?;

    if errors.is_empty() {
        Ok(())
    } else {
        for err in &errors {
            error!("{:?}", err);
        }
        Err(eyre::eyre!("{} of {} Run failed", errors.len(), tasks_len))
    }
}

pub enum SsufidPluginRegistry {
    SsuCatch(SsuCatchPlugin),
    SsuPath(SsuPathPlugin),
}

impl SsufidPluginRegistry {
    async fn save_run(
        self,
        core: Arc<SsufidCore>,
        out_dir: &Path,
        posts_limit: u32,
        retry_count: u32,
    ) -> eyre::Result<()> {
        match self {
            SsufidPluginRegistry::SsuCatch(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::SsuPath(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
        }
    }
}

fn construct_tasks(
    core: Arc<SsufidCore>,
    out_dir: &Path,
    options: SsufidDaemonOptions,
) -> Vec<impl std::future::Future<Output = eyre::Result<()>>> {
    let include: Option<HashSet<String>> = options
        .include
        .is_empty()
        .not()
        .then_some(HashSet::from_iter(options.include));
    let exclude: Option<HashSet<String>> = options
        .exclude
        .is_empty()
        .not()
        .then_some(HashSet::from_iter(options.exclude));
    let tasks = [
        (
            SsuCatchPlugin::IDENTIFIER,
            SsufidPluginRegistry::SsuCatch(SsuCatchPlugin::new()),
        ),
        (
            SsuPathPlugin::IDENTIFIER,
            SsufidPluginRegistry::SsuPath(SsuPathPlugin::new(SsuPathCredential::Password(
                std::env::var("SSU_ID").unwrap_or_default(),
                std::env::var("SSU_PASSWORD").unwrap_or_default(),
            ))),
        ),
    ];

    if let Some(include) = include {
        tasks
            .into_iter()
            .filter_map(|(id, task)| {
                include.contains(id).then_some(task.save_run(
                    core.clone(),
                    out_dir,
                    options.posts_limit,
                    options.retry_count,
                ))
            })
            .collect()
    } else if let Some(exclude) = exclude {
        tasks
            .into_iter()
            .filter_map(|(id, task)| {
                exclude.contains(id).not().then_some(task.save_run(
                    core.clone(),
                    out_dir,
                    options.posts_limit,
                    options.retry_count,
                ))
            })
            .collect()
    } else {
        tasks
            .into_iter()
            .map(|(_, task)| {
                task.save_run(
                    core.clone(),
                    out_dir,
                    options.posts_limit,
                    options.retry_count,
                )
            })
            .collect()
    }
}

async fn save_run<T: SsufidPlugin>(
    core: Arc<SsufidCore>,
    base_out_dir: &Path,
    plugin: T,
    posts_limit: u32,
    retry_count: u32,
) -> eyre::Result<()> {
    let site = core
        .run_with_retry(&plugin, posts_limit, retry_count)
        .await?;
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
