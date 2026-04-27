use std::path::{Path, PathBuf};

use url::Url;

use super::filesystem::safe_filename;

/// Map a URL to a local file path inside `out_dir`.
/// Handles both same-origin and external URLs.
pub fn url_to_local_path(
    root: &Url,
    target: &Url,
    out_dir: &Path,
    ext_hint: Option<&str>,
) -> PathBuf {
    if target.origin() != root.origin() {
        // External: keep hostname as subfolder
        let host = target.host_str().unwrap_or("unknown");
        let host_dir = out_dir.join(safe_filename(host));
        let pathname = target.path();
        let pathname = if pathname.ends_with('/') {
            format!("{}index", pathname)
        } else {
            pathname.to_string()
        };
        let with_ext = if has_extension(&pathname) {
            pathname
        } else {
            format!("{}{}", pathname, ext_hint.unwrap_or(""))
        };
        // Strip query/fragment
        let clean = with_ext.split('?').next().unwrap_or(&with_ext);
        let clean = clean.split('#').next().unwrap_or(clean);
        // Remove leading slash for joining
        let clean = clean.trim_start_matches('/');
        host_dir.join(clean)
    } else {
        // Same-origin
        let mut p = target.path().to_string();
        let query_slug = target.query().map(query_to_slug);

        if p.ends_with('/') {
            match query_slug {
                Some(q) => { p.push_str(&q); p.push_str(".html"); }
                None    => p.push_str("index.html"),
            }
        } else if !has_extension(&p) {
            if let Some(q) = query_slug {
                p.push('-');
                p.push_str(&q);
            }
            p.push_str(".html");
        }
        let clean = p.trim_start_matches('/');
        out_dir.join(clean)
    }
}

/// Turn a query string into a safe filename segment.
/// `page_id=32&foo=bar` → `page_id-32-foo-bar`
fn query_to_slug(query: &str) -> String {
    query
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Create a relative path from one file to another.
/// Ensures the result starts with `./` for consistency.
pub fn make_relative(from_file: &Path, to_file: &Path) -> String {
    let from_dir = from_file.parent().unwrap_or(Path::new(""));
    let rel = pathdiff::diff_paths(to_file, from_dir).unwrap_or_else(|| to_file.to_path_buf());
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    if rel_str.starts_with('.') {
        rel_str.to_string()
    } else {
        format!("./{}", rel_str)
    }
}

fn has_extension(path: &str) -> bool {
    if let Some(last_segment) = path.rsplit('/').next() {
        last_segment.contains('.')
            && last_segment
                .rsplit('.')
                .next()
                .map(|ext| ext.chars().all(|c| c.is_ascii_alphanumeric()) && !ext.is_empty())
                .unwrap_or(false)
    } else {
        false
    }
}
