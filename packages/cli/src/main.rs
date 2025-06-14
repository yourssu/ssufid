use std::{collections::HashSet, fs::File, io::BufWriter, ops::Not, path::Path, sync::Arc};

use clap::Parser;
use futures::future::join_all;
use ssufid::core::{SsufidCore, SsufidPlugin};
use ssufid_itsites::{
    cse::{
        bachelor::CseBachelorPlugin, employment::CseEmploymentPlugin, graduate::CseGraduatePlugin,
    },
    sec::SecPlugin,
    sw::{bachelor::SwBachelorPlugin, graduate::SwGraduatePlugin},
};
use ssufid_media::MediaPlugin;
use ssufid_mediamba::MediambaPlugin;
use ssufid_ssucatch::SsuCatchPlugin;
use ssufid_aix::AixPlugin;
use ssufid_ssupath::{SsuPathCredential, SsuPathPlugin};
use tokio::io::AsyncWriteExt;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer, filter, layer::SubscriberExt as _, util::SubscriberInitExt};

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
    setup_tracing()?;

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
            tracing::error!("{err:?}");
        }
        Err(eyre::eyre!("{} of {} Run failed", errors.len(), tasks_len))
    }
}

pub enum SsufidPluginRegistry {
    SsuCatch(SsuCatchPlugin),
    SsuPath(SsuPathPlugin),
    CseBachelor(CseBachelorPlugin),
    CseGraduate(CseGraduatePlugin),
    CseEmployment(CseEmploymentPlugin),
    Media(MediaPlugin),
    Mediamba(MediambaPlugin),
    SwBachelor(SwBachelorPlugin),
    SwGraduate(SwGraduatePlugin),
    SecBachelor(SecPlugin),
    Aix(AixPlugin),
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
            SsufidPluginRegistry::CseBachelor(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::CseGraduate(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::CseEmployment(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::Media(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::Mediamba(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::SwBachelor(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::SwGraduate(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::SecBachelor(plugin) => {
                save_run(core, out_dir, plugin, posts_limit, retry_count).await
            }
            SsufidPluginRegistry::Aix(plugin) => {
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
            SsufidPluginRegistry::SsuCatch(SsuCatchPlugin::default()),
        ),
        (
            SsuPathPlugin::IDENTIFIER,
            SsufidPluginRegistry::SsuPath(SsuPathPlugin::new(SsuPathCredential::Password(
                std::env::var("SSU_ID").unwrap_or_default(),
                std::env::var("SSU_PASSWORD").unwrap_or_default(),
            ))),
        ),
        (
            CseBachelorPlugin::IDENTIFIER,
            SsufidPluginRegistry::CseBachelor(CseBachelorPlugin::default()),
        ),
        (
            CseGraduatePlugin::IDENTIFIER,
            SsufidPluginRegistry::CseGraduate(CseGraduatePlugin::default()),
        ),
        (
            CseEmploymentPlugin::IDENTIFIER,
            SsufidPluginRegistry::CseEmployment(CseEmploymentPlugin::default()),
        ),
        (
            MediaPlugin::IDENTIFIER,
            SsufidPluginRegistry::Media(MediaPlugin),
        ),
        (
            MediambaPlugin::IDENTIFIER,
            SsufidPluginRegistry::Mediamba(MediambaPlugin),
        ),
        (
            SwBachelorPlugin::IDENTIFIER,
            SsufidPluginRegistry::SwBachelor(SwBachelorPlugin::default()),
        ),
        (
            SwGraduatePlugin::IDENTIFIER,
            SsufidPluginRegistry::SwGraduate(SwGraduatePlugin::default()),
        ),
        (
            SecPlugin::IDENTIFIER,
            SsufidPluginRegistry::SecBachelor(SecPlugin::default()),
        ),
        (
            AixPlugin::IDENTIFIER,
            SsufidPluginRegistry::Aix(AixPlugin::default())
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

fn setup_tracing() -> eyre::Result<()> {
    std::fs::create_dir_all("reports").or_else(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(e)
        }
    })?;
    let stdout_log = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_level(true)
        .with_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        );

    let content_report_file = File::create("reports/content_report.json")
        .map_err(|e| eyre::eyre!("Failed to create log file: {e}"))?;
    let content_report_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_span_list(false)
        .with_writer(Arc::new(content_report_file))
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target() == "content_update"
        }));

    let error_report_file = File::create("reports/error_report.json")
        .map_err(|e| eyre::eyre!("Failed to create error log file: {e}"))?;
    let error_report_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(Arc::new(error_report_file))
        .with_filter(LevelFilter::ERROR);

    tracing_subscriber::registry()
        .with(stdout_log)
        .with(content_report_layer)
        .with(error_report_layer)
        .init();
    Ok(())
}
