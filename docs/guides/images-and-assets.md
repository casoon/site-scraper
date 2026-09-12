---
title: Images and assets
description: Control image handling, external styles and scripts, sitemap seeds, and crawl pacing.
order: 2
---

## Image strategies

`--placeholder` accepts three values:

| Value | Behaviour |
| --- | --- |
| `real` | Download original images and rewrite their references locally. |
| `local` | Generate local gray PNG placeholders. |
| `external` | Replace images with placehold.co URLs. This is the non-interactive default. |

```sh
site-scraper https://www.example.com --placeholder local
```

## External CSS and JavaScript

External stylesheets and scripts are downloaded by default. Preserve their remote references with
`--no-allow-external-assets`.

## Sitemap discovery

`sitemap.xml` URLs are included as seeds by default. Use `--no-sitemap` to crawl only from links
discovered below the starting URL.

## Pacing

The default is four concurrent downloads with a 300 ms request delay. Tune both explicitly when a
site or server needs a different load profile:

```sh
site-scraper https://www.example.com --concurrency 2 --delay-ms 750
```
