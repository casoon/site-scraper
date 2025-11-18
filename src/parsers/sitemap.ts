/**
 * Sitemap.xml parsing utilities
 */

import { fetchWithRetry } from "../network/fetch.js";

/**
 * Discover URLs from sitemap.xml or sitemap_index.xml
 * Returns an array of URLs found in <loc> tags
 */
export async function discoverFromSitemap(root: URL): Promise<string[]> {
  const candidates = [
    new URL("/sitemap.xml", root).toString(),
    new URL("/sitemap_index.xml", root).toString(),
  ];
  const found: string[] = [];
  for (const url of candidates) {
    try {
      const res = await fetchWithRetry(url);
      const xml = await res.text();
      const locs = Array.from(xml.matchAll(/<loc>([^<]+)<\/loc>/g)).map(
        (m) => m[1],
      );
      if (locs.length) found.push(...locs);
    } catch {
      /* ignore missing sitemaps */
    }
  }
  return found;
}
