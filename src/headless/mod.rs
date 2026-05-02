use std::path::{Path, PathBuf};

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

/// Print OS-specific installation instructions for Chrome / Chromium.
pub fn print_install_instructions() {
    eprintln!("\nError: --headless requires Chrome or Chromium, but none was found.\n");
    eprintln!("Install Chrome or Chromium for your platform:\n");
    eprintln!("  macOS");
    eprintln!("    brew install --cask google-chrome");
    eprintln!("    brew install --cask chromium          # open-source build\n");
    eprintln!("  Ubuntu / Debian");
    eprintln!("    sudo apt install -y chromium-browser");
    eprintln!("    # or Google Chrome:");
    eprintln!("    wget https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb");
    eprintln!("    sudo dpkg -i google-chrome-stable_current_amd64.deb\n");
    eprintln!("  Arch Linux");
    eprintln!("    sudo pacman -S chromium\n");
    eprintln!("  Windows");
    eprintln!("    winget install Google.Chrome\n");
    eprintln!("After installation, re-run site-scraper with --headless.\n");
}

#[cfg(feature = "headless")]
pub mod crawler;
