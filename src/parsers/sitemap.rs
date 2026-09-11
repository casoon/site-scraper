use std::collections::{HashSet, VecDeque};

use regex::Regex;
use url::Url;

use crate::network::fetch::fetch_with_retry;

/// How many levels of nested sitemap index files are followed.
const MAX_INDEX_DEPTH: u32 = 3;

/// Discover URLs from sitemap.xml or sitemap_index.xml.
/// Sitemap index files are followed recursively (same origin only).
/// Returns the page URLs found in `<loc>` tags.
pub async fn discover_from_sitemap(root: &Url) -> Vec<String> {
    let mut queue: VecDeque<(String, u32)> = ["/sitemap.xml", "/sitemap_index.xml"]
        .iter()
        .map(|path| (root.join(path).unwrap().to_string(), 0))
        .collect();
    let mut visited = HashSet::new();
    let mut found = Vec::new();

    while let Some((url, depth)) = queue.pop_front() {
        if !visited.insert(url.clone()) {
            continue;
        }
        let Ok(resp) = fetch_with_retry(&url, 3, 400).await else {
            continue;
        };
        let Ok(xml) = resp.text().await else {
            continue;
        };

        let locs = extract_locs(&xml);
        if !is_sitemap_index(&xml) {
            found.extend(locs);
        } else if depth < MAX_INDEX_DEPTH {
            for loc in locs {
                if Url::parse(&loc).is_ok_and(|u| u.origin() == root.origin()) {
                    queue.push_back((loc, depth + 1));
                }
            }
        }
    }

    found
}

/// Whether the XML is a sitemap index (its `<loc>`s are further sitemaps).
fn is_sitemap_index(xml: &str) -> bool {
    xml.contains("<sitemapindex")
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
    fn detects_sitemap_index() {
        let xml = r#"<?xml version="1.0"?><sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <sitemap><loc>https://example.com/post-sitemap.xml</loc></sitemap></sitemapindex>"#;
        assert!(is_sitemap_index(xml));
    }

    #[test]
    fn urlset_is_not_a_sitemap_index() {
        let xml = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <url><loc>https://example.com/</loc></url></urlset>"#;
        assert!(!is_sitemap_index(xml));
    }

    #[test]
    fn ignores_malformed_unclosed_loc_tags() {
        let xml = "<urlset><url><loc>https://example.com/</url></urlset>";
        assert!(extract_locs(xml).is_empty());
    }
}
