# Changelog

All notable changes to this project are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/), versions match Git tags and the
`version` field in `Cargo.toml`.

## Unreleased

## [1.4.6] - 2026-09-15

- Security: update `rustls` to 0.23.45 (RUSTSEC-2026-0285)
- Sitemap: also read `Sitemap:` entries from `robots.txt` and `/wp-sitemap.xml`, unpack gzipped sitemaps and decode XML entities in `<loc>`
- Headless: treat an HTTP error with an empty body as its status, so stale sitemap entries only warn
- Update `scraper` to 0.27, `rand` to 0.9 and `anyhow` to 1.0.104 (clears the `rand`/`fxhash` cargo-audit warnings)
- Update `runemark` to 0.3.3: status lines are no longer lost when `TERM` is unset or `dumb`
- Pages: show real crawl output of casoon.de on the project site

## [1.4.5] - 2026-09-12

- Screenshot: capture pages with an HTTP error status (e.g. custom 404) instead of failing
- Headless: raise the default browser navigation timeout to 60s and make it configurable with `--timeout`
- Headless: default to a 1440px desktop viewport; add `--device desktop|tablet|mobile`
- Update README: document headless mode, extended interactive setup, new options

## [1.4.4] - 2026-05-02

- Headless: wait 800ms after load before scrolling

## [1.4.3] - 2026-05-02

- Headless: strip `<script>` tags before saving rendered HTML

## [1.4.2] - 2026-05-02

- Revert canvas freeze: `toDataURL` fails on WebGL/tainted canvases

## [1.4.1] - 2026-05-02

- Headless: freeze canvas elements as static images before capture

## [1.4.0] - 2026-05-02

- Headless: capture HTML while scrolled to preserve scroll-driven styles

## [1.3.9] - 2026-05-02

- Headless: only reveal `opacity-0` elements that also have a translate class

## [1.3.8] - 2026-05-02

- Headless: remove `opacity-0` and paired translate classes before capture

## [1.3.7] - 2026-05-02

- Headless: scroll page after load to trigger IntersectionObserver animations

## [1.3.6] - 2026-05-02

- `install.sh`: fail loudly on download error instead of extracting HTML
- Build release binaries with `--features headless` so `--headless` works out of the box

## [1.3.5] - 2026-05-02

- Add extended interactive setup mode

## [1.3.4] - 2026-05-02

- Add `--headless` mode with optional `chromiumoxide` feature

## [1.3.3] - 2026-04-29

- Resolve redirect before crawling so www → non-www sites are fully crawled
- Update README: document interactive mode and prompt behaviour

## [1.3.2] - 2026-04-29

- Switch interactive prompts to English

## [1.3.1] - 2026-04-29

- Internal fixes, no user-facing change

## [1.3.0] - 2026-04-29

- Add interactive mode with `dialoguer` prompts (depth, images, bot) when only a URL is given

## [1.2.5] - 2026-04-27

- Fix `--placeholder real` being silently ignored (was mapped to `external` in `cli.rs`)

## [1.2.4] - 2026-04-27

- Handle lazy-loaded images (`data-src`/`data-lazy-src`) and skip data URI placeholders

## [1.2.3] - 2026-04-27

- Map query-string URLs to unique filenames (fixes WordPress `?page_id=` sites)
- Opt into Node.js 24 for GitHub Actions to silence deprecation warnings

## [1.2.2] - 2026-04-27

- Change default install dir to `~/.local/bin` to avoid sudo prompt

## [1.2.1] - 2026-04-27

- Add `--placeholder real` mode to download original images
- Document `--placeholder real` in README
- Fix formatting and clippy warnings

## [1.2.0] - 2026-04-07

- Rewrite from TypeScript to Rust
- Refactor codebase into modular structure
- Add request headers/delay config and graceful asset error handling
- Add Puppeteer support for Cloudflare-protected sites
- Fix CSS `url()` path calculation — use CSS file path instead of HTML referer
- Add release workflow for automated binary builds
- Switch to `rustls-tls` and use `cross` for aarch64-linux builds
- Add Biome for linting/formatting and GitHub Actions CI
- Translate README to English
