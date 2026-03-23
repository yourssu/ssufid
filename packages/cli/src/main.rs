use std::{collections::HashSet, fs::File, io::BufWriter, ops::Not, path::Path, sync::Arc};

use clap::Parser;
use futures::future::join_all;
use ssufid::core::{
    CalendarCrawlRange, SsufidCalendarPlugin, SsufidCore, SsufidPlugin, SsufidPostPlugin,
};
use ssufid_biz::BizPlugin;
use ssufid_chemeng::ChemEngPlugin;
use ssufid_common::sites::*;
use ssufid_ee::EePlugin;
use ssufid_infocom::InfocomPlugin;
use ssufid_inso::InsoPlugin;
use ssufid_materials::MaterialsPlugin;
use ssufid_media::MediaPlugin;
use ssufid_mediamba::MediambaPlugin;
use ssufid_oasis::OasisPlugin;
use ssufid_ssfilm::SsfilmPlugin;
use ssufid_ssu_academic_calendar::SsuAcademicCalendarPlugin;
use ssufid_ssucatch::SsuCatchPlugin;
use ssufid_ssudorm::SsuDormPlugin;
use ssufid_ssupath::{SsuPathCredential, SsuPathPlugin};
use ssufid_startup::StartupPlugin;
use ssufid_stu::StuPlugin;
use ssufid_study::StudyPlugin;
use time::{
    Date, Duration, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset,
    macros::{format_description, offset},
};
use tokio::io::AsyncWriteExt;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer, filter, layer::SubscriberExt as _, util::SubscriberInitExt};

use crate::macros::register_plugins;

mod macros;

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

    /// The output directory for calendar data.
    #[arg(long = "calendar-out", default_value = "./out/calendar")]
    calendar_out_dir: String,

    /// The cache directory for the fetched data.
    #[arg(long = "cache", default_value = "./.cache")]
    cache_dir: String,

    /// The number of retries for fetching data.
    #[arg(short = 'r', long = "retry", default_value_t = SsufidCore::RETRY_COUNT)]
    retry_count: u32,

    /// The maximum number of posts to fetch.
    #[arg(short = 'l', long = "limit", default_value_t = SsufidCore::POST_COUNT_LIMIT)]
    posts_limit: u32,

    /// Calendar crawl start date in YYYY-MM-DD.
    #[arg(long = "calendar-start-date")]
    calendar_start_date: Option<String>,

    /// Calendar crawl end date in YYYY-MM-DD.
    #[arg(long = "calendar-end-date")]
    calendar_end_date: Option<String>,

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
    validate_calendar_range_flags(&options)?;

    let calendar_range = calendar_crawl_range_from_options(&options)?;
    let out_dir = Path::new(&options.out_dir).to_owned();
    let calendar_out_dir = Path::new(&options.calendar_out_dir).to_owned();

    let core = Arc::new(SsufidCore::new(&options.cache_dir));

    let tasks = construct_tasks(
        core.clone(),
        &out_dir,
        &calendar_out_dir,
        options,
        calendar_range,
    );
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

register_plugins! {
    post: {
    Accounting(AccountingPlugin) => AccountingPlugin::new(),
    Actx(ActxPlugin) => ActxPlugin::new(),
    Biz(BizPlugin) => BizPlugin::new(),
    Bioinfo(BioinfoPlugin) => BioinfoPlugin::new(),
    Chem(ChemPlugin) => ChemPlugin::new(),
    ChemEng(ChemEngPlugin) => ChemEngPlugin::new(),
    Chilan(ChilanPlugin) => ChilanPlugin::new(),
    CseBachelor(CseBachelorPlugin) => CseBachelorPlugin::new(),
    CseGraduate(CseGraduatePlugin) => CseGraduatePlugin::new(),
    CseEmployment(CseEmploymentPlugin) => CseEmploymentPlugin::new(),
    Docs(DocsPlugin) => DocsPlugin::new(),
    Ee(EePlugin) => EePlugin::default(),
    Eco(EcoPlugin) => EcoPlugin::new(),
    Englan(EnglanPlugin) => EnglanPlugin::new(),
    Ensb(EnsbPlugin) => EnsbPlugin::new(),
    Finance(FinancePlugin) => FinancePlugin::new(),
    France(FrancePlugin) => FrancePlugin::new(),
    Gerlan(GerlanPlugin) => GerlanPlugin::new(),
    Gtrade(GtradePlugin) => GtradePlugin::new(),
    History(HistoryPlugin) => HistoryPlugin::new(),
    Iise(IisePlugin) => IisePlugin::new(),
    Inso(InsoPlugin) => InsoPlugin::new(),
    Itrans(ItransPlugin) => ItransPlugin::new(),
    Infocom(InfocomPlugin) => InfocomPlugin::new(),
    Japanstu(JapanstuPlugin) => JapanstuPlugin::new(),
    Korlan(KorlanPlugin) => KorlanPlugin::new(),
    Law(LawPlugin) => LawPlugin::new(),
    Lawyer(LawyerPlugin) => LawyerPlugin::new(),
    LifelongEdu(LifelongEduPlugin) => LifelongEduPlugin::new(),
    Materials(MaterialsPlugin) => MaterialsPlugin::new(),
    Masscom(MasscomPlugin) => MasscomPlugin::new(),
    Math(MathPlugin) => MathPlugin::new(),
    Media(MediaPlugin) => MediaPlugin,
    Mediamba(MediambaPlugin) => MediambaPlugin,
    Mysoongsil(MysoongsilPlugin) => MysoongsilPlugin::new(),
    Oasis(OasisPlugin) => OasisPlugin,
    Philo(PhiloPlugin) => PhiloPlugin::new(),
    Physics(PhysicsPlugin) => PhysicsPlugin::new(),
    Politics(PoliticsPlugin) => PoliticsPlugin::new(),
    Pubad(PubadPlugin) => PubadPlugin::new(),
    Sec(SecPlugin) => SecPlugin::new(),
    Sls(SlsPlugin) => SlsPlugin::new(),
    Soar(SoarPlugin) => SoarPlugin::new(),
    Ssfilm(SsfilmPlugin) => SsfilmPlugin,
    SsuCatch(SsuCatchPlugin) => SsuCatchPlugin::new(),
    SsuDorm(SsuDormPlugin) => SsuDormPlugin::new(),
    SsuPath(SsuPathPlugin) => SsuPathPlugin::new(SsuPathCredential::Password(
        std::env::var("SSU_ID").unwrap_or_default(),
        std::env::var("SSU_PASSWORD").unwrap_or_default()
    )),
    Startup(StartupPlugin) => StartupPlugin,
    Stu(StuPlugin) => StuPlugin,
    Study(StudyPlugin) => StudyPlugin,
    Sports(SportsPlugin) => SportsPlugin::new(),
    SwBachelor(SwBachelorPlugin) => SwBachelorPlugin::new(),
    SwGraduate(SwGraduatePlugin) => SwGraduatePlugin::new(),
    },
    calendar: {
    SsuAcademicCalendar(SsuAcademicCalendarPlugin) => SsuAcademicCalendarPlugin,
    }
}

