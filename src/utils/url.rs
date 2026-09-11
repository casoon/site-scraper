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
                Some(q) => {
                    p.push_str(&q);
                    p.push_str(".html");
                }
                None => p.push_str("index.html"),
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn same_origin_directory_path_gets_index_html() {
        let root = u("https://example.com/");
        let target = u("https://example.com/blog/");
        let path = url_to_local_path(&root, &target, Path::new("out"), None);
        assert_eq!(path, PathBuf::from("out/blog/index.html"));
    }

    #[test]
    fn same_origin_path_without_extension_gets_html_suffix() {
        let root = u("https://example.com/");
        let target = u("https://example.com/about");
        let path = url_to_local_path(&root, &target, Path::new("out"), None);
        assert_eq!(path, PathBuf::from("out/about.html"));
    }

    #[test]
    fn same_origin_path_with_extension_is_kept_as_is() {
        let root = u("https://example.com/");
        let target = u("https://example.com/style.css");
        let path = url_to_local_path(&root, &target, Path::new("out"), None);
        assert_eq!(path, PathBuf::from("out/style.css"));
    }

    #[test]
    fn query_string_is_slugified_into_filename() {
        let root = u("https://example.com/");
        let target = u("https://example.com/?page_id=32&foo=bar");
        let path = url_to_local_path(&root, &target, Path::new("out"), None);
        assert_eq!(path, PathBuf::from("out/page_id-32-foo-bar.html"));
    }

    #[test]
    fn external_url_is_namespaced_under_host_dir() {
        let root = u("https://example.com/");
        let target = u("https://cdn.other.com/img/logo.png");
        let path = url_to_local_path(&root, &target, Path::new("out"), None);
        assert_eq!(path, PathBuf::from("out/cdn.other.com/img/logo.png"));
    }

    #[test]
    fn external_url_without_extension_uses_ext_hint() {
        let root = u("https://example.com/");
        let target = u("https://cdn.other.com/asset?v=1");
        let path = url_to_local_path(&root, &target, Path::new("out"), Some(".js"));
        assert_eq!(path, PathBuf::from("out/cdn.other.com/asset.js"));
    }

    #[test]
    fn query_to_slug_replaces_non_alphanumeric_and_trims_dashes() {
        assert_eq!(query_to_slug("page_id=32&foo=bar"), "page_id-32-foo-bar");
        assert_eq!(query_to_slug("a=b&&c=d"), "a-b-c-d");
    }

    #[test]
    fn make_relative_walks_up_to_sibling_dir() {
        let from = Path::new("out/blog/index.html");
        let to = Path::new("out/style.css");
        assert_eq!(make_relative(from, to), "../style.css");
    }

    #[test]
    fn make_relative_prefixes_same_dir_with_dot_slash() {
        let from = Path::new("out/index.html");
        let to = Path::new("out/style.css");
        assert_eq!(make_relative(from, to), "./style.css");
    }

    #[test]
    fn has_extension_detects_alphanumeric_suffix_only() {
        assert!(has_extension("/style.css"));
        assert!(!has_extension("/blog/"));
        assert!(!has_extension("/about"));
        assert!(!has_extension("/weird.")); // trailing dot, empty ext
    }
}
