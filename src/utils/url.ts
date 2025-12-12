/**
 * URL manipulation utilities
 */

import path from 'node:path';
import { safeFilename } from './filesystem.js';

/**
 * Map a URL to a local file path inside outDir
 * Handles both same-origin and external URLs
 */
export function urlToLocalPath(root: URL, target: URL, outDir: string, extHint?: string): string {
  if (target.origin !== root.origin) {
    // external: keep hostname as subfolder
    const hostDir = path.join(outDir, safeFilename(target.host));
    const pathname = target.pathname.endsWith('/') ? `${target.pathname}index` : target.pathname;
    const withExt = pathname.match(/\.[a-z0-9]+$/i) ? pathname : `${pathname}${extHint ?? ''}`;
    return path.join(hostDir, withExt).split('?')[0].split('#')[0];
  }
  // same-origin
  let p = target.pathname;
  if (p.endsWith('/')) p = path.join(p, 'index.html');
  else if (!p.match(/\.[a-z0-9]+$/i)) p = `${p}.html`;
  const file = path.join(outDir, p).split('?')[0].split('#')[0];
  return file;
}

/**
 * Create a relative path from one file to another
 * Ensures the result starts with ./ for consistency
 */
export function makeRelative(fromFile: string, toFile: string): string {
  let rel = path.relative(path.dirname(fromFile), toFile);
  if (!rel.startsWith('.')) rel = `./${rel}`;
  return rel.replace(/\\/g, '/');
}
