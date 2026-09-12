# Site Scraper

[![CI](https://github.com/casoon/site-scraper/actions/workflows/ci.yml/badge.svg)](https://github.com/casoon/site-scraper/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-GitHub%20Pages-087685)](https://casoon.github.io/site-scraper/)

A fast CLI tool written in Rust that creates static copies of websites. It crawls from a starting URL, saves HTML files along with stylesheets and scripts locally, and downloads or replaces images. When called with only a URL, it guides you through the most important options interactively.

[Read the full documentation →](https://casoon.github.io/site-scraper/)

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

? Crawl depth  › 0 – start page only / 2 – standard / 3 – deeper / Enter a custom number …
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

Progress and per-page status lines go to stderr (with a progress bar in an interactive terminal), the final summary goes to stdout. Colors follow [`NO_COLOR`](https://no-color.org/) and are disabled when output is redirected. The exit code is `1` if any page or screenshot failed.

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--max-depth` | interactive / `2` | Maximum crawl depth relative to the start page |
| `--concurrency` | `4` | Number of parallel downloads |
| `--delay-ms` | `300` | Delay between requests in milliseconds |
| `--placeholder` | interactive / `external` | Image strategy: `real` (download originals), `local` (generated PNG), or `external` (placehold.co) |
| `--sitemap` / `--no-sitemap` | on | Include sitemap.xml URLs as seeds |
| `--allow-external-assets` / `--no-allow-external-assets` | on | Download external CSS/JS or leave as-is |
| `--bot` | interactive / `false` | Identify as crawler instead of simulating a browser |
| `--headless` | `false` | Use Chrome/Chromium to render JavaScript before saving (requires Chrome installed) |
| `--screenshot` | `false` | Save a full-page PNG screenshot per page (requires `--headless`) |
| `--device` | `desktop` | Viewport for `--headless`: `desktop` (1440x900), `tablet` (768x1024) or `mobile` (375x812, with mobile emulation) |
| `--timeout` | `60` | Seconds to wait for browser navigation before failing (only used with `--headless`) |
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

### Screenshot mode

`site-scraper screenshot` only takes full-page PNG screenshots — nothing is mirrored. It uses the same Chrome rendering as headless mode.

```sh
# One URL
site-scraper screenshot https://www.example.com

# A list of URLs, into a custom directory
site-scraper screenshot --file urls.txt --output shots/
```

The URL file is UTF-8 with one URL per line; blank lines and lines starting with `#` are ignored. All URLs are validated before the browser starts, and invalid lines are reported with their line number.

| Option | Default | Description |
|--------|---------|-------------|
| `[URL]` / `--file <FILE>` | - | A single URL or a URL file (exactly one is required) |
| `--output <DIR>` | `screenshots` | Output directory; existing files in it are kept |
| `--concurrency` | `2` | Number of parallel browser pages |
| `--device` | `desktop` | Viewport: `desktop` (1440x900), `tablet` (768x1024) or `mobile` (375x812, with mobile emulation) |
| `--timeout` | `60` | Seconds to wait for browser navigation before failing |

Files are named `<host>--<path>--<query>.png`, e.g. `example.com--index.png` or `example.com--suche--q-astro.png`. If two URLs map to the same name, a short stable hash of the URL is appended. A screenshot only replaces its own file, and only once it was taken successfully. If any screenshot fails, the exit code is `1`.

Pages that respond with an HTTP error status (e.g. a custom 404) are still captured, with the status shown next to the saved file.

## Build

```sh
cargo build --release
```

## License

[MIT](LICENSE)