pub(crate) async fn save_run<T: SsufidPostPlugin>(
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

#[allow(dead_code)]
pub(crate) async fn save_calendar_run<T: SsufidCalendarPlugin>(
    core: Arc<SsufidCore>,
    base_out_dir: &Path,
    plugin: T,
    calendar_range: CalendarCrawlRange,
    retry_count: u32,
) -> eyre::Result<()> {
    let site = core
        .run_calendar_with_retry(&plugin, &calendar_range, retry_count)
        .await?;
    let json = serde_json::to_string_pretty(&site)?;
    let ics = site.to_ics();

    let out_dir = base_out_dir.join(T::IDENTIFIER);
    tokio::fs::create_dir_all(&out_dir).await?;

    let mut json_file = tokio::fs::File::create(out_dir.join("data.json")).await?;
    json_file.write_all(json.as_bytes()).await?;

    let mut ics_file = tokio::fs::File::create(out_dir.join("calendar.ics")).await?;
    ics_file.write_all(ics.as_bytes()).await?;
    Ok(())
}

fn validate_calendar_range_flags(options: &SsufidDaemonOptions) -> eyre::Result<()> {
    match (&options.calendar_start_date, &options.calendar_end_date) {
        (Some(_), Some(_)) | (None, None) => Ok(()),
        _ => {
            eyre::bail!("--calendar-start-date and --calendar-end-date must be provided together.")
        }
    }
}

fn calendar_crawl_range_from_options(
    options: &SsufidDaemonOptions,
) -> eyre::Result<CalendarCrawlRange> {
    match (&options.calendar_start_date, &options.calendar_end_date) {
        (Some(start), Some(end)) => CalendarCrawlRange::new(
            parse_calendar_start_date(start)?,
            parse_calendar_end_date(end)?,
        )
        .map_err(eyre::Error::msg),
        (None, None) => default_calendar_crawl_range(),
        _ => unreachable!("calendar range flags should have been validated"),
    }
}

fn default_calendar_crawl_range() -> eyre::Result<CalendarCrawlRange> {
    let now = OffsetDateTime::now_utc().to_offset(kst_offset());
    let start_date = now.date() - Duration::days(SsufidCore::CALENDAR_DAY_LIMIT as i64);
    let start = PrimitiveDateTime::new(start_date, Time::MIDNIGHT).assume_offset(kst_offset());
    let end = PrimitiveDateTime::new(now.date(), end_of_day()).assume_offset(kst_offset());

    CalendarCrawlRange::new(start, end).map_err(eyre::Error::msg)
}

fn parse_calendar_start_date(date: &str) -> eyre::Result<OffsetDateTime> {
    Ok(PrimitiveDateTime::new(parse_cli_date(date)?, Time::MIDNIGHT).assume_offset(kst_offset()))
}

fn parse_calendar_end_date(date: &str) -> eyre::Result<OffsetDateTime> {
    Ok(PrimitiveDateTime::new(parse_cli_date(date)?, end_of_day()).assume_offset(kst_offset()))
}

fn parse_cli_date(date: &str) -> eyre::Result<Date> {
    let format = format_description!("[year]-[month]-[day]");
    Date::parse(date, &format).map_err(|e| eyre::eyre!("Invalid date '{date}': {e}"))
}

fn kst_offset() -> UtcOffset {
    offset!(+9)
}

fn end_of_day() -> Time {
    Time::from_hms(23, 59, 59).expect("valid end of day time")
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
