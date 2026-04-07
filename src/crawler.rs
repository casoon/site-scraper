use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use tokio::sync::Semaphore;
use url::Url;

use crate::network::fetch::fetch_with_retry;
use crate::parsers::links::extract_links;
use crate::parsers::sitemap::discover_from_sitemap;
use crate::processors::html::{rewrite_and_save_html, RewriteOptions};

pub struct CrawlOptions {
    pub max_depth: u32,
    pub concurrency: usize,
    pub sitemap: bool,
    pub allow_external_assets: bool,
    pub placeholder: String,
}

/// Main crawl function.
/// Recursively crawls a website, downloading HTML and assets.
pub async fn crawl(start_url: &str, out_dir: &Path, options: CrawlOptions) -> Result<()> {
    let root = Url::parse(start_url)?;
    let semaphore = std::sync::Arc::new(Semaphore::new(options.concurrency));

    let mut to_visit: Vec<(Url, u32)> = vec![(root.clone(), 0)];

    // Optionally seed from sitemap
    if options.sitemap {
        let seeds = discover_from_sitemap(&root).await;
        for s in seeds {
            if let Ok(url) = Url::parse(&s) {
                if url.origin() == root.origin() {
                    to_visit.push((url, 1));
                }
            }
        }
    }

    let mut seen = HashSet::new();

    while !to_visit.is_empty() {
        let batch: Vec<(Url, u32)> = to_visit.drain(..).collect();
        let mut handles = Vec::new();

        for (url, depth) in batch {
            let key = url.to_string().split('#').next().unwrap_or("").to_string();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let root = root.clone();
            let start_url = start_url.to_string();
            let out_dir = out_dir.to_path_buf();
            let sem = semaphore.clone();
            let placeholder = options.placeholder.clone();
            let allow_ext = options.allow_external_assets;
            let max_depth = options.max_depth;

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                process_page(&root, &start_url, &url, depth, &out_dir, allow_ext, &placeholder, max_depth).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(Ok(new_links)) => {
                    for link in new_links {
                        let lk = link.0.to_string().split('#').next().unwrap_or("").to_string();
                        if !seen.contains(&lk) {
                            to_visit.push(link);
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => {}
            }
        }
    }

    Ok(())
}

async fn process_page(
    _root: &Url,
    start_url: &str,
    url: &Url,
    depth: u32,
    out_dir: &Path,
    allow_external_assets: bool,
    placeholder: &str,
    max_depth: u32,
) -> Result<Vec<(Url, u32)>> {
    let key = url.to_string().split('#').next().unwrap_or("").to_string();

    let resp = fetch_with_retry(&key, 3, 400).await?;
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !ct.contains("text/html") {
        return Ok(Vec::new());
    }
    let html = resp.text().await?;

    let start = Url::parse(start_url)?;
    let opts = RewriteOptions {
        allow_external_assets,
        placeholder: placeholder.to_string(),
    };

    let out_file = rewrite_and_save_html(&start, url, &html, out_dir, &opts).await?;

    // Extract further links
    let mut new_links = Vec::new();
    if depth < max_depth {
        let links = extract_links(&html, &start, url);
        for link in links {
            new_links.push((link, depth + 1));
        }
    }

    let rel_path = out_file
        .strip_prefix(out_dir)
        .unwrap_or(&out_file)
        .to_string_lossy();
    println!("Saved: {} -> {}", url, rel_path);

    Ok(new_links)
}
