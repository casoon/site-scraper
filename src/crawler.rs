use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use tokio::sync::Semaphore;
use url::Url;

use crate::network::fetch::{fetch_with_retry, is_not_found};
use crate::output;
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

struct PageParams {
    start_url: String,
    url: Url,
    depth: u32,
    out_dir: std::path::PathBuf,
    allow_external_assets: bool,
    placeholder: String,
    max_depth: u32,
}

fn strip_fragment(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

/// Main crawl function.
/// Recursively crawls a website, downloading HTML and assets.
pub async fn crawl(start_url: &str, out_dir: &Path, options: CrawlOptions) -> Result<()> {
    let started = Instant::now();
    let root = Url::parse(start_url)?;
    let semaphore = std::sync::Arc::new(Semaphore::new(options.concurrency));

    let mut to_visit: Vec<(Url, u32)> = vec![(root.clone(), 0)];
    let mut sitemap_urls = HashSet::new();

    // Optionally seed from sitemap; entries count as linked pages (depth 1),
    // so skip them when only the start page is requested.
    if options.sitemap && options.max_depth >= 1 {
        let seeds = discover_from_sitemap(&root).await;
        for s in seeds {
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

    let mut seen = HashSet::new();
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
            let page_url = url.clone();
            let params = PageParams {
                start_url: start_url.to_string(),
                url,
                depth,
                out_dir: out_dir.to_path_buf(),
                allow_external_assets: options.allow_external_assets,
                placeholder: options.placeholder.clone(),
                max_depth: options.max_depth,
            };

            handles.push((
                page_url,
                tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();
                    process_page(params).await
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
                        let lk = strip_fragment(&link.0);
                        if !seen.contains(&lk) {
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

    output::crawl_summary(start_url, out_dir, seen.len(), failed, warnings, started)
}

async fn process_page(params: PageParams) -> Result<Vec<(Url, u32)>> {
    let key = strip_fragment(&params.url);

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

    let start = Url::parse(&params.start_url)?;
    let opts = RewriteOptions {
        allow_external_assets: params.allow_external_assets,
        placeholder: params.placeholder,
    };

    let out_file =
        rewrite_and_save_html(&start, &params.url, &html, &params.out_dir, &opts).await?;

    // Extract further links
    let mut new_links = Vec::new();
    if params.depth < params.max_depth {
        let links = extract_links(&html, &start, &params.url);
        for link in links {
            new_links.push((link, params.depth + 1));
        }
    }

    let rel_path = out_file
        .strip_prefix(&params.out_dir)
        .unwrap_or(&out_file)
        .to_string_lossy();
    output::success(&format!("{} → {}", params.url, rel_path));

    Ok(new_links)
}
