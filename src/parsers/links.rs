use std::collections::HashSet;

use scraper::{Html, Selector};
use url::Url;

/// Extract absolute URLs from `<a href>` for same-origin crawling.
/// Filters out mailto:, tel:, javascript: links.
pub fn extract_links(html: &str, root: &Url, doc_url: &Url) -> Vec<Url> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").unwrap();
    let mut seen = HashSet::new();

    for element in document.select(&selector) {
        let href = match element.value().attr("href") {
            Some(h) => h.trim(),
            None => continue,
        };

        if href.is_empty()
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
            || href.starts_with("javascript:")
        {
            continue;
        }

        if let Ok(mut url) = doc_url.join(href) {
            if url.origin() == root.origin() {
                url.set_fragment(None);
                seen.insert(url.to_string());
            }
        }
    }

    seen.into_iter()
        .filter_map(|s| Url::parse(&s).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(html: &str) -> HashSet<String> {
        let root = Url::parse("https://example.com/").unwrap();
        let doc_url = Url::parse("https://example.com/blog/").unwrap();
        extract_links(html, &root, &doc_url)
            .into_iter()
            .map(|u| u.to_string())
            .collect()
    }

    #[test]
    fn resolves_relative_links_against_doc_url() {
        let links = urls(r#"<a href="post">Post</a>"#);
        assert!(links.contains("https://example.com/blog/post"));
    }

    #[test]
    fn filters_mailto_tel_and_javascript_links() {
        let links = urls(
            r#"
            <a href="mailto:test@example.com">Mail</a>
            <a href="tel:+123456">Call</a>
            <a href="javascript:void(0)">JS</a>
            "#,
        );
        assert!(links.is_empty());
    }

    #[test]
    fn filters_out_external_origin_links() {
        let links = urls(r#"<a href="https://other.com/page">External</a>"#);
        assert!(links.is_empty());
    }

    #[test]
    fn strips_fragment_from_same_origin_links() {
        let links = urls(r#"<a href="/page#section">Anchor</a>"#);
        assert!(links.contains("https://example.com/page"));
        assert!(!links.iter().any(|u| u.contains('#')));
    }

    #[test]
    fn deduplicates_equivalent_links() {
        let root = Url::parse("https://example.com/").unwrap();
        let doc_url = Url::parse("https://example.com/blog/").unwrap();
        let html = r#"<a href="/page">A</a><a href="/page#top">B</a>"#;
        let links = extract_links(html, &root, &doc_url);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn ignores_links_without_href() {
        let links = urls(r#"<a>No href</a>"#);
        assert!(links.is_empty());
    }
}
