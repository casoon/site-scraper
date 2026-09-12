---
title: Crawl modes
description: Choose browser-like HTTP requests, explicit bot identity, or a rendered headless crawl.
order: 1
---

## Browser-like requests

The default HTTP crawler sends realistic browser headers. It is fast and works well when the
server response already contains the page content.

```sh
site-scraper https://www.example.com --max-depth 2
```

## Bot identity

Use `--bot` to identify the request as `site-scraper/1.2` and send minimal headers:

```sh
site-scraper https://www.example.com --bot
```

`--user-agent` overrides the chosen identity. `--referer` sets an explicit Referer header.

## Headless Chrome

Use `--headless` when the raw response lacks content rendered by React, Vue, Angular or another
client-side framework:

```sh
site-scraper https://www.example.com --headless
```

The browser waits for the page to load, scrolls to trigger lazy and scroll-driven content,
removes script tags from saved HTML, and reveals supported animation initial states.

Headless mode is slower and requires a local Chrome or Chromium installation. See
[Headless mode and screenshots](../headless-and-screenshots/) for build and capture details.
