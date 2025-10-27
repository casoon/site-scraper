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

import fs from "node:fs/promises";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import minimist from "minimist";
import * as cheerio from "cheerio";
import pLimit from "p-limit";
import probe from "probe-image-size";

// Attempt to import sharp lazily (optional)
let sharp: any = null;
(async () => {
  try {
    sharp = (await import("sharp")).default;
  } catch {
    /* optional */
  }
})();

/** Small helpers */
const ensureDir = async (dir: string) => fs.mkdir(dir, { recursive: true });

const safeFilename = (s: string) =>
  s
    .replace(/[^a-zA-Z0-9._-]/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");

// Map a URL to a local file path inside outDir
function urlToLocalPath(
  root: URL,
  target: URL,
  outDir: string,
  extHint?: string,
): string {
  if (target.origin !== root.origin) {
    // external: keep hostname as subfolder
    const hostDir = path.join(outDir, safeFilename(target.host));
    const pathname = target.pathname.endsWith("/")
      ? `${target.pathname}index`
      : target.pathname;
    const withExt = pathname.match(/\.[a-z0-9]+$/i)
      ? pathname
      : `${pathname}${extHint ?? ""}`;
    return path.join(hostDir, withExt).split("?")[0].split("#")[0];
  }
  // same-origin
  let p = target.pathname;
  if (p.endsWith("/")) p = path.join(p, "index.html");
  else if (!p.match(/\.[a-z0-9]+$/i)) p = `${p}.html`;
  const file = path.join(outDir, p).split("?")[0].split("#")[0];
  return file;
}

function makeRelative(fromFile: string, toFile: string) {
  let rel = path.relative(path.dirname(fromFile), toFile);
  if (!rel.startsWith(".")) rel = `./${rel}`;
  return rel.replace(/\\/g, "/");
}

/** Fetch with basic retry/backoff */
async function fetchWithRetry(
  u: string,
  tries = 3,
  backoffMs = 400,
): Promise<Response> {
  let lastErr: any;
  for (let i = 0; i < tries; i++) {
    try {
      const res = await fetch(u, { redirect: "follow" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return res;
    } catch (e) {
      lastErr = e;
      if (i < tries - 1) await delay(backoffMs * Math.pow(2, i));
    }
  }
  throw lastErr;
}

/** Parse <loc> from a sitemap.xml */
async function discoverFromSitemap(root: URL): Promise<string[]> {
  const candidates = [
    new URL("/sitemap.xml", root).toString(),
    new URL("/sitemap_index.xml", root).toString(),
  ];
  const found: string[] = [];
  for (const u of candidates) {
    try {
      const res = await fetchWithRetry(u);
      const xml = await res.text();
      const locs = Array.from(xml.matchAll(/<loc>([^<]+)<\/loc>/g)).map(
        (m) => m[1],
      );
      if (locs.length) found.push(...locs);
    } catch {
      /* ignore */
    }
  }
  return found;
}

/** Extract absolute URLs from <a href> for same-origin crawling */
function extractLinks($: cheerio.CheerioAPI, root: URL, docUrl: URL): URL[] {
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
      const u = new URL(href, docUrl);
      if (u.origin === root.origin) urls.add(u.toString().split("#")[0]);
    } catch {
      /* ignore bad URL */
    }
  });
  return Array.from(urls).map((u) => new URL(u));
}

