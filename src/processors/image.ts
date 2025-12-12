/**
 * Image placeholder processing utilities
 */

import fs from 'node:fs/promises';
import path from 'node:path';
import probe from 'probe-image-size';
import { fetchWithRetry } from '../network/fetch.js';
import { ensureDir } from '../utils/filesystem.js';
import { urlToLocalPath } from '../utils/url.js';

// Attempt to import sharp lazily (optional)
// biome-ignore lint/suspicious/noExplicitAny: sharp types are complex and optional
let sharp: any = null;
(async () => {
  try {
    sharp = (await import('sharp')).default;
  } catch {
    /* optional dependency */
  }
})();

export interface PlaceholderResult {
  src: string;
  width?: number;
  height?: number;
}

/**
 * Decide placeholder URL or create local placeholder for an image
 * Strategy "external": uses placehold.co service
 * Strategy "local": generates a local PNG with sharp (if available)
 */
export async function getPlaceholderForImage(
  imgUrl: URL,
  strategy: 'external' | 'local',
  outDir: string,
  root: URL,
): Promise<PlaceholderResult> {
  // Try to quickly probe dimensions without full download
  let width: number | undefined;
  let height: number | undefined;
  try {
    const r = await fetchWithRetry(imgUrl.toString());
    // biome-ignore lint/suspicious/noExplicitAny: probe-image-size accepts WHATWG streams
    const stream = r.body as any;
    const meta = await probe(stream);
    width = meta.width;
    height = meta.height;
  } catch {
    // fallback to common dimensions
    width = 800;
    height = 450;
  }

  if (strategy === 'local' && sharp) {
    const ext = '.png';
    const localPath = urlToLocalPath(root, imgUrl, outDir, ext);
    await ensureDir(path.dirname(localPath));
    const w = Math.max(1, Math.min(4096, width ?? 800));
    const h = Math.max(1, Math.min(4096, height ?? 450));
    // simple gray placeholder with centered size text
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="#e5e7eb"/><text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="ui-sans-serif,system-ui" font-size="${Math.max(10, Math.floor(Math.min(w, h) / 6))}" fill="#6b7280">${w}×${h}</text></svg>`;
    const png = await sharp(Buffer.from(svg)).png().toBuffer();
    await fs.writeFile(localPath, png);
    return {
      src: localPath, // will be converted to relative path in HTML processor
      width: w,
      height: h,
    };
  }

  // external placeholder via placehold.co
  const w = width ?? 800;
  const h = height ?? 450;
  const placeholder = `https://placehold.co/${w}x${h}`;
  return { src: placeholder, width: w, height: h };
}
