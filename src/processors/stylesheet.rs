use std::path::Path;

use anyhow::Result;
use regex::Regex;
use url::Url;

use crate::network::fetch::{download_binary, fetch_with_retry};
use crate::utils::filesystem::ensure_dir;
use crate::utils::url::{make_relative, url_to_local_path};

/// Download a stylesheet and rewrite its `url()` assets to local files.
/// Returns the local path where the CSS was saved.
pub async fn process_stylesheet(
    root: &Url,
    css_url: &Url,
    out_dir: &Path,
    allow_external_assets: bool,
) -> Result<String> {
    let resp = fetch_with_retry(css_url.as_str(), 3, 400).await?;
    let css = resp.text().await?;

    let css_path = url_to_local_path(root, css_url, out_dir, None);
    let asset_re = Regex::new(r"url\(([^)]+)\)").unwrap();

    // Collect all assets that need downloading
    let mut download_tasks = Vec::new();
    let rewritten = asset_re
        .replace_all(&css, |caps: &regex::Captures| {
            let raw = caps
                .get(1)
                .map(|m| m.as_str().trim().trim_matches(|c| c == '\'' || c == '"'))
                .unwrap_or("");

            if raw.is_empty() || raw.starts_with("data:") {
                return caps[0].to_string();
            }

            match css_url.join(raw) {
                Ok(asset_url) => {
                    let is_external = asset_url.origin() != root.origin();
                    if is_external && !allow_external_assets {
                        return caps[0].to_string();
                    }
                    let local_path = url_to_local_path(root, &asset_url, out_dir, None);
                    let is_image = local_path
                        .extension()
                        .map(|e| {
                            matches!(
                                e.to_str().unwrap_or(""),
                                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico"
                            )
                        })
                        .unwrap_or(false);
                    download_tasks.push((asset_url.to_string(), local_path.clone(), is_image));
                    let rel = make_relative(&css_path, &local_path);
                    format!("url({})", rel)
                }
                Err(_) => caps[0].to_string(),
            }
        })
        .to_string();

    // Write the rewritten CSS
    if let Some(parent) = css_path.parent() {
        ensure_dir(parent).await?;
    }
    tokio::fs::write(&css_path, &rewritten).await?;

    // Download all referenced assets concurrently
    let mut handles = Vec::new();
    for (url, dest, is_image) in download_tasks {
        handles.push(tokio::spawn(async move {
            download_binary(&url, &dest, is_image).await;
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }

    Ok(css_path.to_string_lossy().to_string())
}
