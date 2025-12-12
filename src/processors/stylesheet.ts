/**
 * CSS stylesheet processing utilities
 */

import fs from 'node:fs/promises';
import path from 'node:path';
import { downloadBinary, fetchWithRetry } from '../network/fetch.js';
import { ensureDir } from '../utils/filesystem.js';
import { makeRelative, urlToLocalPath } from '../utils/url.js';

/**
 * Download a stylesheet and rewrite its url() assets to local files
 * Returns the local path where the CSS was saved
 */
export async function processStylesheet(
  root: URL,
  cssUrl: URL,
  outDir: string,
  allowExternalAssets: boolean,
): Promise<string> {
  const res = await fetchWithRetry(cssUrl.toString());
  const css = await res.text();

  // Determine the local path of the CSS file first (needed for relative path calculation)
  const cssPath = urlToLocalPath(root, cssUrl, outDir);

  const assetRe = /url\(([^)]+)\)/g; // naive but works for most cases
  const tasks: Array<Promise<boolean>> = [];
  const rewritten = css.replace(assetRe, (match, p1) => {
    const raw = String(p1)
      .trim()
      .replace(/^['"]|['"]$/g, '');
    if (!raw || raw.startsWith('data:')) return match; // leave data URIs
    try {
      const assetUrl = new URL(raw, cssUrl);
      const isExternal = assetUrl.origin !== root.origin;
      if (isExternal && !allowExternalAssets) return match; // keep as-is
      const localPath = urlToLocalPath(root, assetUrl, outDir);
      // Silent mode for images - they'll be replaced with placeholders anyway
      const isImage = /\.(png|jpe?g|gif|svg|webp|ico)$/i.test(localPath);
      tasks.push(downloadBinary(assetUrl.toString(), localPath, isImage));
      // Calculate relative path from CSS file to asset (not from HTML referer)
      const rel = makeRelative(cssPath, localPath);
      return `url(${rel})`;
    } catch {
      return match;
    }
  });
  await ensureDir(path.dirname(cssPath));
  await fs.writeFile(cssPath, rewritten, 'utf8');
  await Promise.all(tasks);
  return cssPath;
}
