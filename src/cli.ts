/**
 * CLI argument parsing and validation
 */

import fs from "node:fs/promises";
import path from "node:path";
import minimist from "minimist";
import { crawl } from "./crawler.js";
import {
  closeBrowser,
  hasBrowserSession,
  solveChallenge,
} from "./network/challenge.js";
import {
  configureRequests,
  fetchOnce,
  isCloudflareChallenge,
} from "./network/fetch.js";
import { ensureDir, safeFilename } from "./utils/filesystem.js";

/**
 * Parse CLI arguments and run the crawler
 */
export async function runCLI(): Promise<void> {
  const argv = minimist(process.argv.slice(2), {
    boolean: ["sitemap", "allowExternalAssets"],
    string: ["placeholder", "userAgent", "referer"],
    default: {
      maxDepth: 2,
      concurrency: 4,
      sitemap: true,
      allowExternalAssets: true,
      placeholder: "external",
      delayMs: 300,
    },
  });

  const [start] = argv._;
  if (!start) {
    console.error(
      "Usage: site-scraper <url> [--maxDepth 2] [--concurrency 4] [--delayMs 300] [--placeholder external|local] [--sitemap] [--allowExternalAssets] [--userAgent <string>] [--referer <url>]",
    );
    process.exit(1);
  }

  // Configure request settings
  const delayMs = Number(argv.delayMs ?? 300);
  configureRequests({
    delayMs: Number.isFinite(delayMs) && delayMs >= 0 ? delayMs : 300,
    userAgent: typeof argv.userAgent === "string" ? argv.userAgent : undefined,
    referer: typeof argv.referer === "string" ? argv.referer : undefined,
  });

  let startUrl: URL;
  try {
    startUrl = new URL(String(start));
  } catch {
    console.error("Invalid URL provided");
    process.exit(1);
  }

  // Check for Cloudflare challenge and solve if needed
  try {
    const testResponse = await fetchOnce(startUrl.toString());
    if (isCloudflareChallenge(testResponse)) {
      console.log("Cloudflare challenge detected. Opening browser...\n");
      const { cookies, userAgent } = await solveChallenge(startUrl.toString());
      configureRequests({ cookies, userAgent });
      console.log("Continuing with authenticated session.\n");
    }
  } catch (err) {
    // If test fetch fails, try to solve challenge anyway
    console.log("Initial request failed. Attempting browser-based access...\n");
    try {
      const { cookies, userAgent } = await solveChallenge(startUrl.toString());
      configureRequests({ cookies, userAgent });
      console.log("Continuing with authenticated session.\n");
    } catch {
      console.warn("Could not solve challenge. Proceeding anyway...");
    }
  }

  if (typeof (argv as Record<string, unknown>).out !== "undefined") {
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
  // Force concurrency to 1 when using browser session (Puppeteer can only handle sequential navigation)
  let concurrency = Number(argv.concurrency ?? 8);
  if (hasBrowserSession() && concurrency > 1) {
    console.log("Note: Using concurrency=1 for browser-based scraping.\n");
    concurrency = 1;
  }
  const placeholder =
    (argv.placeholder as string) === "local" ? "local" : "external";

  await fs.rm(outDir, { recursive: true, force: true });
  await ensureDir(outDir);

  try {
    await crawl(startUrl.toString(), outDir, {
      maxDepth,
      concurrency,
      sitemap: Boolean(argv.sitemap),
      allowExternalAssets: Boolean(argv.allowExternalAssets),
      placeholder,
    });
  } finally {
    // Clean up browser if it was used
    await closeBrowser();
  }
}
