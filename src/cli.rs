use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use url::Url;

use crate::crawler::{crawl, CrawlOptions};
use crate::headless::{find_chrome, print_install_instructions};
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
    bot: bool,

    /// Use a headless Chrome/Chromium browser to render JavaScript (requires Chrome installed)
    #[arg(long)]
    headless: bool,

    /// Save a full-page screenshot for each crawled page (requires --headless)
    #[arg(long)]
    screenshot: bool,

    /// Custom User-Agent header
    #[arg(long)]
    user_agent: Option<String>,

    /// Custom Referer header
    #[arg(long)]
    referer: Option<String>,
}

struct PromptResult {
    max_depth: u32,
    placeholder: String,
    bot: bool,
    headless: bool,
    screenshot: bool,
    concurrency: usize,
    delay_ms: u64,
    sitemap: bool,
    allow_external_assets: bool,
}

/// Parse CLI arguments and run the crawler.
pub async fn run_cli() -> Result<()> {
    let args = Args::parse();

    Url::parse(&args.url).map_err(|_| anyhow::anyhow!("Invalid URL provided"))?;

    // Enter interactive mode when no options were explicitly set and stdin is a terminal
    let interactive = args.max_depth.is_none()
        && args.placeholder.is_none()
        && !args.bot
        && !args.headless
        && std::io::stdin().is_terminal();

    let opts = if interactive {
        prompt_options()?
    } else {
        PromptResult {
            max_depth: args.max_depth.unwrap_or(2),
            placeholder: resolve_placeholder(args.placeholder.as_deref()),
            bot: args.bot,
            headless: args.headless,
            screenshot: args.screenshot,
            concurrency: args.concurrency,
            delay_ms: args.delay_ms,
            sitemap: args.sitemap,
            allow_external_assets: args.allow_external_assets,
        }
    };

    configure_requests(ConfigureOptions {
        delay_ms: Some(opts.delay_ms),
        bot_mode: opts.bot,
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

    if opts.headless {
        return run_headless(
            start_url.as_str(),
            &out_dir,
            opts.max_depth,
            &opts.placeholder,
            opts.screenshot,
        )
        .await;
    }

    crawl(
        start_url.as_str(),
        &out_dir,
        CrawlOptions {
            max_depth: opts.max_depth,
            concurrency: opts.concurrency,
            sitemap: opts.sitemap,
            allow_external_assets: opts.allow_external_assets,
            placeholder: opts.placeholder,
        },
    )
    .await
}

async fn run_headless(
    start_url: &str,
    out_dir: &std::path::Path,
    max_depth: u32,
    placeholder: &str,
    screenshot: bool,
) -> Result<()> {
    let chrome = match find_chrome() {
        Some(p) => p,
        None => {
            print_install_instructions();
            anyhow::bail!("Chrome or Chromium not found");
        }
    };

    println!("Using browser: {}", chrome.display());

    #[cfg(feature = "headless")]
    {
        use crate::headless::crawler::{crawl, HeadlessOptions};
        crawl(
            start_url,
            out_dir,
            &chrome,
            HeadlessOptions {
                max_depth,
                placeholder: placeholder.to_string(),
                screenshot,
            },
        )
        .await
    }

    #[cfg(not(feature = "headless"))]
    {
        let _ = (
            start_url,
            out_dir,
            max_depth,
            placeholder,
            screenshot,
            chrome,
        );
        anyhow::bail!(
            "Headless mode is not compiled in.\n\
             Rebuild with:  cargo build --features headless"
        )
    }
}

fn prompt_options() -> Result<PromptResult> {
    let theme = ColorfulTheme::default();

    // --- Quick vs Extended ---
    let extended = Select::with_theme(&theme)
        .with_prompt("Setup")
        .items(&[
            "Quick  (depth, images, mode)",
            "Extended  (+ headless, concurrency, delay, sitemap, assets)",
        ])
        .default(0)
        .interact()?
        == 1;

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
    let mode_idx = Select::with_theme(&theme)
        .with_prompt("Mode")
        .items(&[
            "Simulate browser  (default, avoids bot detection)",
            "Identify as bot  (--bot)",
            "Headless Chrome  (--headless, renders JavaScript)",
        ])
        .default(0)
        .interact()?;

    let bot = mode_idx == 1;
    let headless = mode_idx == 2;

    // --- Screenshot (only when headless is selected) ---
    let screenshot = if headless {
        Confirm::with_theme(&theme)
            .with_prompt("Save full-page screenshots?  (--screenshot)")
            .default(false)
            .interact()?
    } else {
        false
    };

    if !extended {
        return Ok(PromptResult {
            max_depth,
            placeholder,
            bot,
            headless,
            screenshot,
            concurrency: 4,
            delay_ms: 300,
            sitemap: true,
            allow_external_assets: true,
        });
    }

    // --- Concurrency ---
    let conc_idx = Select::with_theme(&theme)
        .with_prompt("Parallel downloads")
        .items(&[
            "1 – sequential",
            "2",
            "4  (default)",
            "8",
            "Enter a custom number …",
        ])
        .default(2)
        .interact()?;

    let concurrency: usize = match conc_idx {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => Input::with_theme(&theme)
            .with_prompt("Concurrency")
            .default(4usize)
            .interact_text()?,
    };

    // --- Delay ---
    let delay_idx = Select::with_theme(&theme)
        .with_prompt("Delay between requests")
        .items(&[
            "0 ms – no delay",
            "150 ms",
            "300 ms  (default)",
            "500 ms – polite",
            "Enter a custom value …",
        ])
        .default(2)
        .interact()?;

    let delay_ms: u64 = match delay_idx {
        0 => 0,
        1 => 150,
        2 => 300,
        3 => 500,
        _ => Input::with_theme(&theme)
            .with_prompt("Delay (ms)")
            .default(300u64)
            .interact_text()?,
    };

    // --- Sitemap ---
    let sitemap = Confirm::with_theme(&theme)
        .with_prompt("Use sitemap.xml as seed?  (--sitemap)")
        .default(true)
        .interact()?;

    // --- External assets ---
    let allow_external_assets = Confirm::with_theme(&theme)
        .with_prompt("Download external CSS/JS?  (--allow-external-assets)")
        .default(true)
        .interact()?;

    Ok(PromptResult {
        max_depth,
        placeholder,
        bot,
        headless,
        screenshot,
        concurrency,
        delay_ms,
        sitemap,
        allow_external_assets,
    })
}

fn resolve_placeholder(raw: Option<&str>) -> String {
    match raw {
        Some("real") => "real".to_string(),
        Some("local") => "local".to_string(),
        _ => "external".to_string(),
    }
}
