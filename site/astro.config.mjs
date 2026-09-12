// @ts-check
import casoonPages from '@casoon/pages-theme';
import { defineConfig } from 'astro/config';

// Project page: https://casoon.github.io/site-scraper/ — `base` is the GitHub Pages path.
export default defineConfig({
  site: 'https://casoon.github.io/site-scraper',
  base: '/site-scraper/',
  integrations: [
    casoonPages({
      name: 'site-scraper',
      description:
        'A fast Rust CLI for creating self-contained static copies and full-page screenshots of websites.',
      repo: 'casoon/site-scraper',
      version: '1.4.4',
      license: 'MIT',
      packages: [
        { label: 'Releases', href: 'https://github.com/casoon/site-scraper/releases' },
      ],
      docsGroups: {
        'getting-started': 'Getting started',
        guides: 'Guides',
        reference: 'Reference',
      },
    }),
  ],
});
