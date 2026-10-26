---
title: CLI reference
description: Arguments, defaults and switches for website mirroring and screenshot capture.
order: 1
---

## Crawl

```text
site-scraper <URL> [OPTIONS]
```

| Option | Default | Purpose |
| --- | --- | --- |
| `--max-depth <N>` | `2` outside interactive setup | Maximum crawl depth from the start page. |
| `--concurrency <N>` | `4` | Parallel downloads. |
| `--delay-ms <MS>` | `300` | Delay between requests. |
| `--placeholder <MODE>` | `external` outside interactive setup | `real`, `local`, or `external` image handling. |
| `--sitemap` / `--no-sitemap` | on | Include or exclude sitemap URLs as seeds. |
| `--allow-external-assets` / `--no-allow-external-assets` | on | Download or preserve external CSS and JavaScript references. |
| `--bot` | off | Identify as a crawler instead of simulating a browser. |
| `--headless` | off | Render pages in local Chrome or Chromium. |
| `--screenshot` | off | Save each crawled page as a full-page PNG; requires `--headless`. |
| `--user-agent <VALUE>` | browser-like or bot identity | Override the User-Agent header. |
| `--referer <VALUE>` | none | Set a custom Referer header. |
| `--device <DEVICE>` | `desktop` | Viewport for `--headless`: `desktop`, `tablet` or `mobile` (see below). |
| `--timeout <SECONDS>` | `60` | Browser navigation timeout for `--headless`; minimum 1. |

Passing only the URL in an interactive terminal opens the guided setup. Supplying any option uses
its explicit value and the non-interactive defaults for the rest.

## Screenshot

```text
site-scraper screenshot [URL]
site-scraper screenshot --file <FILE>
```

Exactly one URL source is required.

| Option | Default | Purpose |
| --- | --- | --- |
| `--file <FILE>` | none | UTF-8 file containing one URL per line. |
| `--output <DIR>` | `screenshots` | Destination directory. |
| `--concurrency <N>` | `2` | Parallel browser pages; minimum 1. |
| `--device <DEVICE>` | `desktop` | Viewport: `desktop`, `tablet` or `mobile`. |
| `--timeout <SECONDS>` | `60` | Seconds to wait for navigation before a URL fails; minimum 1. |

Both commands support `--help`; the main command also supports `--version`.

## Devices

| Device | Viewport |
| --- | --- |
| `desktop` | 1440 × 900 |
| `tablet` | 768 × 1024 |
| `mobile` | 375 × 812 with mobile emulation: device scale factor, touch and a mobile User-Agent |
