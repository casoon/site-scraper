use std::path::Path;

use anyhow::{anyhow, Result};
use chromiumoxide::cdp::browser_protocol::target::CreateTargetParams;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use url::Url;

use crate::network::fetch::HttpStatusError;

/// Launch Chrome/Chromium and drive its event loop in the background.
pub async fn launch_browser(chrome_path: &Path) -> Result<Browser> {
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage")
        .build()
        .map_err(|e| anyhow!("Browser config error: {}", e))?;

    let (browser, mut handler) = Browser::launch(config).await?;

    // Drive the browser event loop in the background
    tokio::spawn(async move {
        loop {
            if handler.next().await.is_none() {
                break;
            }
        }
    });

    Ok(browser)
}

/// Open a blank page in its own window.
pub async fn open_page(browser: &Browser) -> Result<Page> {
    // Open each page in its own window: with several tabs in one window only
    // the front tab is visible, and hidden tabs get no scroll events, so
    // scroll-driven state (e.g. sticky headers) would be missing.
    let target = CreateTargetParams::builder()
        .url("about:blank")
        .new_window(true)
        .build()
        .map_err(|e| anyhow!(e))?;
    Ok(browser.new_page(target).await?)
}

/// Navigate to `url`, let it render and freeze the DOM into a static snapshot.
pub async fn prepare_page(page: &Page, url: &Url) -> Result<()> {
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

    Ok(())
}

/// Scroll back to top and take a full-page PNG screenshot.
pub async fn full_page_png(page: &Page) -> Result<Vec<u8>> {
    let _ = page
        .evaluate("window.scrollTo({ top: 0, behavior: 'instant' })")
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let params = ScreenshotParams::builder().full_page(true).build();
    Ok(page.screenshot(params).await?)
}
