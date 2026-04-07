# Site Scraper

[![CI](https://github.com/casoon/site-scraper/actions/workflows/ci.yml/badge.svg)](https://github.com/casoon/site-scraper/actions/workflows/ci.yml)

A fast CLI tool written in Rust that creates static copies of websites. It crawls from a starting URL, saves HTML files along with stylesheets and scripts locally, and replaces images with placeholders.

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

### Examples

```sh
# Standard crawl (simulates a browser)
site-scraper https://www.example.com

# Identify as bot/crawler
site-scraper https://www.example.com --bot

# Deeper crawl with local image placeholders
site-scraper https://www.example.com --max-depth 3 --placeholder local

# Faster crawl with more concurrency and less delay
site-scraper https://www.example.com --concurrency 8 --delay-ms 100
```

### Output

All results are saved to `./output/<domain>/`. The folder is recreated on each run. HTML files are stored in a directory structure matching the URL paths. CSS, JS and fonts are downloaded and all references are rewritten to local relative paths. Images are replaced with placeholders.

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--max-depth` | `2` | Maximum crawl depth relative to the start page |
| `--concurrency` | `4` | Number of parallel downloads |
| `--delay-ms` | `300` | Delay between requests in milliseconds |
| `--placeholder` | `external` | Image placeholder strategy: `external` (placehold.co) or `local` (generated PNG) |
| `--sitemap` | `true` | Include sitemap.xml URLs as seeds |
| `--allow-external-assets` | `true` | Download external CSS/JS or leave as-is |
| `--bot` | `false` | Identify as crawler instead of simulating a browser |
| `--user-agent` | - | Custom User-Agent header (overrides `--bot`) |
| `--referer` | - | Custom Referer header |

### Identity Modes

By default, site-scraper sends realistic browser headers (Chrome User-Agent, Sec-Ch-Ua, etc.) to avoid bot detection. With `--bot`, it identifies honestly as `site-scraper/1.2` and sends minimal headers.

## Build

```sh
cargo build --release
```

## License

[MIT](LICENSE)
