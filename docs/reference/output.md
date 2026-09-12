---
title: Output and exit codes
description: Directory layout, progress streams, summaries, file naming and failure behaviour.
order: 2
---

## Crawl directory

Each crawl writes to `output/<domain>/`. The domain directory is removed and recreated before the
run, so it contains only the newest snapshot for that domain.

HTML files follow the source URL paths. Downloaded CSS, JavaScript, fonts and images are referenced
with local relative paths.

## Screenshot directory

Standalone screenshots go to `screenshots/` unless `--output` is set. That directory is not
cleared. A successfully captured URL replaces only its own PNG.

## Streams

Progress and per-page status lines go to stderr. The final run summary goes to stdout. In an
interactive terminal, progress may be displayed as a live bar; redirected output stays plain.
Colours follow the `NO_COLOR` convention.

## Exit codes

- `0`: all requested pages or screenshots completed without a fatal failure.
- `1`: input or setup failed, or at least one page or screenshot failed.

Optional assets that cannot be downloaded are reported as warnings or skipped items and are
included in the final summary.
