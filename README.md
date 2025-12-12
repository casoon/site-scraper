# Site Scraper

[![CI](https://github.com/casoon/site-scraper/actions/workflows/ci.yml/badge.svg)](https://github.com/casoon/site-scraper/actions/workflows/ci.yml)

A small Node.js CLI tool that creates static copies of websites. It crawls from a starting URL, saves HTML files along with stylesheets and scripts locally, and replaces images with placeholders based on configuration.

## Prerequisites

- Node.js >= 24 (for native `fetch` support)
- pnpm as package manager (npm or yarn work as well, but the commands below are for pnpm)

## Installation

```sh
pnpm install
```

## Usage

```sh
pnpm run dev <URL> [--maxDepth 2] [--concurrency 8] [--placeholder external|local] [--sitemap] [--allowExternalAssets]
```

Example:

```sh
pnpm run dev https://www.example.com --maxDepth 2 --placeholder local
```

### Output

- All results are automatically saved to `./output/<domain>`.
- If the folder already exists, it will be deleted and recreated before the run.
- HTML files are stored in a folder structure matching the URL paths.
- Assets (CSS/JS/Fonts) are downloaded and internal references are rewritten.
- Images can be replaced with external placeholders (`external`) or locally generated PNGs (`local`, optionally requires `sharp`).

### Options

- `--maxDepth`: Maximum crawl depth relative to the start page (default: `2`).
- `--concurrency`: Number of parallel downloads (default: `8`).
- `--sitemap`: When set (default: `true`), entries from `/sitemap.xml` or `/sitemap_index.xml` are also used as starting points.
- `--allowExternalAssets`: When `false`, external CSS/JS/assets are not downloaded (default: `true`).

## Build

To create a compiled output in `dist/`:

```sh
pnpm run build
```

## Linting & Formatting

This project uses [Biome](https://biomejs.dev/) for linting and formatting:

```sh
pnpm run check      # Check lint + format
pnpm run check:fix  # Auto-fix issues
pnpm run lint       # Lint only
pnpm run format     # Format only
```

## License

This project is licensed under the [MIT License](LICENSE).
