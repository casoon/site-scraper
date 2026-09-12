use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use url::Url;

use crate::crawler::{crawl, CrawlOptions};
use crate::headless::{chrome_not_found, find_chrome, Device};
use crate::network::fetch::{configure_requests, resolve_redirect, ConfigureOptions};
use crate::output;
use crate::screenshot;
use crate::utils::filesystem::{ensure_dir, safe_filename};

#[derive(Parser)]
#[command(
    name = "site-scraper",
    version,
    about = "CLI utility to mirror a website (HTML + CSS) into a local folder.",
    // Keep `site-scraper <URL> [OPTIONS]` working next to subcommands
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    crawl: Args,
}

#[derive(Subcommand)]
enum Command {
    /// Only take full-page screenshots of one URL or a list of URLs (requires Chrome)
    Screenshot(ScreenshotArgs),
}

#[derive(clap::Args)]
struct ScreenshotArgs {
    /// URL to screenshot
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    url: Option<String>,

    /// Text file with one URL per line (blank lines and # comments are ignored)
    #[arg(long)]
    file: Option<PathBuf>,

    /// Output directory (existing screenshots are kept)
    #[arg(long, default_value = "screenshots")]
    output: PathBuf,

    /// Number of parallel browser pages
    #[arg(long, default_value_t = 2, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    concurrency: usize,

    /// Viewport to render with
    #[arg(long, value_enum, default_value = "desktop")]
    device: Device,

    /// Seconds to wait for browser navigation before failing
    #[arg(long, default_value_t = 60, value_parser = clap::builder::RangedU64ValueParser::<u64>::new().range(1..))]
    timeout: u64,
}

#[derive(clap::Args)]
struct Args {
    /// URL to scrape
    #[arg(required = true)]
    url: Option<String>,

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

    /// Include sitemap.xml URLs as seeds (default)
    #[arg(long, overrides_with = "no_sitemap")]
    sitemap: bool,

    /// Don't use sitemap.xml URLs as seeds
    #[arg(long, overrides_with = "sitemap")]
    no_sitemap: bool,

    /// Download external CSS/JS (default)
    #[arg(long, overrides_with = "no_allow_external_assets")]
    allow_external_assets: bool,

    /// Leave external CSS/JS as-is instead of downloading
    #[arg(long, overrides_with = "allow_external_assets")]
    no_allow_external_assets: bool,

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

    /// Viewport to render with (only used with --headless)
    #[arg(long, value_enum, default_value = "desktop")]
    device: Device,

    /// Seconds to wait for browser navigation before failing (only used with --headless)
    #[arg(long, default_value_t = 60, value_parser = clap::builder::RangedU64ValueParser::<u64>::new().range(1..))]
    timeout: u64,
}

// Both options default to on; with overrides_with only the last flag given is set.
impl Args {
    fn use_sitemap(&self) -> bool {
        self.sitemap || !self.no_sitemap
    }

    fn use_external_assets(&self) -> bool {
        self.allow_external_assets || !self.no_allow_external_assets
    }
}

struct PromptResult {
    max_depth: u32,
    placeholder: String,
    bot: bool,
    headless: bool,
    // Only read by the headless crawler.
    #[cfg_attr(not(feature = "headless"), allow(dead_code))]
    screenshot: bool,
    concurrency: usize,
    delay_ms: u64,
    sitemap: bool,
    allow_external_assets: bool,
}

