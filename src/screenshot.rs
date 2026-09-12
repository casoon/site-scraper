use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use url::Url;

use crate::headless::{chrome_not_found, find_chrome, Device};

/// Longest file name stem before it is shortened and made unique with a hash.
const MAX_STEM_LEN: usize = 150;

/// Where the URLs to screenshot come from.
pub enum Input {
    Url(String),
    File(PathBuf),
}

/// One URL and the PNG file it is saved to.
// Only read by the headless screenshot runner.
#[cfg_attr(not(feature = "headless"), allow(dead_code))]
pub struct ScreenshotJob {
    pub url: Url,
    pub dest: PathBuf,
}

/// Take full-page screenshots of all input URLs into `output`.
pub async fn run(
    input: Input,
    output: &Path,
    concurrency: usize,
    device: Device,
    timeout: Duration,
) -> Result<()> {
    // Validate every URL before the browser starts
    let urls = match input {
        Input::Url(s) => vec![parse_url(&s).map_err(|e| anyhow::anyhow!("{}", e))?],
        Input::File(path) => {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read URL file {}", path.display()))?;
            parse_url_list(&path.display().to_string(), &content)?
        }
    };

    // Screenshot each URL once, keeping the input order
    let mut unique = HashSet::new();
    let urls: Vec<Url> = urls
        .into_iter()
        .filter(|u| unique.insert(u.as_str().to_string()))
        .collect();

    let jobs: Vec<ScreenshotJob> = screenshot_filenames(&urls)
        .into_iter()
        .zip(urls)
        .map(|(name, url)| ScreenshotJob {
            url,
            dest: output.join(name),
        })
        .collect();

    let chrome = find_chrome().ok_or_else(chrome_not_found)?;

    #[cfg(feature = "headless")]
    {
        crate::output::info(&format!("Using browser: {}", chrome.display()));
        // Never clear the output directory; only successful screenshots replace their file
        tokio::fs::create_dir_all(output)
            .await
            .with_context(|| format!("Cannot create output directory {}", output.display()))?;
        crate::headless::screenshot::capture_all(&chrome, jobs, output, concurrency, device, timeout)
            .await
    }

    #[cfg(not(feature = "headless"))]
    {
        let _ = (jobs, concurrency, chrome, device, timeout);
        anyhow::bail!(
            "Headless mode is not compiled in.\n\
             Rebuild with:  cargo build --features headless"
        )
    }
}

/// Parse a single http(s) URL.
fn parse_url(s: &str) -> Result<Url, String> {
    let url = Url::parse(s).map_err(|e| format!("invalid URL '{}': {}", s, e))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        other => Err(format!(
            "invalid URL '{}': unsupported scheme '{}'",
            s, other
        )),
    }
}

/// Parse a URL list with one URL per line; blank lines and `#` comments are
/// ignored. All invalid lines are reported at once as `<source>:<line>: ...`.
fn parse_url_list(source: &str, content: &str) -> Result<Vec<Url>> {
    let mut urls = Vec::new();
    let mut errors = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_url(line) {
            Ok(url) => urls.push(url),
            Err(e) => errors.push(format!("{}:{}: {}", source, idx + 1, e)),
        }
    }

    if !errors.is_empty() {
        return Err(anyhow::anyhow!(errors.join("\n")).context(format!("{}: invalid URLs", source)));
    }
    if urls.is_empty() {
        anyhow::bail!("{}: no URLs found", source);
    }
    Ok(urls)
}

/// Deterministic PNG file names `<host>--<path>--<query>.png`, one per URL.
/// URLs whose readable names collide get a short stable hash of the full URL.
fn screenshot_filenames(urls: &[Url]) -> Vec<String> {
    let stems: Vec<String> = urls.iter().map(readable_stem).collect();

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for stem in &stems {
        *counts.entry(stem.as_str()).or_default() += 1;
    }

    stems
        .iter()
        .zip(urls)
        .map(|(stem, url)| {
            if counts[stem.as_str()] > 1 {
                format!("{}-{}.png", stem, short_hash(url.as_str()))
            } else {
                format!("{}.png", stem)
            }
        })
        .collect()
}

