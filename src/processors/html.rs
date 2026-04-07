use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use lol_html::{element, rewrite_str, RewriteStrSettings};
use scraper::{Html, Selector};
use url::Url;

use crate::network::fetch::download_binary;
use crate::utils::filesystem::ensure_dir;
use crate::utils::url::{make_relative, url_to_local_path};

use super::image::get_placeholder_for_image;
use super::stylesheet::process_stylesheet;

pub struct RewriteOptions {
    pub allow_external_assets: bool,
    pub placeholder: String,
}

/// Rewrite HTML: links, styles, scripts, images.
/// Downloads assets and rewrites all references to local relative paths.
/// Returns the output file path.
pub async fn rewrite_and_save_html(
    root: &Url,
    page_url: &Url,
    html: &str,
    out_dir: &Path,
    opts: &RewriteOptions,
) -> Result<PathBuf> {
    let page_path = url_to_local_path(root, page_url, out_dir, None);

    // Pass 1: Parse HTML and collect all URLs (synchronous, non-Send Html stays in this block)
    let (css_urls, js_urls, img_urls, link_replacements) = {
        let document = Html::parse_document(html);

        let link_sel = Selector::parse("link[rel='stylesheet'][href], link[rel=\"stylesheet\"][href]").unwrap();
        let mut css_urls: Vec<(String, Url)> = Vec::new();
        for el in document.select(&link_sel) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(url) = page_url.join(href) {
                    let is_external = url.origin() != root.origin();
                    if is_external && !opts.allow_external_assets {
                        continue;
                    }
                    css_urls.push((href.to_string(), url));
                }
            }
        }

        let script_sel = Selector::parse("script[src]").unwrap();
        let mut js_urls: Vec<(String, Url)> = Vec::new();
        for el in document.select(&script_sel) {
            if let Some(src) = el.value().attr("src") {
                if let Ok(url) = page_url.join(src) {
                    let is_external = url.origin() != root.origin();
                    if is_external && !opts.allow_external_assets {
                        continue;
                    }
                    js_urls.push((src.to_string(), url));
                }
            }
        }

        let img_sel = Selector::parse("img[src]").unwrap();
        let mut img_urls: Vec<(String, Url)> = Vec::new();
        for el in document.select(&img_sel) {
            if let Some(src) = el.value().attr("src") {
                if let Ok(url) = page_url.join(src) {
                    img_urls.push((src.to_string(), url));
                }
            }
        }

        let a_sel = Selector::parse("a[href]").unwrap();
        let mut link_replacements: HashMap<String, String> = HashMap::new();
        for el in document.select(&a_sel) {
            if let Some(href) = el.value().attr("href") {
                let href = href.trim();
                if let Ok(url) = page_url.join(href) {
                    if url.origin() == root.origin() {
                        let target_path = url_to_local_path(root, &url, out_dir, None);
                        let rel = make_relative(&page_path, &target_path);
                        link_replacements.insert(href.to_string(), rel);
                    }
                }
            }
        }

        (css_urls, js_urls, img_urls, link_replacements)
    }; // document is dropped here — before any .await

    // Process all assets concurrently
    let mut replacements: HashMap<String, String> = HashMap::new();
    let mut img_replacements: HashMap<String, (String, u32, u32)> = HashMap::new();

    let mut handles: Vec<tokio::task::JoinHandle<(String, String)>> = Vec::new();

    // Process stylesheets
    for (orig_href, url) in css_urls {
        let root = root.clone();
        let out_dir = out_dir.to_path_buf();
        let page_path = page_path.clone();
        let allow_ext = opts.allow_external_assets;
        let orig = orig_href.clone();
        handles.push(tokio::spawn(async move {
            match process_stylesheet(&root, &url, &out_dir, allow_ext).await {
                Ok(css_path) => {
                    let rel = make_relative(&page_path, Path::new(&css_path));
                    (orig, rel)
                }
                Err(_) => (orig, String::new()),
            }
        }));
    }

    // Process scripts
    for (orig_src, url) in js_urls {
        let root = root.clone();
        let out_dir = out_dir.to_path_buf();
        let page_path = page_path.clone();
        let orig = orig_src.clone();
        handles.push(tokio::spawn(async move {
            let js_path = url_to_local_path(&root, &url, &out_dir, Some(".js"));
            if download_binary(url.as_str(), &js_path, false).await {
                let rel = make_relative(&page_path, &js_path);
                (orig, rel)
            } else {
                (orig, String::new())
            }
        }));
    }

    // Process images
    let mut img_handles: Vec<tokio::task::JoinHandle<(String, String, u32, u32)>> = Vec::new();
    for (orig_src, url) in img_urls {
        let root = root.clone();
        let out_dir = out_dir.to_path_buf();
        let page_path = page_path.clone();
        let placeholder = opts.placeholder.clone();
        let orig = orig_src.clone();
        img_handles.push(tokio::spawn(async move {
            let ph = get_placeholder_for_image(&url, &placeholder, &out_dir, &root).await;
            let src = if placeholder == "local" && !ph.src.starts_with("http") {
                make_relative(&page_path, Path::new(&ph.src))
            } else {
                ph.src
            };
            (orig, src, ph.width, ph.height)
        }));
    }

    // Collect results
    for handle in handles {
        if let Ok((orig, new_path)) = handle.await {
            if !new_path.is_empty() {
                replacements.insert(orig, new_path);
            }
        }
    }
    for handle in img_handles {
        if let Ok((orig, src, w, h)) = handle.await {
            img_replacements.insert(orig, (src, w, h));
        }
    }

    // Pass 2: Rewrite HTML using lol_html
    let replacements_css = replacements.clone();
    let replacements_js = replacements.clone();
    let img_repls = img_replacements.clone();
    let link_repls = link_replacements.clone();

    let output = rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("link[rel='stylesheet'][href], link[rel=\"stylesheet\"][href]", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        if let Some(new_href) = replacements_css.get(&href) {
                            el.set_attribute("href", new_href)?;
                        }
                    }
                    Ok(())
                }),
                element!("script[src]", |el| {
                    if let Some(src) = el.get_attribute("src") {
                        if let Some(new_src) = replacements_js.get(&src) {
                            el.set_attribute("src", new_src)?;
                        }
                    }
                    Ok(())
                }),
                element!("img[src]", |el| {
                    if let Some(src) = el.get_attribute("src") {
                        if let Some((new_src, w, h)) = img_repls.get(&src) {
                            el.set_attribute("src", new_src)?;
                            if el.get_attribute("width").is_none() {
                                el.set_attribute("width", &w.to_string())?;
                            }
                            if el.get_attribute("height").is_none() {
                                el.set_attribute("height", &h.to_string())?;
                            }
                            el.remove_attribute("srcset");
                        }
                    }
                    Ok(())
                }),
                element!("a[href]", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        let href_trimmed = href.trim().to_string();
                        if let Some(new_href) = link_repls.get(&href_trimmed) {
                            el.set_attribute("href", new_href)?;
                        }
                    }
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("HTML rewrite error: {:?}", e))?;

    // Write output file
    if let Some(parent) = page_path.parent() {
        ensure_dir(parent).await?;
    }
    tokio::fs::write(&page_path, &output).await?;

    Ok(page_path)
}
