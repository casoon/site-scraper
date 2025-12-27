/**
 * Network fetch utilities with retry logic
 */

import fs from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";
import { ensureDir } from "../utils/filesystem.js";

const DEFAULT_USER_AGENT =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const BASE_REQUEST_HEADERS: Record<string, string> = {
  "User-Agent": DEFAULT_USER_AGENT,
  Accept:
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
  "Accept-Language": "en-US,en;q=0.9,de;q=0.8",
  "Accept-Encoding": "gzip, deflate, br",
  "Cache-Control": "no-cache",
  Pragma: "no-cache",
  "Sec-Ch-Ua":
    '"Google Chrome";v="131", "Chromium";v="131", "Not_A Brand";v="24"',
  "Sec-Ch-Ua-Mobile": "?0",
  "Sec-Ch-Ua-Platform": '"macOS"',
  "Sec-Fetch-Dest": "document",
  "Sec-Fetch-Mode": "navigate",
  "Sec-Fetch-Site": "none",
  "Sec-Fetch-User": "?1",
  "Upgrade-Insecure-Requests": "1",
  Connection: "keep-alive",
};

let requestDelayMs = 0;
let requestHeaders: Record<string, string> = { ...BASE_REQUEST_HEADERS };
let baseReferer: string | null = null;

/**
 * Configure request settings
 */
export function configureRequests(options: {
  delayMs?: number;
  userAgent?: string;
  referer?: string;
  cookies?: string;
}): void {
  if (options.delayMs !== undefined && options.delayMs >= 0) {
    requestDelayMs = options.delayMs;
  }
  if (options.userAgent) {
    // Extract Chrome version from User-Agent to update Sec-Ch-Ua headers
    const chromeMatch = options.userAgent.match(/Chrome\/(\d+)/);
    const chromeVersion = chromeMatch ? chromeMatch[1] : "131";

    requestHeaders = {
      ...requestHeaders,
      "User-Agent": options.userAgent,
      "Sec-Ch-Ua": `"Google Chrome";v="${chromeVersion}", "Chromium";v="${chromeVersion}", "Not_A Brand";v="24"`,
    };
  }
  if (options.referer) {
    baseReferer = options.referer;
  }
  if (options.cookies) {
    requestHeaders = {
      ...requestHeaders,
      Cookie: options.cookies,
    };
  }
}

/**
 * Build headers for a specific request, including dynamic Referer
 */
function buildRequestHeaders(url: string): Record<string, string> {
  const headers = { ...requestHeaders };

  // Set Referer based on the URL being fetched
  if (baseReferer) {
    headers.Referer = baseReferer;
    headers.Origin = new URL(baseReferer).origin;
    headers["Sec-Fetch-Site"] = "same-origin";
  } else {
    // Use the target URL's origin as referer for first request
    try {
      const urlObj = new URL(url);
      headers.Referer = urlObj.origin + "/";
      headers.Origin = urlObj.origin;
    } catch {
      // Keep default headers if URL parsing fails
    }
  }

  return headers;
}

/**
 * Check if a response indicates a Cloudflare challenge
 */
export function isCloudflareChallenge(res: Response): boolean {
  if (res.status !== 403) return false;

  const cfMitigated = res.headers.get("cf-mitigated");
  const server = res.headers.get("server")?.toLowerCase() || "";

  return cfMitigated === "challenge" || server.includes("cloudflare");
}

/**
 * Fetch once without retry, returns response even if not ok
 */
export async function fetchOnce(url: string): Promise<Response> {
  const headers = buildRequestHeaders(url);

  if (requestDelayMs > 0) {
    const jitter = Math.floor(
      Math.random() * Math.max(1, requestDelayMs * 0.2),
    );
    await delay(requestDelayMs + jitter);
  }

  return fetch(url, {
    redirect: "follow",
    headers,
  });
}

/**
 * Fetch with basic retry and exponential backoff
 */
export async function fetchWithRetry(
  url: string,
  tries = 3,
  backoffMs = 400,
): Promise<Response> {
  let lastErr: unknown;
  const headers = buildRequestHeaders(url);

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
        headers,
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      return res;
    } catch (e) {
      lastErr = e;
      if (i < tries - 1) await delay(backoffMs * 2 ** i);
    }
  }
  throw lastErr;
}

/**
 * Download binary asset (image/font/js etc.) to a local file
 * Returns true if successful, false if the asset could not be downloaded
 * Set silent=true to suppress warnings for expected failures (e.g. images)
 */
export async function downloadBinary(
  url: string,
  dest: string,
  silent = false,
): Promise<boolean> {
  try {
    await ensureDir(dest.substring(0, dest.lastIndexOf("/")));
    const res = await fetchWithRetry(url);
    const buf = Buffer.from(await res.arrayBuffer());
    await fs.writeFile(dest, buf);
    return true;
  } catch (err) {
    if (!silent) {
      const msg = err instanceof Error ? err.message : String(err);
      console.warn(`Skipping asset ${url}: ${msg}`);
    }
    return false;
  }
}
