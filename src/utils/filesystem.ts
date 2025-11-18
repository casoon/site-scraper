/**
 * Filesystem utility functions
 */

import fs from "node:fs/promises";

/**
 * Ensure a directory exists, creating it recursively if needed
 */
export async function ensureDir(dir: string): Promise<void> {
  await fs.mkdir(dir, { recursive: true });
}

/**
 * Convert a string to a safe filename by replacing invalid characters
 */
export function safeFilename(s: string): string {
  return s
    .replace(/[^a-zA-Z0-9._-]/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}
