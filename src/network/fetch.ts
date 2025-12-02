/**
 * Network fetch utilities with retry logic
 */

import { setTimeout as delay } from "node:timers/promises";
import fs from "node:fs/promises";
import { ensureDir } from "../utils/filesystem.js";

const DEFAULT_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";

const BASE_REQUEST_HEADERS: Record<string, string> = {
  "User-Agent": DEFAULT_USER_AGENT,
  Accept:
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
  "Accept-Language": "en-US,en;q=0.9",
  "Cache-Control": "no-cache",
};

let requestDelayMs = 0;
let requestHeaders: Record<string, string> = { ...BASE_REQUEST_HEADERS };

/**
 * Configure request settings
 */
export function configureRequests(options: {
  delayMs?: number;
  userAgent?: string;
}): void {
  if (options.delayMs !== undefined && options.delayMs >= 0) {
    requestDelayMs = options.delayMs;
  }
  if (options.userAgent) {
    requestHeaders = {
      ...BASE_REQUEST_HEADERS,
      "User-Agent": options.userAgent,
    };
  }
}

/**
 * Fetch with basic retry and exponential backoff
 */
export async function fetchWithRetry(
  url: string,
  tries = 3,
  backoffMs = 400,
): Promise<Response> {
  let lastErr: any;
  for (let i = 0; i < tries; i++) {
    try {
      if (requestDelayMs > 0) {
        const jitter = Math.floor(
          Math.random() * Math.max(1, requestDelayMs * 0.2),
        );
        await delay(requestDelayMs + jitter);
      }
      const res = await fetch(url, {
        redirect: "follow",
        headers: requestHeaders,
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      return res;
    } catch (e) {
      lastErr = e;
      if (i < tries - 1) await delay(backoffMs * Math.pow(2, i));
    }
  }
  throw lastErr;
}

/**
 * Download binary asset (image/font/js etc.) to a local file
 * Returns true if successful, false if the asset could not be downloaded
 */
export async function downloadBinary(
  url: string,
  dest: string,
): Promise<boolean> {
  try {
    await ensureDir(dest.substring(0, dest.lastIndexOf("/")));
    const res = await fetchWithRetry(url);
    const buf = Buffer.from(await res.arrayBuffer());
    await fs.writeFile(dest, buf);
    return true;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.warn(`Skipping asset ${url}: ${msg}`);
    return false;
  }
}
