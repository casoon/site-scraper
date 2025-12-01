/**
 * HTML rewriting and processing utilities
 */

import fs from "node:fs/promises";
import path from "node:path";
import * as cheerio from "cheerio";
import { processStylesheet } from "./stylesheet.js";
import { getPlaceholderForImage } from "./image.js";
import { downloadBinary } from "../network/fetch.js";
import { urlToLocalPath, makeRelative } from "../utils/url.js";
import { ensureDir } from "../utils/filesystem.js";

export interface RewriteOptions {
  allowExternalAssets: boolean;
  placeholder: "external" | "local";
}

/**
 * Rewrite HTML: links, styles, scripts, images
 * Downloads assets and rewrites all references to local relative paths
 */
export async function rewriteAndSaveHTML(
  root: URL,
  pageUrl: URL,
  html: string,
  outDir: string,
  opts: RewriteOptions,
): Promise<string> {
  const $ = cheerio.load(html);

  // Stylesheets: download and rewrite
  const cssTasks: Promise<void>[] = [];
  $('link[rel="stylesheet"][href]').each((_, el) => {
    const href = $(el).attr("href")!;
    try {
      const url = new URL(href, pageUrl);
      const isExternal = url.origin !== root.origin;
      if (isExternal && !opts.allowExternalAssets) return; // leave as-is
      cssTasks.push(
        (async () => {
          const cssPath = await processStylesheet(
            root,
            url,
            outDir,
            opts.allowExternalAssets,
          );
          const rel = makeRelative(
            urlToLocalPath(root, pageUrl, outDir),
            cssPath,
          );
          $(el).attr("href", rel);
        })(),
      );
    } catch {
      /* ignore invalid URLs */
    }
  });

  // Scripts: download same-origin; optionally leave externals
  const jsTasks: Promise<void>[] = [];
  $("script[src]").each((_, el) => {
    const src = $(el).attr("src")!;
    try {
      const url = new URL(src, pageUrl);
      const isExternal = url.origin !== root.origin;
      if (isExternal && !opts.allowExternalAssets) return;
      jsTasks.push(
        (async () => {
          const jsPath = urlToLocalPath(root, url, outDir, ".js");
          await downloadBinary(url.toString(), jsPath);
          const rel = makeRelative(
            urlToLocalPath(root, pageUrl, outDir),
            jsPath,
          );
          $(el).attr("src", rel);
        })(),
      );
    } catch {
      /* ignore invalid URLs */
    }
  });

  // Images: replace with placeholders (external or local)
  const imgTasks: Promise<void>[] = [];
  $("img[src]").each((_, el) => {
    const $el = $(el);
    const src = $el.attr("src")!;
    try {
      const url = new URL(src, pageUrl);
      imgTasks.push(
        (async () => {
          const ph = await getPlaceholderForImage(
            url,
            opts.placeholder,
            outDir,
            root,
          );
          if (opts.placeholder === "local" && ph.src) {
            // ph.src is absolute path, convert to relative
            const rel = makeRelative(
              urlToLocalPath(root, pageUrl, outDir),
              ph.src,
            );
            $el.attr("src", rel);
          } else {
            $el.attr("src", ph.src);
          }
          if (ph.width && !$el.attr("width"))
            $el.attr("width", String(ph.width));
          if (ph.height && !$el.attr("height"))
            $el.attr("height", String(ph.height));
          // remove srcset to avoid unexpected fetches
          $el.removeAttr("srcset");
        })(),
      );
    } catch {
      /* ignore invalid URLs */
    }
  });

  await Promise.all([...cssTasks, ...jsTasks, ...imgTasks]);

  // Rewrite internal anchor hrefs to local files
  $("a[href]").each((_, el) => {
    const $el = $(el);
    const href = ($el.attr("href") || "").trim();
    try {
      const url = new URL(href, pageUrl);
      if (url.origin !== root.origin) return; // leave externals
      const targetPath = urlToLocalPath(root, url, outDir);
      const rel = makeRelative(
        urlToLocalPath(root, pageUrl, outDir),
        targetPath,
      );
      $el.attr("href", rel);
    } catch {
      /* ignore invalid URLs */
    }
  });

  const outFile = urlToLocalPath(root, pageUrl, outDir);
  await ensureDir(path.dirname(outFile));
  await fs.writeFile(outFile, $.html(), "utf8");
  return outFile;
}
