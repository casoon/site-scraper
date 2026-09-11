use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chromiumoxide::cdp::browser_protocol::target::CreateTargetParams;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use tokio::sync::Semaphore;
use url::Url;

use crate::network::fetch::{is_not_found, HttpStatusError};
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
}

/// Settings shared by all page tasks of one crawl.
struct PageContext {
    root: Url,
    out_dir: PathBuf,
    screenshot_dir: Option<PathBuf>,
    max_depth: u32,
    rewrite_opts: RewriteOptions,
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
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage")
        .build()
        .map_err(|e| anyhow!("Browser config error: {}", e))?;

    let (browser, mut handler) = Browser::launch(config).await?;
    let browser = Arc::new(browser);

    // Drive the browser event loop in the background
    tokio::spawn(async move {
        loop {
            if handler.next().await.is_none() {
                break;
            }
        }
    });

    let root = Url::parse(start_url)?;

    let screenshot_dir = if opts.screenshot {
        let dir = out_dir.join("screenshots");
        tokio::fs::create_dir_all(&dir).await?;
        println!("Screenshots will be saved to {}/", dir.display());
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

    let mut seen: HashSet<String> = HashSet::new();
    let mut failed = 0usize;

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

        for (page_url, handle) in handles {
            match handle.await {
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
                    eprintln!("Warning: {}: {:#} (listed in sitemap)", page_url, e);
                }
                Ok(Err(e)) => {
                    failed += 1;
                    eprintln!("Failed: {}: {:#}", page_url, e);
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("Failed: {}: {}", page_url, e);
                }
            }
        }
    }

    // All page tasks have finished, so this is the last reference.
    if let Ok(mut browser) = Arc::try_unwrap(browser) {
        let _ = browser.close().await;
    }
    if failed > 0 {
        anyhow::bail!("{} of {} pages failed", failed, seen.len());
    }
    Ok(())
}

/// Render one page in its own tab, save it and return the links to follow.
async fn process_page(
    browser: &Browser,
    ctx: &PageContext,
    url: Url,
    depth: u32,
) -> Result<Vec<(Url, u32)>> {
    // Open each page in its own window: with several tabs in one window only
    // the front tab is visible, and hidden tabs get no scroll events, so
    // scroll-driven state (e.g. sticky headers) would be missing.
    let target = CreateTargetParams::builder()
        .url("about:blank")
        .new_window(true)
        .build()
        .map_err(|e| anyhow!(e))?;
    let page = browser.new_page(target).await?;
    let captured = capture_page(&page, &url, ctx.screenshot_dir.as_deref()).await;
    let _ = page.close().await;
    let html = captured?;

    // Save page using existing HTML processor (rewrites assets, links)
    let out_file =
        rewrite_and_save_html(&ctx.root, &url, &html, &ctx.out_dir, &ctx.rewrite_opts).await?;
    let rel = out_file.strip_prefix(&ctx.out_dir).unwrap_or(&out_file);
    println!("Saved: {} -> {}", url, rel.display());

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
    // Navigate via goto() instead of new_page(url): Chrome renders its
    // own error page for failed navigations, and only goto() reports
    // network errors. HTTP error status is read from the Navigation
    // Timing API because chromiumoxide's navigation response is not
    // reliable. Waiting for navigation also waits for the initial load;
    // after it we scroll to trigger IntersectionObserver animations
    // (common in React/Next.js apps that use opacity-0 as initial state).
    page.goto(url.as_str()).await?;
    let _ = page.wait_for_navigation().await;
    let status = page
        .evaluate("performance.getEntriesByType('navigation')[0]?.responseStatus ?? 0")
        .await
        .ok()
        .and_then(|r| r.into_value::<u16>().ok())
        .unwrap_or(0);
    if status >= 400 {
        return Err(HttpStatusError(status).into());
    }

    // Wait for React/Next.js useEffect hooks to mount and attach event
    // listeners (e.g. scroll listeners for sticky headers). The load
    // event fires before these hooks run, so scrolling immediately
    // after wait_for_navigation() misses them.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    // Scroll to bottom: triggers IntersectionObserver animations and
    // scroll-driven style changes (sticky headers, etc.).
    let _ = page
        .evaluate("window.scrollTo({ top: document.body.scrollHeight, behavior: 'instant' })")
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // Freeze DOM into a static-friendly snapshot:
    //   1. Remove all <script> tags so the saved HTML does not re-run
    //      JS when opened locally (Next.js/React hydration would
    //      reset the rendered state and break the page).
    //   2. Reveal opacity-0 + translate-* animation entry states so
    //      elements are visible without JS driving them.
    let _ = page
        .evaluate(
            r#"(() => {
                document.querySelectorAll('script').forEach(s => s.remove());
                document.querySelectorAll('.opacity-0').forEach(el => {
                    const hasTranslate = [...el.classList]
                        .some(c => /^-?translate-[xy]-/.test(c));
                    if (!hasTranslate) return;
                    el.classList.remove('opacity-0');
                    [...el.classList]
                        .filter(c => /^-?translate-[xy]-/.test(c))
                        .forEach(c => el.classList.remove(c));
                });
            })()"#,
        )
        .await;

    // Capture HTML while scrolled — preserves scroll-driven class
    // changes (e.g. header gaining a background). Scroll back to top
    // only for the screenshot so it shows the page from the beginning.
    let html = page.content().await?;

    // Optional screenshot
    if let Some(dir) = screenshot_dir {
        let _ = page
            .evaluate("window.scrollTo({ top: 0, behavior: 'instant' })")
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        take_screenshot(page, url, dir).await;
    }

    Ok(html)
}

async fn take_screenshot(page: &Page, url: &Url, screenshot_dir: &Path) {
    use chromiumoxide::page::ScreenshotParams;

    let params = ScreenshotParams::builder().full_page(true).build();
    match page.screenshot(params).await {
        Ok(data) => {
            let filename = url_to_screenshot_filename(url);
            let dest = screenshot_dir.join(&filename);
            if let Err(e) = tokio::fs::write(&dest, data).await {
                eprintln!("Screenshot write failed for {}: {}", url, e);
            } else {
                println!("Screenshot: {}", dest.display());
            }
        }
        Err(e) => eprintln!("Screenshot failed for {}: {}", url, e),
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
