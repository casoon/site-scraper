#!/usr/bin/env node
/**
 * site-scraper
 *
 * A TypeScript CLI to mirror a website (HTML + CSS) to a local folder.
 * - Recursively crawls same-origin links (with optional sitemap.xml seeding)
 * - Saves pages in a hierarchical directory structure (path/to/page/index.html)
 * - Downloads CSS and rewrites url() assets; optionally pulls external assets
 * - Rewrites internal links/scripts/styles to local relative paths
 * - Replaces <img> with placeholders that match the original dimensions
 *   - Strategy "external": swap to a remote placeholder service (placehold.co)
 *   - Strategy "local": generate local placeholder images (PNG) via sharp (optional)
 *     (If you pick "local" but do not install sharp, the script will fall back to external)
 *
 * Usage:
 *   npm run dev -- <url> [--maxDepth 2] [--concurrency 8]
 *     [--placeholder external|local] [--sitemap] [--allowExternalAssets]
 *
 * Node >= 18 recommended (for global fetch).
 */

import { runCLI } from './cli.js';

runCLI().catch((err) => {
  console.error(err);
  process.exit(1);
});
