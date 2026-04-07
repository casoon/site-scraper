use std::path::Path;

use anyhow::Result;
use tokio::fs;

/// Ensure a directory exists, creating it recursively if needed.
pub async fn ensure_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).await?;
    Ok(())
}

/// Convert a string to a safe filename by replacing invalid characters.
pub fn safe_filename(s: &str) -> String {
    let replaced: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse multiple underscores
    let mut result = String::new();
    let mut prev_underscore = false;
    for c in replaced.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
            result.push(c);
        }
    }
    result.trim_matches('_').to_string()
}
