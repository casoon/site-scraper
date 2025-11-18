---
description: Run a quick test scrape with common options
---

Run a test scrape with the following command:

```bash
pnpm run dev https://example.com --maxDepth 1 --concurrency 4 --placeholder external
```

After completion:
1. Check the output in `./output/example.com/`
2. Verify HTML files are present
3. Check that CSS and assets were downloaded
4. Confirm images were replaced with placeholders

Common test URLs:
- https://example.com (simple, fast)
- https://www.casoon.de (production site)
