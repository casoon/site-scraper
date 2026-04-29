use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use url::Url;

use crate::crawler::{crawl, CrawlOptions};
use crate::network::fetch::{configure_requests, resolve_redirect, ConfigureOptions};
use crate::utils::filesystem::{ensure_dir, safe_filename};

#[derive(Parser)]
#[command(
    name = "site-scraper",
    version,
    about = "CLI utility to mirror a website (HTML + CSS) into a local folder."
)]
struct Args {
    /// URL to scrape
    url: String,

    /// Maximum crawl depth relative to the start page
    #[arg(long)]
    max_depth: Option<u32>,

    /// Number of parallel downloads
    #[arg(long, default_value_t = 4)]
    concurrency: usize,

    /// Delay between requests in milliseconds
    #[arg(long, default_value_t = 300)]
    delay_ms: u64,

    /// Image placeholder strategy: "real" (download originals), "local" (gray PNG), or "external" (placehold.co)
    #[arg(long)]
    placeholder: Option<String>,

    /// Include sitemap.xml URLs as seeds
    #[arg(long, default_value_t = true)]
    sitemap: bool,

    /// Download external CSS/JS or leave as-is
    #[arg(long, default_value_t = true)]
    allow_external_assets: bool,

    /// Identify as bot/crawler instead of simulating a browser
    #[arg(long)]
    bot: Option<bool>,

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

    Url::parse(&args.url).map_err(|_| anyhow::anyhow!("Invalid URL provided"))?;

    // Enter interactive mode when no options were explicitly set and stdin is a terminal
    let interactive = args.max_depth.is_none()
        && args.placeholder.is_none()
        && args.bot.is_none()
        && std::io::stdin().is_terminal();

    let (max_depth, placeholder, bot) = if interactive {
        prompt_options()?
    } else {
        (
            args.max_depth.unwrap_or(2),
            resolve_placeholder(args.placeholder.as_deref()),
            args.bot.unwrap_or(false),
        )
    };

    configure_requests(ConfigureOptions {
        delay_ms: Some(args.delay_ms),
        bot_mode: bot,
        user_agent: args.user_agent,
        referer: args.referer,
    });

    // Resolve the canonical start URL by following any redirects (e.g. www → non-www)
    let canonical = resolve_redirect(&args.url).await;
    let start_url =
        Url::parse(&canonical).map_err(|_| anyhow::anyhow!("Invalid URL after redirect"))?;

    let host_dir = safe_filename(start_url.host_str().unwrap_or("unknown"));
    if host_dir.is_empty() {
        anyhow::bail!("Unable to derive output directory name");
    }

    let base_output = PathBuf::from("output");
    ensure_dir(&base_output).await?;
    let out_dir = base_output.join(&host_dir);

    // Remove and recreate output directory
    let _ = tokio::fs::remove_dir_all(&out_dir).await;
    ensure_dir(&out_dir).await?;

    crawl(
        start_url.as_str(),
        &out_dir,
        CrawlOptions {
            max_depth,
            concurrency: args.concurrency,
            sitemap: args.sitemap,
            allow_external_assets: args.allow_external_assets,
            placeholder,
        },
    )
    .await
}

fn prompt_options() -> Result<(u32, String, bool)> {
    let theme = ColorfulTheme::default();

    // --- Crawl depth ---
    let depth_idx = Select::with_theme(&theme)
        .with_prompt("Crawl depth")
        .items(&[
            "1 – start page only",
            "2 – standard (recommended)",
            "3 – deeper",
            "Enter a custom number …",
        ])
        .default(1)
        .interact()?;

    let max_depth: u32 = match depth_idx {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => Input::with_theme(&theme)
            .with_prompt("Depth")
            .default(2u32)
            .interact_text()?,
    };

    // --- Images ---
    let img_idx = Select::with_theme(&theme)
        .with_prompt("Images")
        .items(&[
            "Download originals  (--placeholder real)",
            "Local gray placeholder  (--placeholder local)",
            "External – placehold.co  (--placeholder external)",
        ])
        .default(0)
        .interact()?;

    let placeholder = match img_idx {
        0 => "real",
        1 => "local",
        _ => "external",
    }
    .to_string();

    // --- Mode ---
    let bot_idx = Select::with_theme(&theme)
        .with_prompt("Mode")
        .items(&[
            "Simulate browser  (default, avoids bot detection)",
            "Identify as bot  (--bot)",
        ])
        .default(0)
        .interact()?;

    let bot = bot_idx == 1;

    Ok((max_depth, placeholder, bot))
}

fn resolve_placeholder(raw: Option<&str>) -> String {
    match raw {
        Some("real") => "real".to_string(),
        Some("local") => "local".to_string(),
        _ => "external".to_string(),
    }
}
