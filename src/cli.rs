use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use url::Url;

use crate::crawler::{crawl, CrawlOptions};
use crate::network::fetch::{configure_requests, ConfigureOptions};
use crate::utils::filesystem::{ensure_dir, safe_filename};

#[derive(Parser)]
#[command(name = "site-scraper", version, about = "CLI utility to mirror a website (HTML + CSS) into a local folder.")]
struct Args {
    /// URL to scrape
    url: String,

    /// Maximum crawl depth relative to the start page
    #[arg(long, default_value_t = 2)]
    max_depth: u32,

    /// Number of parallel downloads
    #[arg(long, default_value_t = 4)]
    concurrency: usize,

    /// Delay between requests in milliseconds
    #[arg(long, default_value_t = 300)]
    delay_ms: u64,

    /// Image placeholder strategy: "external" or "local"
    #[arg(long, default_value = "external")]
    placeholder: String,

    /// Include sitemap.xml URLs as seeds
    #[arg(long, default_value_t = true)]
    sitemap: bool,

    /// Download external CSS/JS or leave as-is
    #[arg(long, default_value_t = true)]
    allow_external_assets: bool,

    /// Identify as bot/crawler instead of simulating a browser
    #[arg(long)]
    bot: bool,

    /// Custom User-Agent header
    #[arg(long)]
    user_agent: Option<String>,

    /// Custom Referer header
    #[arg(long)]
    referer: Option<String>,
}

/// Parse CLI arguments and run the crawler.
pub async fn run_cli() -> Result<()> {
    let args = Args::parse();

    let start_url = Url::parse(&args.url).map_err(|_| anyhow::anyhow!("Invalid URL provided"))?;

    configure_requests(ConfigureOptions {
        delay_ms: Some(args.delay_ms),
        bot_mode: args.bot,
        user_agent: args.user_agent,
        referer: args.referer,
    });

    let host_dir = safe_filename(start_url.host_str().unwrap_or("unknown"));
    if host_dir.is_empty() {
        anyhow::bail!("Unable to derive output directory name");
    }

    let base_output = PathBuf::from("output");
    ensure_dir(&base_output).await?;
    let out_dir = base_output.join(&host_dir);

    let placeholder = if args.placeholder == "local" {
        "local".to_string()
    } else {
        "external".to_string()
    };

    // Remove and recreate output directory
    let _ = tokio::fs::remove_dir_all(&out_dir).await;
    ensure_dir(&out_dir).await?;

    crawl(
        start_url.as_str(),
        &out_dir,
        CrawlOptions {
            max_depth: args.max_depth,
            concurrency: args.concurrency,
            sitemap: args.sitemap,
            allow_external_assets: args.allow_external_assets,
            placeholder,
        },
    )
    .await
}
