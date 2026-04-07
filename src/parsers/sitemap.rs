use regex::Regex;
use url::Url;

use crate::network::fetch::fetch_with_retry;

/// Discover URLs from sitemap.xml or sitemap_index.xml.
/// Returns an array of URL strings found in `<loc>` tags.
pub async fn discover_from_sitemap(root: &Url) -> Vec<String> {
    let candidates = [
        root.join("/sitemap.xml").unwrap().to_string(),
        root.join("/sitemap_index.xml").unwrap().to_string(),
    ];

    let loc_re = Regex::new(r"<loc>([^<]+)</loc>").unwrap();
    let mut found = Vec::new();

    for url in &candidates {
        if let Ok(resp) = fetch_with_retry(url, 3, 400).await {
            if let Ok(xml) = resp.text().await {
                for cap in loc_re.captures_iter(&xml) {
                    if let Some(m) = cap.get(1) {
                        found.push(m.as_str().to_string());
                    }
                }
            }
        }
    }

    found
}
