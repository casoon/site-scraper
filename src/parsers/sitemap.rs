use std::collections::{HashSet, VecDeque};
use std::io::Read;

use flate2::read::GzDecoder;
use regex::Regex;
use url::Url;

use crate::network::fetch::fetch_with_retry;

/// How many levels of nested sitemap index files are followed.
const MAX_INDEX_DEPTH: u32 = 3;

/// Well-known sitemap locations; `wp-sitemap.xml` is WordPress core without an SEO plugin.
const SITEMAP_PATHS: [&str; 3] = ["/sitemap.xml", "/sitemap_index.xml", "/wp-sitemap.xml"];

/// Discover URLs from the sitemaps listed in robots.txt and at well-known locations.
/// Sitemap index files are followed recursively (same origin only).
/// Returns the page URLs found in `<loc>` tags.
pub async fn discover_from_sitemap(root: &Url) -> Vec<String> {
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    if let Ok(resp) = fetch_with_retry(root.join("/robots.txt").unwrap().as_str(), 3, 400).await {
        if let Ok(robots) = resp.text().await {
            queue.extend(
                sitemaps_from_robots(&robots, root)
                    .into_iter()
                    .map(|u| (u, 0)),
            );
        }
    }
    queue.extend(
        SITEMAP_PATHS
            .iter()
            .map(|path| (root.join(path).unwrap().to_string(), 0)),
    );
    let mut visited = HashSet::new();
    let mut found = Vec::new();

    while let Some((url, depth)) = queue.pop_front() {
        if !visited.insert(url.clone()) {
            continue;
        }
        let Ok(resp) = fetch_with_retry(&url, 3, 400).await else {
            continue;
        };
        let Ok(bytes) = resp.bytes().await else {
            continue;
        };
        let Some(xml) = decode_body(&bytes) else {
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

/// `Sitemap:` entries from robots.txt (same origin only).
fn sitemaps_from_robots(robots: &str, root: &Url) -> Vec<String> {
    robots
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("sitemap")
                .then(|| root.join(value.trim()).ok())?
        })
        .filter(|url| url.origin() == root.origin())
        .map(|url| url.to_string())
        .collect()
}

/// Sitemap body as text; gzip-compressed files (`.xml.gz`) are unpacked.
fn decode_body(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut xml = String::new();
        GzDecoder::new(bytes).read_to_string(&mut xml).ok()?;
        Some(xml)
    } else {
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
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
        .filter_map(|cap| cap.get(1).map(|m| decode_entities(m.as_str().trim())))
        .collect()
}

/// Decode XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#..;`).
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').and_then(|end| {
            let c = match &rest[1..end] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16)
                    .ok()
                    .and_then(char::from_u32),
                e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            }?;
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_entities_in_locs() {
        let xml = "<urlset><url><loc>https://example.com/?page_id=2&amp;lang=de</loc></url>\
                   <url><loc> https://example.com/&#x41;&#66;&lt;x&gt; </loc></url></urlset>";
        assert_eq!(
            extract_locs(xml),
            vec![
                "https://example.com/?page_id=2&lang=de",
                "https://example.com/AB<x>"
            ]
        );
    }

    #[test]
    fn keeps_unknown_entities_and_bare_ampersands() {
        assert_eq!(decode_entities("a&b&foo;c"), "a&b&foo;c");
    }

    #[test]
    fn reads_sitemaps_from_robots_txt() {
        let root = Url::parse("https://example.com/").unwrap();
        let robots = "User-agent: *\nDisallow: /admin\n\
                      sitemap: https://example.com/wp-sitemap.xml\n\
                      Sitemap: /news-sitemap.xml\n\
                      Sitemap: https://cdn.other.com/sitemap.xml\n";
        assert_eq!(
            sitemaps_from_robots(robots, &root),
            vec![
                "https://example.com/wp-sitemap.xml",
                "https://example.com/news-sitemap.xml"
            ]
        );
    }

    #[test]
    fn unpacks_gzipped_sitemaps() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let xml = "<urlset><url><loc>https://example.com/</loc></url></urlset>";
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(xml.as_bytes()).unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(decode_body(&gz).as_deref(), Some(xml));
        assert_eq!(decode_body(xml.as_bytes()).as_deref(), Some(xml));
    }

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
