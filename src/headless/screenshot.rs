use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chromiumoxide::Browser;
use tokio::sync::Semaphore;
use url::Url;

use super::browser::{full_page_png, launch_browser, open_page, prepare_page};
use super::Device;
use crate::output;
use crate::screenshot::ScreenshotJob;

/// Take all screenshots with at most `concurrency` pages open at once.
pub async fn capture_all(
    chrome_path: &Path,
    jobs: Vec<ScreenshotJob>,
    out_dir: &Path,
    concurrency: usize,
    device: Device,
    request_timeout: Duration,
) -> Result<()> {
    let started = Instant::now();
    let browser = Arc::new(launch_browser(chrome_path, device, request_timeout).await?);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let total = jobs.len();
    output::progress_start(total, "Taking screenshots");

    let mut handles = Vec::new();
    for job in jobs {
        let sem = semaphore.clone();
        let browser = browser.clone();
        let url = job.url.clone();
        handles.push((
            url,
            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                capture_one(&browser, &job.url, &job.dest, device).await
            }),
        ));
    }

    let mut failed = 0usize;
    for (done, (url, handle)) in handles.into_iter().enumerate() {
        let result = handle.await;
        output::progress_advance(done + 1, url.as_str());
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                failed += 1;
                output::failure(&format!("{}: {:#}", url, e));
            }
            Err(e) => {
                failed += 1;
                output::failure(&format!("{}: {}", url, e));
            }
        }
    }

    // All tasks have finished, so this is the last reference.
    if let Ok(mut browser) = Arc::try_unwrap(browser) {
        let _ = browser.close().await;
    }
    output::screenshot_summary(out_dir, total, failed, started)
}

async fn capture_one(browser: &Browser, url: &Url, dest: &Path, device: Device) -> Result<()> {
    let page = open_page(browser, device).await?;
    let captured = async {
        // Screenshot mode captures a page regardless of its HTTP status (e.g.
        // a custom 404), unlike crawl/mirror mode which fails on 4xx/5xx.
        let status = prepare_page(&page, url).await?;
        let png = full_page_png(&page).await?;
        Ok::<_, anyhow::Error>((status, png))
    }
    .await;
    let _ = page.close().await;

    let (status, png) = captured?;
    write_atomic(dest, &png).await?;
    if status >= 400 {
        output::success(&format!("{} → {} (HTTP {})", url, dest.display(), status));
    } else {
        output::success(&format!("{} → {}", url, dest.display()));
    }
    Ok(())
}

/// Write via a temporary file and rename, so an existing screenshot is only
/// replaced by a complete new one.
async fn write_atomic(dest: &Path, data: &[u8]) -> Result<()> {
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dest.with_file_name(format!(".{}.tmp", name));
    tokio::fs::write(&tmp, data).await?;
    if let Err(e) = tokio::fs::rename(&tmp, dest).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e.into());
    }
    Ok(())
}
