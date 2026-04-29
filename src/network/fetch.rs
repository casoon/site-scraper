use std::path::Path;
use std::sync::RwLock;
use std::time::Duration;

use anyhow::{anyhow, Result};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Response};

use crate::utils::filesystem::ensure_dir;

const BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const BOT_USER_AGENT: &str = "site-scraper/1.2 (+https://github.com/casoon/site-scraper)";

struct RequestConfig {
    delay_ms: u64,
    bot_mode: bool,
    user_agent: String,
    referer: Option<String>,
    client: Client,
}

static CONFIG: RwLock<Option<RequestConfig>> = RwLock::new(None);

fn browser_headers(user_agent: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let h = |s: &str| HeaderValue::from_str(s).unwrap();

    headers.insert("user-agent", h(user_agent));
    headers.insert("accept", h("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"));
    headers.insert("accept-language", h("en-US,en;q=0.9,de;q=0.8"));
    headers.insert("accept-encoding", h("gzip, deflate, br"));
    headers.insert("cache-control", h("no-cache"));
    headers.insert("pragma", h("no-cache"));

    let chrome_ver = user_agent
        .find("Chrome/")
        .and_then(|i| user_agent[i + 7..].split('.').next())
        .unwrap_or("131");
    let sec_ch_ua = format!(
        "\"Google Chrome\";v=\"{}\", \"Chromium\";v=\"{}\", \"Not_A Brand\";v=\"24\"",
        chrome_ver, chrome_ver
    );
    headers.insert("sec-ch-ua", h(&sec_ch_ua));
    headers.insert("sec-ch-ua-mobile", h("?0"));
    headers.insert("sec-ch-ua-platform", h("\"macOS\""));
    headers.insert("sec-fetch-dest", h("document"));
    headers.insert("sec-fetch-mode", h("navigate"));
    headers.insert("sec-fetch-site", h("none"));
    headers.insert("sec-fetch-user", h("?1"));
    headers.insert("upgrade-insecure-requests", h("1"));
    headers.insert("connection", h("keep-alive"));

    headers
}

fn bot_headers(user_agent: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let h = |s: &str| HeaderValue::from_str(s).unwrap();

    headers.insert("user-agent", h(user_agent));
    headers.insert(
        "accept",
        h("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert("accept-language", h("en-US,en;q=0.9"));
    headers.insert("accept-encoding", h("gzip, deflate, br"));

    headers
}

fn build_client(user_agent: &str, bot_mode: bool) -> Client {
    let headers = if bot_mode {
        bot_headers(user_agent)
    } else {
        browser_headers(user_agent)
    };

    Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client")
}

pub struct ConfigureOptions {
    pub delay_ms: Option<u64>,
    pub bot_mode: bool,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
}

/// Configure request settings.
pub fn configure_requests(opts: ConfigureOptions) {
    let mut lock = CONFIG.write().unwrap();
    let existing = lock.take();

    let mut delay_ms = existing.as_ref().map(|c| c.delay_ms).unwrap_or(0);
    let bot_mode = opts.bot_mode;
    let default_ua = if bot_mode {
        BOT_USER_AGENT
    } else {
        BROWSER_USER_AGENT
    };
    let has_custom_ua = opts.user_agent.is_some();
    let mut user_agent = opts
        .user_agent
        .or_else(|| existing.map(|c| c.user_agent))
        .unwrap_or_else(|| default_ua.to_string());

    if !has_custom_ua {
        user_agent = default_ua.to_string();
    }

    let referer = opts.referer;

    if let Some(d) = opts.delay_ms {
        delay_ms = d;
    }

    let client = build_client(&user_agent, bot_mode);
    *lock = Some(RequestConfig {
        delay_ms,
        bot_mode,
        user_agent,
        referer,
        client,
    });
}

fn get_delay_ms() -> u64 {
    CONFIG
        .read()
        .unwrap()
        .as_ref()
        .map(|c| c.delay_ms)
        .unwrap_or(0)
}

fn get_client() -> Client {
    CONFIG
        .read()
        .unwrap()
        .as_ref()
        .map(|c| c.client.clone())
        .unwrap_or_else(|| build_client(BROWSER_USER_AGENT, false))
}

fn build_request_headers(url: &str) -> HeaderMap {
    let lock = CONFIG.read().unwrap();
    let mut headers = HeaderMap::new();
    let h = |s: &str| HeaderValue::from_str(s).unwrap();

    let is_bot = lock.as_ref().map(|c| c.bot_mode).unwrap_or(false);

    if is_bot {
        // Bots don't need Referer/Origin spoofing
        return headers;
    }

    if let Some(cfg) = lock.as_ref() {
        if let Some(ref base_ref) = cfg.referer {
            headers.insert("referer", h(base_ref));
            if let Ok(u) = url::Url::parse(base_ref) {
                headers.insert("origin", h(u.origin().ascii_serialization().as_str()));
            }
            headers.insert("sec-fetch-site", h("same-origin"));
        } else if let Ok(u) = url::Url::parse(url) {
            let origin = u.origin().ascii_serialization();
            headers.insert("referer", h(&format!("{}/", origin)));
            headers.insert("origin", h(&origin));
        }
    } else if let Ok(u) = url::Url::parse(url) {
        let origin = u.origin().ascii_serialization();
        headers.insert("referer", h(&format!("{}/", origin)));
        headers.insert("origin", h(&origin));
    }

    headers
}

async fn apply_delay() {
    let delay_ms = get_delay_ms();
    if delay_ms > 0 {
        let jitter = rand::thread_rng().gen_range(0..std::cmp::max(1, delay_ms / 5));
        tokio::time::sleep(Duration::from_millis(delay_ms + jitter)).await;
    }
}

/// Fetch with basic retry and exponential backoff.
pub async fn fetch_with_retry(url: &str, tries: u32, backoff_ms: u64) -> Result<Response> {
    let client = get_client();
    let headers = build_request_headers(url);
    let mut last_err: Option<anyhow::Error> = None;

    for i in 0..tries {
        apply_delay().await;
        match client.get(url).headers(headers.clone()).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    last_err = Some(anyhow!("HTTP {}", resp.status().as_u16()));
                    if i < tries - 1 {
                        tokio::time::sleep(Duration::from_millis(backoff_ms * 2u64.pow(i))).await;
                    }
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                last_err = Some(e.into());
                if i < tries - 1 {
                    tokio::time::sleep(Duration::from_millis(backoff_ms * 2u64.pow(i))).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("fetch failed")))
}

/// Follow redirects and return the final URL (e.g. www → non-www).
/// Falls back to the original URL on error.
pub async fn resolve_redirect(url: &str) -> String {
    let client = get_client();
    match client.get(url).send().await {
        Ok(resp) => resp.url().to_string(),
        Err(_) => url.to_string(),
    }
}

/// Download binary asset to a local file.
/// Returns true if successful, false otherwise.
pub async fn download_binary(url: &str, dest: &Path, silent: bool) -> bool {
    let result: Result<()> = async {
        if let Some(parent) = dest.parent() {
            ensure_dir(parent).await?;
        }
        let resp = fetch_with_retry(url, 3, 400).await?;
        let bytes = resp.bytes().await?;
        tokio::fs::write(dest, &bytes).await?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => true,
        Err(e) => {
            if !silent {
                eprintln!("Skipping asset {}: {}", url, e);
            }
            false
        }
    }
}