fn readable_stem(url: &Url) -> String {
    let mut host = slug(url.host_str().unwrap_or("unknown"));
    if let Some(port) = url.port() {
        host.push_str(&format!("-{}", port));
    }

    let path: Vec<String> = url
        .path()
        .split('/')
        .map(|seg| slug(&percent_decode(seg)))
        .filter(|seg| !seg.is_empty())
        .collect();

    let mut parts = vec![host];
    if path.is_empty() {
        parts.push("index".to_string());
    } else {
        parts.extend(path);
    }
    if let Some(q) = url.query().map(|q| slug(&percent_decode(q))) {
        if !q.is_empty() {
            parts.push(q);
        }
    }

    let stem = parts.join("--");
    if stem.len() > MAX_STEM_LEN {
        // Slugs are ASCII, so byte slicing is safe
        format!("{}-{}", &stem[..MAX_STEM_LEN - 9], short_hash(url.as_str()))
    } else {
        stem
    }
}

/// Lowercase ASCII slug: umlauts transliterated, other characters become `-`,
/// runs of `-` collapsed (so `--` stays free as the part separator).
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '.' => out.push(c),
            _ => {
                if !out.ends_with('-') {
                    out.push('-');
                }
            }
        }
    }
    out.trim_matches('-').to_string()
}

/// Decode `%XX` escapes; invalid UTF-8 is replaced.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 8 hex characters of a FNV-1a hash — stable across runs and Rust versions.
fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", (h ^ (h >> 32)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn names(urls: &[&str]) -> Vec<String> {
        let urls: Vec<Url> = urls.iter().map(|s| u(s)).collect();
        screenshot_filenames(&urls)
    }

    #[test]
    fn root_url_is_named_index() {
        assert_eq!(names(&["https://example.com/"]), ["example.com--index.png"]);
    }

    #[test]
    fn path_and_query_become_parts() {
        assert_eq!(
            names(&[
                "https://example.com/produkte/produkt-a",
                "https://example.com/suche?q=astro",
            ]),
            [
                "example.com--produkte--produkt-a.png",
                "example.com--suche--q-astro.png"
            ]
        );
    }

    #[test]
    fn umlauts_are_transliterated() {
        assert_eq!(
            names(&["https://example.com/über-uns/größe"]),
            ["example.com--ueber-uns--groesse.png"]
        );
    }

    #[test]
    fn port_is_part_of_host() {
        assert_eq!(
            names(&["http://127.0.0.1:8765/about/"]),
            ["127.0.0.1-8765--about.png"]
        );
    }

    #[test]
    fn colliding_slugs_get_distinct_stable_hashes() {
        let urls = [
            "https://example.com/a-b",
            "https://example.com/a_b/../a-b/",
            "https://example.com/A-B",
        ];
        let first = names(&urls);
        assert_eq!(first, names(&urls));
        let unique: HashSet<&String> = first.iter().collect();
        assert_eq!(unique.len(), 3);
        assert!(first.iter().all(|n| n.starts_with("example.com--a-b-")));
    }

    #[test]
    fn unique_slugs_get_no_hash() {
        assert_eq!(
            names(&["https://example.com/a", "https://example.com/b"]),
            ["example.com--a.png", "example.com--b.png"]
        );
    }

    #[test]
    fn long_names_are_shortened_with_hash() {
        let long = format!("https://example.com/{}", "x".repeat(300));
        let name = &names(&[&long])[0];
        assert!(name.len() <= MAX_STEM_LEN + ".png".len());
        assert!(name.starts_with("example.com--xxx"));
    }

    #[test]
    fn url_list_skips_blank_lines_and_comments() {
        let content = "# my list\nhttps://example.com/\n\n  https://example.com/about  \n";
        let urls = parse_url_list("urls.txt", content).unwrap();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[1].as_str(), "https://example.com/about");
    }

    #[test]
    fn url_list_reports_all_invalid_lines_with_numbers() {
        let content = "https://example.com/\nnot a url\n# ok\nftp://example.com/file\n";
        let err = format!("{:#}", parse_url_list("urls.txt", content).unwrap_err());
        assert!(err.contains("urls.txt:2: invalid URL 'not a url'"));
        assert!(err.contains(
            "urls.txt:4: invalid URL 'ftp://example.com/file': unsupported scheme 'ftp'"
        ));
    }

    #[test]
    fn url_list_without_urls_is_an_error() {
        assert!(parse_url_list("urls.txt", "# only comments\n\n").is_err());
    }
}
