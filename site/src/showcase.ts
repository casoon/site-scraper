import { ansiToHtml } from '@casoon/pages-theme/ansi';
import type { ShowcaseExample } from '@casoon/pages-theme/showcase';

// Captured from the current CLI so the website examples stay reviewable alongside the code.
const files = import.meta.glob<string>('../../examples/*.txt', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const examples_ = [
  {
    slug: 'crawl-help',
    title: 'Crawl command',
    file: 'crawl-help.txt',
    command: 'site-scraper --help',
    tags: ['crawl', 'options'],
    description: 'The default command mirrors a website and exposes crawl, asset and request options.',
  },
  {
    slug: 'screenshot-help',
    title: 'Screenshot command',
    file: 'screenshot-help.txt',
    command: 'site-scraper screenshot --help',
    tags: ['screenshot', 'headless'],
    description: 'Capture one URL or a UTF-8 list as full-page PNG files without mirroring the site.',
  },
  {
    slug: 'invalid-url',
    title: 'Invalid URL',
    file: 'invalid-url.txt',
    command: 'site-scraper not-a-url',
    tags: ['error', 'validation'],
    description: 'Invalid input fails before a crawl starts and returns exit code 1.',
  },
];

export const examples: ShowcaseExample[] = examples_.map(({ file, command, ...meta }) => {
  const source = files[`../../examples/${file}`] ?? '';
  return {
    ...meta,
    file: `examples/${file}`,
    input: { code: command, lang: 'shell' },
    output: { html: ansiToHtml(source), kind: 'terminal' },
  };
});
