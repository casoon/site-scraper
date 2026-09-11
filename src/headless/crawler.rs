use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use url::Url;

use crate::parsers::links::extract_links;
use crate::processors::html::{rewrite_and_save_html, RewriteOptions};

pub struct HeadlessOptions {
    pub max_depth: u32,
    pub placeholder: String,
    pub screenshot: bool,
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
        .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;

    // Drive the browser event loop in the background
    tokio::spawn(async move {
        loop {
            if handler.next().await.is_none() {
                break;
            }
        }
    });

    let root = Url::parse(start_url)?;
    let screenshot_dir = out_dir.join("screenshots");

    if opts.screenshot {
        tokio::fs::create_dir_all(&screenshot_dir).await?;
        println!("Screenshots will be saved to {}/", screenshot_dir.display());
    }

    let mut to_visit: Vec<(Url, u32)> = vec![(root.clone(), 0)];
    let mut seen: HashSet<String> = HashSet::new();
    let mut failed = 0usize;

    while !to_visit.is_empty() {
        let batch = std::mem::take(&mut to_visit);

        for (url, depth) in batch {
            let key = {
                let mut u = url.clone();
                u.set_fragment(None);
                u.to_string()
            };
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let page = match browser.new_page("about:blank").await {
                Ok(p) => p,
                Err(e) => {
                    failed += 1;
                    eprintln!("Failed: {}: {}", url, e);
                    continue;
                }
            };

            // Navigate via goto() instead of new_page(url): Chrome renders its
            // own error page for failed navigations, and only goto() reports
            // network errors. HTTP error status is read from the Navigation
            // Timing API because chromiumoxide's navigation response is not
            // reliable. Waiting for navigation also waits for the initial load;
            // after it we scroll to trigger IntersectionObserver animations
            // (common in React/Next.js apps that use opacity-0 as initial state).
            let nav_err = match page.goto(url.as_str()).await {
                Err(e) => Some(e.to_string()),
                Ok(_) => {
                    let _ = page.wait_for_navigation().await;
                    let status = page
                        .evaluate(
                            "performance.getEntriesByType('navigation')[0]?.responseStatus ?? 0",
                        )
                        .await
                        .ok()
                        .and_then(|r| r.into_value::<u16>().ok())
                        .unwrap_or(0);
                    (status >= 400).then(|| format!("HTTP {}", status))
                }
            };
            if let Some(err) = nav_err {
                failed += 1;
                eprintln!("Failed: {}: {}", url, err);
                let _ = page.close().await;
                continue;
            }
            // Wait for React/Next.js useEffect hooks to mount and attach event
            // listeners (e.g. scroll listeners for sticky headers). The load
            // event fires before these hooks run, so scrolling immediately
            // after wait_for_navigation() misses them.
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            // Scroll to bottom: triggers IntersectionObserver animations and
            // scroll-driven style changes (sticky headers, etc.).
            let _ = page
                .evaluate(
                    "window.scrollTo({ top: document.body.scrollHeight, behavior: 'instant' })",
                )
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
            let html = match page.content().await {
                Ok(h) => h,
                Err(e) => {
                    failed += 1;
                    eprintln!("Failed: {}: {}", url, e);
                    let _ = page.close().await;
                    continue;
                }
            };

            // Optional screenshot
            if opts.screenshot {
                let _ = page
                    .evaluate("window.scrollTo({ top: 0, behavior: 'instant' })")
                    .await;
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                take_screenshot(&page, &url, &screenshot_dir).await;
            }

            let _ = page.close().await;

            // Save page using existing HTML processor (rewrites assets, links)
            let rewrite_opts = RewriteOptions {
                allow_external_assets: true,
                placeholder: opts.placeholder.clone(),
            };

            match rewrite_and_save_html(&root, &url, &html, out_dir, &rewrite_opts).await {
                Ok(out_file) => {
                    let rel = out_file.strip_prefix(out_dir).unwrap_or(&out_file);
                    println!("Saved: {} -> {}", url, rel.display());
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("Failed: {}: {:#}", url, e);
                }
            }

            // Follow links up to max_depth
            if depth < opts.max_depth {
                for link in extract_links(&html, &root, &url) {
                    let mut lk = link.clone();
                    lk.set_fragment(None);
                    if !seen.contains(lk.as_str()) {
                        to_visit.push((link, depth + 1));
                    }
                }
            }
        }
    }

    let _ = browser.close().await;
    if failed > 0 {
        anyhow::bail!("{} of {} pages failed", failed, seen.len());
    }
    Ok(())
}

async fn take_screenshot(page: &chromiumoxide::Page, url: &Url, screenshot_dir: &Path) {
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
