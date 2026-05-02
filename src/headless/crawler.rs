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

            let page = match browser.new_page(url.as_str()).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping {}: {}", url, e);
                    continue;
                }
            };

            // Wait for initial load, then scroll to trigger IntersectionObserver
            // animations (common in React/Next.js apps that use opacity-0 as
            // initial state) and wait for them to complete before capturing.
            let _ = page.wait_for_navigation().await;
            let _ = page
                .evaluate(
                    "window.scrollTo({ top: document.body.scrollHeight, behavior: 'instant' })",
                )
                .await;
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let _ = page
                .evaluate("window.scrollTo({ top: 0, behavior: 'instant' })")
                .await;

            // Reveal scroll-animation initial states (opacity-0 + translate-*)
            // so the saved HTML renders correctly as a static file without JS.
            // Only touch elements that have BOTH opacity-0 AND a translate
            // class — those are animation entry states. Elements with just
            // opacity-0 (mobile menus, modals, overlays) are intentionally
            // hidden and must not be revealed.
            let _ = page
                .evaluate(
                    r#"(() => {
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

            let html = match page.content().await {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Failed to get content of {}: {}", url, e);
                    let _ = page.close().await;
                    continue;
                }
            };

            // Optional screenshot
            if opts.screenshot {
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
                Err(e) => eprintln!("Failed to save {}: {}", url, e),
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
