/**
 * Main crawling logic
 */

import path from 'node:path';
import * as cheerio from 'cheerio';
import pLimit from 'p-limit';
import { fetchWithRetry } from './network/fetch.js';
import { extractLinks } from './parsers/links.js';
import { discoverFromSitemap } from './parsers/sitemap.js';
import { rewriteAndSaveHTML } from './processors/html.js';

export interface CrawlOptions {
  maxDepth: number;
  concurrency: number;
  sitemap: boolean;
  allowExternalAssets: boolean;
  placeholder: 'external' | 'local';
}

/**
 * Main crawl function
 * Recursively crawls a website, downloading HTML and assets
 */
export async function crawl(
  startUrl: string,
  outDir: string,
  options: CrawlOptions,
): Promise<void> {
  const root = new URL(startUrl);
  const limit = pLimit(options.concurrency);

  const toVisit: Array<{ url: URL; depth: number }> = [{ url: root, depth: 0 }];

  // Optionally seed from sitemap
  if (options.sitemap) {
    const seeds = await discoverFromSitemap(root);
    for (const s of seeds) {
      try {
        const url = new URL(s);
        if (url.origin === root.origin) toVisit.push({ url, depth: 1 });
      } catch {
        /* ignore invalid URLs */
      }
    }
  }

  const seen = new Set<string>();

  async function processPage(url: URL, depth: number): Promise<void> {
    const key = url.toString().split('#')[0];
    if (seen.has(key)) return;
    seen.add(key);

    try {
      const res = await fetchWithRetry(key);
      const ct = res.headers.get('content-type') || '';
      if (!ct.includes('text/html')) return; // ignore non-HTML

      const html = await res.text();

      const outFile = await rewriteAndSaveHTML(new URL(startUrl), url, html, outDir, {
        allowExternalAssets: options.allowExternalAssets,
        placeholder: options.placeholder,
      });

      // Extract further links
      const $ = cheerio.load(html);
      if (depth < options.maxDepth) {
        const links = extractLinks($, new URL(startUrl), url);
        for (const link of links) {
          const lk = link.toString().split('#')[0];
          if (!seen.has(lk)) toVisit.push({ url: link, depth: depth + 1 });
        }
      }
      console.log(`Saved: ${url} -> ${path.relative(outDir, outFile)}`);
    } catch (err) {
      const message = err instanceof Error && err.message ? err.message : String(err);
      console.warn(`Skipping ${key}: ${message}`);
    }
  }

  while (toVisit.length) {
    const batch = toVisit.splice(0, options.concurrency);
    await Promise.all(batch.map((item) => limit(() => processPage(item.url, item.depth))));
  }
}