/// Parse CLI arguments and run the crawler.
pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Screenshot(args)) = cli.command {
        let input = match (args.url, args.file) {
            (Some(url), None) => screenshot::Input::Url(url),
            (None, Some(file)) => screenshot::Input::File(file),
            _ => unreachable!("clap requires exactly one of URL and --file"),
        };
        return screenshot::run(
            input,
            &args.output,
            args.concurrency,
            args.device,
            Duration::from_secs(args.timeout),
        )
        .await;
    }

    let args = cli.crawl;
    let url = args
        .url
        .clone()
        .expect("clap requires URL without a subcommand");
    Url::parse(&url).map_err(|_| anyhow::anyhow!("Invalid URL provided"))?;

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
            sitemap: args.use_sitemap(),
            allow_external_assets: args.use_external_assets(),
        }
    };

    configure_requests(ConfigureOptions {
        delay_ms: Some(opts.delay_ms),
        bot_mode: opts.bot,
        user_agent: args.user_agent,
        referer: args.referer,
    })?;

    // Resolve the canonical start URL by following any redirects (e.g. www → non-www)
    let canonical = resolve_redirect(&url).await;
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
            opts,
            args.device,
            Duration::from_secs(args.timeout),
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
    opts: PromptResult,
    device: Device,
    timeout: Duration,
) -> Result<()> {
    let chrome = find_chrome().ok_or_else(chrome_not_found)?;

    output::info(&format!("Using browser: {}", chrome.display()));

    #[cfg(feature = "headless")]
    {
        use crate::headless::crawler::{crawl, HeadlessOptions};
        crawl(
            start_url,
            out_dir,
            &chrome,
            HeadlessOptions {
                max_depth: opts.max_depth,
                concurrency: opts.concurrency,
                sitemap: opts.sitemap,
                allow_external_assets: opts.allow_external_assets,
                placeholder: opts.placeholder,
                screenshot: opts.screenshot,
                device,
                request_timeout: timeout,
            },
        )
        .await
    }

    #[cfg(not(feature = "headless"))]
    {
        let _ = (start_url, out_dir, opts, chrome, device, timeout);
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
            "0 – start page only",
            "2 – standard (recommended)",
            "3 – deeper",
            "Enter a custom number …",
        ])
        .default(1)
        .interact()?;

    let max_depth: u32 = match depth_idx {
        0 => 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(flags: &[&str]) -> Args {
        let argv = ["site-scraper", "https://example.com"].iter().chain(flags);
        Cli::try_parse_from(argv).unwrap().crawl
    }

    fn parse_screenshot(argv: &[&str]) -> Result<ScreenshotArgs, clap::Error> {
        let argv = ["site-scraper", "screenshot"].iter().chain(argv);
        match Cli::try_parse_from(argv)?.command {
            Some(Command::Screenshot(args)) => Ok(args),
            None => panic!("expected screenshot subcommand"),
        }
    }

    #[test]
    fn crawl_invocation_without_subcommand_still_works() {
        let cli = Cli::try_parse_from(["site-scraper", "https://example.com", "--max-depth", "1"])
            .unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.crawl.url.as_deref(), Some("https://example.com"));
        assert_eq!(cli.crawl.max_depth, Some(1));
    }

    #[test]
    fn crawl_requires_url() {
        assert!(Cli::try_parse_from(["site-scraper"]).is_err());
    }

    #[test]
    fn screenshot_accepts_url_with_defaults() {
        let args = parse_screenshot(&["https://example.com"]).unwrap();
        assert_eq!(args.url.as_deref(), Some("https://example.com"));
        assert_eq!(args.output, PathBuf::from("screenshots"));
        assert_eq!(args.concurrency, 2);
    }

    #[test]
    fn screenshot_accepts_file_and_output() {
        let args = parse_screenshot(&["--file", "urls.txt", "--output", "shots/"]).unwrap();
        assert_eq!(args.file, Some(PathBuf::from("urls.txt")));
        assert_eq!(args.output, PathBuf::from("shots/"));
    }

    #[test]
    fn screenshot_requires_exactly_one_input() {
        assert!(parse_screenshot(&[]).is_err());
        assert!(parse_screenshot(&["https://example.com", "--file", "urls.txt"]).is_err());
    }

    #[test]
    fn screenshot_rejects_zero_concurrency() {
        assert!(parse_screenshot(&["https://example.com", "--concurrency", "0"]).is_err());
    }

    #[test]
    fn screenshot_device_defaults_to_desktop() {
        let args = parse_screenshot(&["https://example.com"]).unwrap();
        assert_eq!(args.device, Device::Desktop);
        assert_eq!(args.timeout, 60);
    }

    #[test]
    fn screenshot_accepts_device_and_timeout() {
        let args = parse_screenshot(&[
            "https://example.com",
            "--device",
            "mobile",
            "--timeout",
            "90",
        ])
        .unwrap();
        assert_eq!(args.device, Device::Mobile);
        assert_eq!(args.timeout, 90);
    }

    #[test]
    fn screenshot_rejects_zero_timeout() {
        assert!(parse_screenshot(&["https://example.com", "--timeout", "0"]).is_err());
    }

    #[test]
    fn sitemap_and_external_assets_default_to_on() {
        let args = parse(&[]);
        assert!(args.use_sitemap());
        assert!(args.use_external_assets());
    }

    #[test]
    fn crawl_device_and_timeout_default() {
        let args = parse(&[]);
        assert_eq!(args.device, Device::Desktop);
        assert_eq!(args.timeout, 60);
    }

    #[test]
    fn crawl_accepts_device_and_timeout() {
        let args = parse(&["--device", "tablet", "--timeout", "45"]);
        assert_eq!(args.device, Device::Tablet);
        assert_eq!(args.timeout, 45);
    }

    #[test]
    fn no_flags_disable_sitemap_and_external_assets() {
        let args = parse(&["--no-sitemap", "--no-allow-external-assets"]);
        assert!(!args.use_sitemap());
        assert!(!args.use_external_assets());
    }

    #[test]
    fn last_of_positive_and_negative_flag_wins() {
        assert!(parse(&["--no-sitemap", "--sitemap"]).use_sitemap());
        assert!(!parse(&["--sitemap", "--no-sitemap"]).use_sitemap());
        assert!(
            parse(&["--no-allow-external-assets", "--allow-external-assets"]).use_external_assets()
        );
        assert!(
            !parse(&["--allow-external-assets", "--no-allow-external-assets"])
                .use_external_assets()
        );
    }
}
