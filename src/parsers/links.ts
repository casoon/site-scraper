/**
 * HTML link extraction utilities
 */

import type * as cheerio from "cheerio";

/**
 * Extract absolute URLs from <a href> for same-origin crawling
 * Filters out mailto:, tel:, javascript: links
 */
export function extractLinks(
  $: cheerio.CheerioAPI,
  root: URL,
  docUrl: URL,
): URL[] {
  const urls = new Set<string>();
  $("a[href]").each((_, el) => {
    const href = ($(el).attr("href") || "").trim();
    if (
      !href ||
      href.startsWith("mailto:") ||
      href.startsWith("tel:") ||
      href.startsWith("javascript:")
    )
      return;
    try {
      const url = new URL(href, docUrl);
      if (url.origin === root.origin) urls.add(url.toString().split("#")[0]);
    } catch {
      /* ignore bad URLs */
    }
  });
  return Array.from(urls).map((u) => new URL(u));
}
