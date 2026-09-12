---
title: Headless mode and screenshots
description: Render JavaScript before mirroring a page or capture full-page PNG files only.
order: 3
---

## Render before mirroring

```sh
site-scraper https://www.example.com --headless
```

Add `--screenshot` to save a full-page PNG for every page discovered during that headless crawl:

```sh
site-scraper https://www.example.com --headless --screenshot
```

## Screenshots without a crawl

The `screenshot` subcommand captures one URL and does not mirror HTML or assets:

```sh
site-scraper screenshot https://www.example.com
```

For a batch, provide a UTF-8 text file with one URL per line. Blank lines and lines beginning with
`#` are ignored.

```text
# public pages
https://www.example.com/
https://www.example.com/about/
```

```sh
site-scraper screenshot --file urls.txt --output shots/ --concurrency 2
```

Every URL is validated before Chrome starts. Files use readable, deterministic names such as
`example.com--index.png` and `example.com--search--q-astro.png`; colliding names receive a stable
short hash. Existing unrelated screenshots in the output directory are kept.

<Callout type="caution">
  The screenshot subcommand and headless crawling require a build with the `headless` feature and
  a local Chrome or Chromium installation.
</Callout>
