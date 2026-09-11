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

    let mut found = Vec::new();

    for url in &candidates {
        if let Ok(resp) = fetch_with_retry(url, 3, 400).await {
            if let Ok(xml) = resp.text().await {
                found.extend(extract_locs(&xml));
            }
        }
    }

    found
}

/// Extract all `<loc>` values from sitemap XML.
fn extract_locs(xml: &str) -> Vec<String> {
    let loc_re = Regex::new(r"<loc>([^<]+)</loc>").unwrap();
    loc_re
        .captures_iter(xml)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_multiple_locs() {
        let xml = "<urlset><url><loc>https://example.com/</loc></url>\
                   <url><loc>https://example.com/about</loc></url></urlset>";
        assert_eq!(
            extract_locs(xml),
            vec!["https://example.com/", "https://example.com/about"]
        );
    }

    #[test]
    fn returns_empty_when_no_locs_present() {
        assert!(extract_locs("<urlset></urlset>").is_empty());
    }

    #[test]
    fn ignores_malformed_unclosed_loc_tags() {
        let xml = "<urlset><url><loc>https://example.com/</url></urlset>";
        assert!(extract_locs(xml).is_empty());
    }
}
