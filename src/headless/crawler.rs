use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chromiumoxide::{Browser, Page};
use tokio::sync::Semaphore;
use url::Url;

use super::browser::{full_page_png, launch_browser, open_page, prepare_page};
use super::Device;
use crate::network::fetch::{is_not_found, HttpStatusError};
use crate::output;
use crate::parsers::links::extract_links;
use crate::parsers::sitemap::discover_from_sitemap;
use crate::processors::html::{rewrite_and_save_html, RewriteOptions};

pub struct HeadlessOptions {
    pub max_depth: u32,
    pub concurrency: usize,
    pub sitemap: bool,
    pub allow_external_assets: bool,
    pub placeholder: String,
    pub screenshot: bool,
    pub device: Device,
    pub request_timeout: Duration,
}

/// Settings shared by all page tasks of one crawl.
struct PageContext {
    root: Url,
    out_dir: PathBuf,
    screenshot_dir: Option<PathBuf>,
    max_depth: u32,
    rewrite_opts: RewriteOptions,
    device: Device,
}

fn strip_fragment(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

pub async fn crawl(
    start_url: &str,
    out_dir: &Path,
    chrome_path: &Path,
    opts: HeadlessOptions,
) -> Result<()> {
    let started = Instant::now();
    let browser = Arc::new(launch_browser(chrome_path, opts.device, opts.request_timeout).await?);

    let root = Url::parse(start_url)?;

    let screenshot_dir = if opts.screenshot {
        let dir = out_dir.join("screenshots");
        tokio::fs::create_dir_all(&dir).await?;
        output::info(&format!("Screenshots will be saved to {}/", dir.display()));
        Some(dir)
    } else {
        None
    };

    let ctx = Arc::new(PageContext {
        root: root.clone(),
        out_dir: out_dir.to_path_buf(),
        screenshot_dir,
        max_depth: opts.max_depth,
        rewrite_opts: RewriteOptions {
            allow_external_assets: opts.allow_external_assets,
            placeholder: opts.placeholder,
        },
        device: opts.device,
    });
    // Each permit is one open browser tab.
    let semaphore = Arc::new(Semaphore::new(opts.concurrency));

    let mut to_visit: Vec<(Url, u32)> = vec![(root.clone(), 0)];
    let mut sitemap_urls = HashSet::new();

    // Optionally seed from sitemap; entries count as linked pages (depth 1),
    // so skip them when only the start page is requested.
    if opts.sitemap && opts.max_depth >= 1 {
        for s in discover_from_sitemap(&root).await {
            if let Ok(url) = Url::parse(&s) {
                if url.origin() == root.origin() {
                    sitemap_urls.insert(strip_fragment(&url));
                    to_visit.push((url, 1));
                }
            }
        }
    }

    output::info(&match sitemap_urls.len() {
        0 => format!("Crawling {}", root),
        n => format!("Crawling {} ({} URLs from sitemap)", root, n),
    });

    let mut seen: HashSet<String> = HashSet::new();
    let mut failed = 0usize;
    let mut warnings = 0usize;
    let mut done = 0usize;

    while !to_visit.is_empty() {
        let batch = std::mem::take(&mut to_visit);
        let mut handles = Vec::new();

        for (url, depth) in batch {
            let key = strip_fragment(&url);
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let sem = semaphore.clone();
            let browser = browser.clone();
            let ctx = ctx.clone();
            let page_url = url.clone();
            handles.push((
                page_url,
                tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();
                    process_page(&browser, &ctx, url, depth).await
                }),
            ));
        }

        output::progress_grow(seen.len(), "Crawling");
        for (page_url, handle) in handles {
            let result = handle.await;
            done += 1;
            output::progress_advance(done, page_url.as_str());
            match result {
                Ok(Ok(new_links)) => {
                    for link in new_links {
                        if !seen.contains(&strip_fragment(&link.0)) {
                            to_visit.push(link);
                        }
                    }
                }
                // Stale sitemap entries are common; don't fail the crawl for them.
                Ok(Err(e))
                    if is_not_found(&e) && sitemap_urls.contains(&strip_fragment(&page_url)) =>
                {
                    warnings += 1;
                    output::warning(&format!("{}: {:#} (listed in sitemap)", page_url, e));
                }
                Ok(Err(e)) => {
                    failed += 1;
                    output::failure(&format!("{}: {:#}", page_url, e));
                }
                Err(e) => {
                    failed += 1;
                    output::failure(&format!("{}: {}", page_url, e));
                }
            }
        }
    }

    // All page tasks have finished, so this is the last reference.
    if let Ok(mut browser) = Arc::try_unwrap(browser) {
        let _ = browser.close().await;
    }
    output::crawl_summary(start_url, out_dir, seen.len(), failed, warnings, started)
}

/// Render one page in its own tab, save it and return the links to follow.
async fn process_page(
    browser: &Browser,
    ctx: &PageContext,
    url: Url,
    depth: u32,
) -> Result<Vec<(Url, u32)>> {
    let page = open_page(browser, ctx.device).await?;
    let captured = capture_page(&page, &url, ctx.screenshot_dir.as_deref()).await;
    let _ = page.close().await;
    let html = captured?;

    // Save page using existing HTML processor (rewrites assets, links)
    let out_file =
        rewrite_and_save_html(&ctx.root, &url, &html, &ctx.out_dir, &ctx.rewrite_opts).await?;
    let rel = out_file.strip_prefix(&ctx.out_dir).unwrap_or(&out_file);
    output::success(&format!("{} → {}", url, rel.display()));

    // Follow links up to max_depth
    let mut new_links = Vec::new();
    if depth < ctx.max_depth {
        for link in extract_links(&html, &ctx.root, &url) {
            new_links.push((link, depth + 1));
        }
    }
    Ok(new_links)
}

/// Navigate to `url`, freeze the rendered DOM and return its HTML.
async fn capture_page(page: &Page, url: &Url, screenshot_dir: Option<&Path>) -> Result<String> {
    let status = prepare_page(page, url).await?;
    if status >= 400 {
        return Err(HttpStatusError(status).into());
    }

    // Capture HTML while scrolled — preserves scroll-driven class
    // changes (e.g. header gaining a background). The screenshot scrolls
    // back to top so it shows the page from the beginning.
    let html = page.content().await?;

    // Optional screenshot
    if let Some(dir) = screenshot_dir {
        take_screenshot(page, url, dir).await;
    }

    Ok(html)
}

async fn take_screenshot(page: &Page, url: &Url, screenshot_dir: &Path) {
    match full_page_png(page).await {
        Ok(data) => {
            let filename = url_to_screenshot_filename(url);
            let dest = screenshot_dir.join(&filename);
            if let Err(e) = tokio::fs::write(&dest, data).await {
                output::warning(&format!("Screenshot write failed for {}: {}", url, e));
            } else {
                output::success(&format!("Screenshot {} → {}", url, dest.display()));
            }
        }
        Err(e) => output::warning(&format!("Screenshot failed for {}: {}", url, e)),
    }
}

fn url_to_screenshot_filename(url: &Url) -> String {
    let path = url.path().trim_matches('/').replace('/', "_");
    let base = if path.is_empty() {
        "index".to_string()
    } else {
        path
    };
    if let Some(q) = url.query() {
        let q_slug: String = q
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        format!("{}-{}.png", base, q_slug)
    } else {
        format!("{}.png", base)
    }
}
