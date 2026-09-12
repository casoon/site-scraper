---
title: Overview
description: What site-scraper saves, which mode to choose, and where to find the result.
order: 0
---

site-scraper creates a local static copy of a website. It starts at one URL, follows internal
links, downloads pages and assets, and rewrites references to local relative paths.

## Choose a mode

- **Standard crawl** fetches HTML directly and is the fastest choice for server-rendered sites.
- **Headless crawl** uses local Chrome or Chromium when content appears only after JavaScript runs.
- **Screenshot** captures full-page PNG files without creating a static copy.

## What gets saved

A crawl recreates `output/<domain>/` on every run. HTML follows the source URL structure; CSS,
JavaScript and fonts are downloaded when allowed. Images follow the selected placeholder strategy.

The screenshot command writes to `screenshots/` by default and keeps unrelated existing files.

## Start here

Install the binary, then follow the [Quickstart](getting-started/quickstart/). For JavaScript-heavy
pages, continue with [Headless mode and screenshots](guides/headless-and-screenshots/).
