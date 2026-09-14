use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chromiumoxide::cdp::browser_protocol::target::CreateTargetParams;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use url::Url;

use super::Device;
use crate::network::fetch::{fetch_with_retry, HttpStatusError};

/// User agent sent when emulating a mobile device: the device metrics
/// override alone doesn't change `navigator.userAgent`, so pages that
/// sniff the UA string would still see a desktop browser.
const MOBILE_USER_AGENT: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";

impl Device {
    fn viewport(self) -> Viewport {
        match self {
            Device::Desktop => Viewport {
                width: 1440,
                height: 900,
                ..Default::default()
            },
            Device::Tablet => Viewport {
                width: 768,
                height: 1024,
                ..Default::default()
            },
            Device::Mobile => Viewport {
                width: 375,
                height: 812,
                device_scale_factor: Some(3.0),
                emulating_mobile: true,
                has_touch: true,
                ..Default::default()
            },
        }
    }
}

/// Launch Chrome/Chromium and drive its event loop in the background.
pub async fn launch_browser(
    chrome_path: &Path,
    device: Device,
    request_timeout: Duration,
) -> Result<Browser> {
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage")
        .viewport(device.viewport())
        .request_timeout(request_timeout)
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
pub async fn open_page(browser: &Browser, device: Device) -> Result<Page> {
    // Open each page in its own window: with several tabs in one window only
    // the front tab is visible, and hidden tabs get no scroll events, so
    // scroll-driven state (e.g. sticky headers) would be missing.
    let target = CreateTargetParams::builder()
        .url("about:blank")
        .new_window(true)
        .build()
        .map_err(|e| anyhow!(e))?;
    let page = browser.new_page(target).await?;
    if device == Device::Mobile {
        let _ = page.set_user_agent(MOBILE_USER_AGENT).await;
    }
    Ok(page)
}

/// Navigate to `url`, let it render and freeze the DOM into a static
/// snapshot. Returns the HTTP status of the navigation response; callers
/// decide whether a 4xx/5xx status should fail the page.
pub async fn prepare_page(page: &Page, url: &Url) -> Result<u16> {
    // Navigate via goto() instead of new_page(url): Chrome renders its
    // own error page for failed navigations, and only goto() reports
    // network errors. HTTP error status is read from the Navigation
    // Timing API because chromiumoxide's navigation response is not
    // reliable. Waiting for navigation also waits for the initial load;
    // after it we scroll to trigger IntersectionObserver animations
    // (common in React/Next.js apps that use opacity-0 as initial state).
    if let Err(e) = page.goto(url.as_str()).await {
        // Chrome reports an HTTP error status with an empty body as a network
        // error. Recover the real status so callers handle it like any other
        // 4xx/5xx (e.g. a stale sitemap entry only warns).
        if e.to_string().contains("ERR_HTTP_RESPONSE_CODE_FAILURE") {
            if let Err(status_err) = fetch_with_retry(url.as_str(), 1, 0).await {
                if status_err.is::<HttpStatusError>() {
                    return Err(status_err);
                }
            }
        }
        return Err(e.into());
    }
    let _ = page.wait_for_navigation().await;
    let status = page
        .evaluate("performance.getEntriesByType('navigation')[0]?.responseStatus ?? 0")
        .await
        .ok()
        .and_then(|r| r.into_value::<u16>().ok())
        .unwrap_or(0);

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

    Ok(status)
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
