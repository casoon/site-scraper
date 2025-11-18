/**
 * Network fetch utilities with retry logic
 */

import { setTimeout as delay } from "node:timers/promises";
import fs from "node:fs/promises";
import { ensureDir } from "../utils/filesystem.js";

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
      const res = await fetch(url, { redirect: "follow" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
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
 */
export async function downloadBinary(url: string, dest: string): Promise<void> {
  await ensureDir(dest.substring(0, dest.lastIndexOf("/")));
  const res = await fetchWithRetry(url);
  const buf = Buffer.from(await res.arrayBuffer());
  await fs.writeFile(dest, buf);
}
