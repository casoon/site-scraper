---
title: Quickstart
description: Mirror a first website and choose between interactive setup and explicit flags.
order: 2
---

## Interactive setup

Pass only a URL in an interactive terminal:

```sh
site-scraper https://www.example.com
```

site-scraper asks for a quick or extended setup, then collects the relevant crawl depth, image
strategy and identity or rendering mode.

## Scripted crawl

Pass any option to skip the prompts. This makes commands predictable in scripts and CI:

```sh
site-scraper https://www.example.com \
  --max-depth 2 \
  --placeholder real
```

The command follows internal links to depth 2, downloads original images, and writes the result
below `output/www.example.com/`.

## Inspect the result

Open `output/<domain>/index.html` in a browser. URL paths become directories and files, while
downloaded asset references are rewritten to work locally.

<Callout type="caution" title="The crawl output is replaced">
  A new crawl recreates the output directory for that domain. Move or copy a snapshot before
  crawling the same domain again if you need to retain both versions.
</Callout>

Next, compare the [crawl modes](../../guides/crawl-modes/) or review the complete
[CLI reference](../../reference/cli/).
