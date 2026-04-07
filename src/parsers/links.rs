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

    seen.into_iter().filter_map(|s| Url::parse(&s).ok()).collect()
}
