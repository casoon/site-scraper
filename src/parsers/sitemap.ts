/**
 * Sitemap.xml parsing utilities
 */

import { hasBrowserSession } from "../network/challenge.js";
import { fetchWithRetry } from "../network/fetch.js";

/**
 * Discover URLs from sitemap.xml or sitemap_index.xml
 * Returns an array of URLs found in <loc> tags
 */
export async function discoverFromSitemap(root: URL): Promise<string[]> {
  // Skip sitemap discovery if we're using browser session (Cloudflare protected sites usually don't expose sitemaps)
  if (hasBrowserSession()) {
    return [];
  }

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
