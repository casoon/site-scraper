use std::path::{Path, PathBuf};

use runemark::ErrorBlock;

use crate::output::BlockError;

/// Viewport a headless render or screenshot uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Device {
    /// 1440x900 (default)
    Desktop,
    /// 768x1024
    Tablet,
    /// 375x812 with mobile emulation (scale factor, touch, mobile user agent)
    Mobile,
}

/// Find a usable Chrome or Chromium executable on the current system.
pub fn find_chrome() -> Option<PathBuf> {
    let abs_paths: &[&str] = &[
        // macOS
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        // Linux
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ];

    for &p in abs_paths {
        if Path::new(p).exists() {
            return Some(PathBuf::from(p));
        }
    }

    // Fallback: search PATH via `which`
    for cmd in &[
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ] {
        if let Ok(out) = std::process::Command::new("which").arg(cmd).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(PathBuf::from(s));
                }
            }
        }
    }

    None
}

/// Error block telling the user how to install Chrome / Chromium.
pub fn chrome_not_found() -> anyhow::Error {
    let commands: &[&str] = if cfg!(target_os = "macos") {
        &[
            "brew install --cask google-chrome",
            "brew install --cask chromium   # open-source build",
        ]
    } else if cfg!(target_os = "windows") {
        &["winget install Google.Chrome"]
    } else {
        &[
            "sudo apt install -y chromium-browser   # Ubuntu / Debian",
            "sudo pacman -S chromium   # Arch Linux",
        ]
    };
    let block = commands.iter().fold(
        ErrorBlock::new("Chrome or Chromium not found")
            .with_explanation(
                "--headless and the screenshot command need a local Chrome or Chromium.",
            )
            .with_remedy("Install it, then run site-scraper again:"),
        |block, cmd| block.add_command(*cmd),
    );
    BlockError(block).into()
}

#[cfg(feature = "headless")]
mod browser;
#[cfg(feature = "headless")]
pub mod crawler;
#[cfg(feature = "headless")]
pub mod screenshot;
