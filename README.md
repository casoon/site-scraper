# Site Scraper

[![CI](https://github.com/casoon/site-scraper/actions/workflows/ci.yml/badge.svg)](https://github.com/casoon/site-scraper/actions/workflows/ci.yml)

A fast CLI tool written in Rust that creates static copies of websites. It crawls from a starting URL, saves HTML files along with stylesheets and scripts locally, and downloads or replaces images. When called with only a URL, it guides you through the most important options interactively.

## Why?

When migrating client websites from a CMS (WordPress, TYPO3, Drupal, etc.) to a modern stack like Astro, the old site often needs to be preserved first. Site Scraper creates a complete static snapshot of the existing site before the relaunch -- as a reference, for content extraction, or simply as a backup. Instead of relying on the CMS staying online, you get a self-contained local copy with all HTML, CSS, JS and fonts in place.

## Installation

### via curl

```sh
curl -fsSL https://raw.githubusercontent.com/casoon/site-scraper/main/install.sh | bash
```

Custom install directory:

```sh
INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/casoon/site-scraper/main/install.sh | bash
```

### via Cargo

```sh
cargo install --git https://github.com/casoon/site-scraper
```

### From source

```sh
git clone https://github.com/casoon/site-scraper.git
cd site-scraper
cargo install --path .
```

## Usage

```sh
site-scraper <URL> [OPTIONS]
```

### Interactive mode

When only a URL is provided, site-scraper prompts for the most important settings. Two modes are available:

```sh
site-scraper https://www.example.com

? Setup        › Quick (depth, images, mode)
               › Extended (+ headless, concurrency, delay, sitemap, assets)

? Crawl depth  › 1 – start page only / 2 – standard / 3 – deeper / Enter a custom number …
? Images       › Download originals / Local gray placeholder / External – placehold.co
? Mode         › Simulate browser / Identify as bot / Headless Chrome
```

All flags can be passed directly to skip the prompts (useful for scripts and CI).

### Examples

```sh
# Interactive mode — prompts for depth, images, and mode
site-scraper https://www.example.com

# Standard crawl (simulates a browser, no prompts)
site-scraper https://www.example.com --max-depth 2 --placeholder external

# Download original images
site-scraper https://www.example.com --placeholder real

# Headless Chrome — renders JavaScript before saving (React, Next.js, Vue, …)
site-scraper https://www.example.com --headless

# Headless with full-page screenshots
site-scraper https://www.example.com --headless --screenshot

# Identify as bot/crawler
site-scraper https://www.example.com --bot

# Deeper crawl with local image placeholders
site-scraper https://www.example.com --max-depth 3 --placeholder local

# Faster crawl with more concurrency and less delay
site-scraper https://www.example.com --concurrency 8 --delay-ms 100
```

### Output

All results are saved to `./output/<domain>/`. The folder is recreated on each run. HTML files are stored in a directory structure matching the URL paths. CSS, JS and fonts are downloaded and all references are rewritten to local relative paths. Images are either downloaded as originals or replaced with placeholders, depending on the `--placeholder` option.

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--max-depth` | interactive / `2` | Maximum crawl depth relative to the start page |
| `--concurrency` | `4` | Number of parallel downloads |
| `--delay-ms` | `300` | Delay between requests in milliseconds |
| `--placeholder` | interactive / `external` | Image strategy: `real` (download originals), `local` (generated PNG), or `external` (placehold.co) |
| `--sitemap` | `true` | Include sitemap.xml URLs as seeds |
| `--allow-external-assets` | `true` | Download external CSS/JS or leave as-is |
| `--bot` | interactive / `false` | Identify as crawler instead of simulating a browser |
| `--headless` | `false` | Use Chrome/Chromium to render JavaScript before saving (requires Chrome installed) |
| `--screenshot` | `false` | Save a full-page PNG screenshot per page (requires `--headless`) |
| `--user-agent` | - | Custom User-Agent header (overrides `--bot`) |
| `--referer` | - | Custom Referer header |

### Identity Modes

By default, site-scraper sends realistic browser headers (Chrome User-Agent, Sec-Ch-Ua, etc.) to avoid bot detection. With `--bot`, it identifies honestly as `site-scraper/1.2` and sends minimal headers.

### Headless mode

`--headless` launches a local Chrome or Chromium instance to fully render the page before saving. This is useful for JavaScript-heavy sites (React, Next.js, Vue, Angular) where the raw HTML is incomplete without JS execution.

What headless mode does before saving each page:
- Waits for JS frameworks to mount (React hooks, event listeners)
- Scrolls to the bottom to trigger scroll-driven styles and IntersectionObserver animations
- Removes script tags from the saved HTML so the static file does not re-run JS locally
- Reveals animation initial states (`opacity-0` + translate classes) so all content is visible

Chrome or Chromium must be installed on the system. If not found, installation instructions for your OS are printed.

To build with headless support:

```sh
cargo build --release --features headless
```

Pre-built binaries from the releases page already include headless support.

## Build

```sh
cargo build --release
```

## License

[MIT](LICENSE)