/** Download a stylesheet and rewrite its url() assets to local files. */
async function processStylesheet(
  root: URL,
  cssUrl: URL,
  outDir: string,
  allowExternalAssets: boolean,
  refererFile: string,
) {
  const res = await fetchWithRetry(cssUrl.toString());
  const css = await res.text();

  const assetRe = /url\(([^)]+)\)/g; // naive but works for most cases
  const tasks: Array<Promise<void>> = [];
  let rewritten = css.replace(assetRe, (m, p1) => {
    const raw = String(p1)
      .trim()
      .replace(/^['"]|['"]$/g, "");
    if (!raw || raw.startsWith("data:")) return m; // leave data URIs
    try {
      const assetUrl = new URL(raw, cssUrl);
      const isExternal = assetUrl.origin !== root.origin;
      if (isExternal && !allowExternalAssets) return m; // keep as-is
      const localPath = urlToLocalPath(root, assetUrl, outDir);
      tasks.push(downloadBinary(assetUrl.toString(), localPath));
      const rel = makeRelative(refererFile, localPath);
      return `url(${rel})`;
    } catch {
      return m;
    }
  });

  const cssPath = urlToLocalPath(root, cssUrl, outDir);
  await ensureDir(path.dirname(cssPath));
  await fs.writeFile(cssPath, rewritten, "utf8");
  await Promise.all(tasks);
  return cssPath;
}

/** Download binary asset (image/font/js etc.) */
async function downloadBinary(url: string, dest: string) {
  await ensureDir(path.dirname(dest));
  const res = await fetchWithRetry(url);
  const buf = Buffer.from(await res.arrayBuffer());
  await fs.writeFile(dest, buf);
}

/**
 * Decide placeholder URL or create local placeholder for an image.
 */
async function getPlaceholderForImage(
  imgUrl: URL,
  strategy: "external" | "local",
  outDir: string,
  root: URL,
): Promise<{ src: string; width?: number; height?: number }> {
  // Try to quickly probe dimensions without full download
  let width: number | undefined;
  let height: number | undefined;
  try {
    const r = await fetchWithRetry(imgUrl.toString());
    const stream = r.body as any; // web stream -> node readable via experimental; probe supports WHATWG streams
    const meta = await probe(stream);
    width = meta.width;
    height = meta.height;
  } catch {
    // ignore; fallback to 800x450
    width = 800;
    height = 450;
  }

  if (strategy === "local" && sharp) {
    const ext = ".png";
    const localPath = urlToLocalPath(root, imgUrl, outDir, ext);
    await ensureDir(path.dirname(localPath));
    const w = Math.max(1, Math.min(4096, width ?? 800));
    const h = Math.max(1, Math.min(4096, height ?? 450));
    // simple gray placeholder with centered size text
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="#e5e7eb"/><text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="ui-sans-serif,system-ui" font-size="${Math.max(10, Math.floor(Math.min(w, h) / 6))}" fill="#6b7280">${w}×${h}</text></svg>`;
    const png = await sharp(Buffer.from(svg)).png().toBuffer();
    await fs.writeFile(localPath, png);
    return {
      src: makeRelative(localPath, localPath) /* replaced later */,
      width: w,
      height: h,
    };
  }

  // external
  const w = width ?? 800;
  const h = height ?? 450;
  const placeholder = `https://placehold.co/${w}x${h}`;
  return { src: placeholder, width: w, height: h };
}

/** Rewrite HTML: links, styles, scripts, images */
async function rewriteAndSaveHTML(
  root: URL,
  pageUrl: URL,
  html: string,
  outDir: string,
  opts: { allowExternalAssets: boolean; placeholder: "external" | "local" },
) {
  const $ = cheerio.load(html);

  // Stylesheets: download and rewrite
  const cssTasks: Promise<void>[] = [];
  $('link[rel="stylesheet"][href]').each((_, el) => {
    const href = $(el).attr("href")!;
    try {
      const u = new URL(href, pageUrl);
      const isExternal = u.origin !== root.origin;
      if (isExternal && !opts.allowExternalAssets) return; // leave as-is
      cssTasks.push(
        (async () => {
          const cssPath = await processStylesheet(
            root,
            u,
            outDir,
            opts.allowExternalAssets,
            urlToLocalPath(root, pageUrl, outDir),
          );
          const rel = makeRelative(
            urlToLocalPath(root, pageUrl, outDir),
            cssPath,
          );
          $(el).attr("href", rel);
        })(),
      );
    } catch {
      /* ignore */
    }
  });

  // Scripts: download same-origin; optionally leave externals
  const jsTasks: Promise<void>[] = [];
  $("script[src]").each((_, el) => {
    const src = $(el).attr("src")!;
    try {
      const u = new URL(src, pageUrl);
      const isExternal = u.origin !== root.origin;
      if (isExternal && !opts.allowExternalAssets) return;
      jsTasks.push(
        (async () => {
          const jsPath = urlToLocalPath(root, u, outDir, ".js");
          await downloadBinary(u.toString(), jsPath);
          const rel = makeRelative(
            urlToLocalPath(root, pageUrl, outDir),
            jsPath,
          );
          $(el).attr("src", rel);
        })(),
      );
    } catch {
      /* ignore */
    }
  });

  // Images: replace with placeholders (external or local)
  const imgTasks: Promise<void>[] = [];
  $("img[src]").each((_, el) => {
    const $el = $(el);
    const src = $el.attr("src")!;
    try {
      const u = new URL(src, pageUrl);
      imgTasks.push(
        (async () => {
          const ph = await getPlaceholderForImage(
            u,
            opts.placeholder,
            outDir,
            root,
          );
          if (opts.placeholder === "local" && sharp) {
            // When local, we already know the target file path used in getPlaceholderForImage.
            // It returned a rel based on itself (placeholder); recompute now that we know page path
            const localAssetPath = urlToLocalPath(root, u, outDir, ".png");
            const rel = makeRelative(
              urlToLocalPath(root, pageUrl, outDir),
              localAssetPath,
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
      /* ignore */
    }
  });

  await Promise.all([...cssTasks, ...jsTasks, ...imgTasks]);

  // Rewrite internal anchor hrefs to local files
  $("a[href]").each((_, el) => {
    const $el = $(el);
    const href = ($el.attr("href") || "").trim();
    try {
      const u = new URL(href, pageUrl);
      if (u.origin !== root.origin) return; // leave externals
      const targetPath = urlToLocalPath(root, u, outDir);
      const rel = makeRelative(
        urlToLocalPath(root, pageUrl, outDir),
        targetPath,
      );
      $el.attr("href", rel);
    } catch {
      /* ignore */
    }
  });

  const outFile = urlToLocalPath(root, pageUrl, outDir);
  await ensureDir(path.dirname(outFile));
  await fs.writeFile(outFile, $.html(), "utf8");
  return outFile;
}

/** Main crawl */
async function crawl(
  startUrl: string,
  outDir: string,
  options: {
    maxDepth: number;
    concurrency: number;
    sitemap: boolean;
    allowExternalAssets: boolean;
    placeholder: "external" | "local";
  },
) {
  const root = new URL(startUrl);
  const limit = pLimit(options.concurrency);

  const toVisit: Array<{ url: URL; depth: number }> = [{ url: root, depth: 0 }];

  if (options.sitemap) {
    const seeds = await discoverFromSitemap(root);
    for (const s of seeds) {
      try {
        const u = new URL(s);
        if (u.origin === root.origin) toVisit.push({ url: u, depth: 1 });
      } catch {
        /* ignore */
      }
    }
  }

  const seen = new Set<string>();

  async function processPage(u: URL, depth: number) {
    const key = u.toString().split("#")[0];
    if (seen.has(key)) return;
    seen.add(key);

    try {
      const res = await fetchWithRetry(key);
      const ct = res.headers.get("content-type") || "";
      if (!ct.includes("text/html")) return; // ignore non-HTML
      const html = await res.text();

      const outFile = await rewriteAndSaveHTML(
        new URL(startUrl),
        u,
        html,
        outDir,
        {
          allowExternalAssets: options.allowExternalAssets,
          placeholder: options.placeholder,
        },
      );
      // Extract further links
      const $ = cheerio.load(html);
      if (depth < options.maxDepth) {
        const links = extractLinks($, new URL(startUrl), u);
        for (const l of links) {
          const lk = l.toString().split("#")[0];
          if (!seen.has(lk)) toVisit.push({ url: l, depth: depth + 1 });
        }
      }
      console.log(`Saved: ${u} -> ${path.relative(outDir, outFile)}`);
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : String(err);
      console.warn(`Skipping ${key}: ${message}`);
    }
  }

  while (toVisit.length) {
    const batch = toVisit.splice(0, options.concurrency);
    await Promise.all(
      batch.map((item) => limit(() => processPage(item.url, item.depth))),
    );
  }
}

/** CLI */
async function main() {
  const argv = minimist(process.argv.slice(2), {
    boolean: ["sitemap", "allowExternalAssets"],
    string: ["placeholder"],
    default: {
      maxDepth: 2,
      concurrency: 8,
      sitemap: true,
      allowExternalAssets: true,
      placeholder: "external",
    },
  });

  const [start] = argv._;
  if (!start) {
    console.error(
      "Usage: site-scraper <url> [--maxDepth 2] [--concurrency 8] [--placeholder external|local] [--sitemap] [--allowExternalAssets]",
    );
    process.exit(1);
  }
  let startUrl: URL;
  try {
    startUrl = new URL(String(start));
  } catch {
    console.error("Invalid URL provided");
    process.exit(1);
  }
  if (typeof (argv as any).out !== "undefined") {
    console.warn(
      "The --out option is ignored; output is always written to ./output/<domain>.",
    );
  }
  const requestedOut = safeFilename(startUrl.host);
  if (!requestedOut) {
    console.error("Unable to derive output directory name");
    process.exit(1);
  }
  const baseOutput = path.resolve(process.cwd(), "output");
  await ensureDir(baseOutput);
  const outDir = path.join(baseOutput, requestedOut);
  const maxDepth = Number(argv.maxDepth ?? 2);
  const concurrency = Number(argv.concurrency ?? 8);
  const placeholder =
    (argv.placeholder as string) === "local" ? "local" : "external";

  await fs.rm(outDir, { recursive: true, force: true });
  await ensureDir(outDir);
  await crawl(startUrl.toString(), outDir, {
    maxDepth,
    concurrency,
    sitemap: Boolean(argv.sitemap),
    allowExternalAssets: Boolean(argv.allowExternalAssets),
    placeholder,
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
