/**
 * CSS stylesheet processing utilities
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fetchWithRetry, downloadBinary } from "../network/fetch.js";
import { urlToLocalPath, makeRelative } from "../utils/url.js";
import { ensureDir } from "../utils/filesystem.js";

/**
 * Download a stylesheet and rewrite its url() assets to local files
 * Returns the local path where the CSS was saved
 */
export async function processStylesheet(
  root: URL,
  cssUrl: URL,
  outDir: string,
  allowExternalAssets: boolean,
  refererFile: string,
): Promise<string> {
  const res = await fetchWithRetry(cssUrl.toString());
  const css = await res.text();

  const assetRe = /url\(([^)]+)\)/g; // naive but works for most cases
  const tasks: Array<Promise<void>> = [];
  let rewritten = css.replace(assetRe, (match, p1) => {
    const raw = String(p1)
      .trim()
      .replace(/^['"]|['"]$/g, "");
    if (!raw || raw.startsWith("data:")) return match; // leave data URIs
    try {
      const assetUrl = new URL(raw, cssUrl);
      const isExternal = assetUrl.origin !== root.origin;
      if (isExternal && !allowExternalAssets) return match; // keep as-is
      const localPath = urlToLocalPath(root, assetUrl, outDir);
      tasks.push(downloadBinary(assetUrl.toString(), localPath));
      const rel = makeRelative(refererFile, localPath);
      return `url(${rel})`;
    } catch {
      return match;
    }
  });

  const cssPath = urlToLocalPath(root, cssUrl, outDir);
  await ensureDir(path.dirname(cssPath));
  await fs.writeFile(cssPath, rewritten, "utf8");
  await Promise.all(tasks);
  return cssPath;
}
