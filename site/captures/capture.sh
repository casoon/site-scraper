#!/usr/bin/env bash
# Regenerates the real outputs shown on the project site, from the release binary.
#   site/captures/capture.sh   (after cargo build --release --features headless)
# Crawls only the owner's own sites, with the default 300 ms delay and browser headers.
#
# 1. Help and error output in examples/ (showcase, CLI reference).
# 2. examples/crawl-casoon.txt: a crawl of casoon.de in a pseudo-terminal, for colour.
# 3. site/public/screens/casoon-dev-full-page.webp: the screenshot subcommand on casoon.dev.
#
# The start page scene crawl.svg is recorded separately, because castwright runs the
# commands itself (exec: steps, needs node-pty; on macOS chmod +x its spawn-helper once).
# crawl.exec.yaml holds the exec: steps; with site-scraper on PATH:
#   cd site/captures && cp crawl.exec.yaml crawl.terminal.yaml && npx -y -p @casoon/castwright@0.5.0 -p shiki -p node-pty \
#     castwright build crawl.terminal.yaml --allow-exec --record --format svg -o .
# and remove the .work/ directory it crawled into. Rebuilding the recorded scene needs no network:
#   cd site/captures && npx -y -p @casoon/castwright@0.5.0 -p shiki castwright build crawl.terminal.yaml --format svg -o .
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
bin="${SITE_SCRAPER:-${CARGO_TARGET_DIR:-$repo/target}/release/site-scraper}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$bin" --help > "$repo/examples/crawl-help.txt"
"$bin" screenshot --help > "$repo/examples/screenshot-help.txt"
"$bin" not-a-url > "$repo/examples/invalid-url.txt" 2>&1 || true

# script(1) gives the crawl a terminal, so status lines keep their colours.
(cd "$work" && script -q "$work/crawl.txt" "$bin" https://www.casoon.de --max-depth 1 --no-sitemap --placeholder local > /dev/null)
# Drop carriage returns and the ^D echo that macOS script(1) writes first.
tr -d '\r' < "$work/crawl.txt" | perl -pe 's/^\^D\x08\x08// if $. == 1' > "$repo/examples/crawl-casoon.txt"

"$bin" screenshot https://casoon.dev/ --output "$work/shots"
cwebp -quiet -q 80 -resize 960 0 "$work/shots/casoon.dev--index.png" \
  -o "$repo/site/public/screens/casoon-dev-full-page.webp"
# Update width/height in docs/guides/headless-and-screenshots.mdx if the page height changed.
