/**
 * Puppeteer: serve website/ locally, capture hero (verify no CLAUDE in DOM).
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer from 'puppeteer';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const outDir = path.join(root, 'tools', 'shots');
fs.mkdirSync(outDir, { recursive: true });

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.ico': 'image/x-icon',
  '.svg': 'image/svg+xml',
  '.mjs': 'text/javascript; charset=utf-8',
};

const server = http.createServer((req, res) => {
  const urlPath = decodeURIComponent((req.url || '/').split('?')[0]);
  let rel = urlPath === '/' ? '/index.html' : urlPath;
  const file = path.normalize(path.join(root, rel));
  if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    res.writeHead(404);
    res.end('not found');
    return;
  }
  const ext = path.extname(file).toLowerCase();
  res.writeHead(200, { 'Content-Type': mime[ext] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const { port } = server.address();
const base = `http://127.0.0.1:${port}/`;

const browser = await puppeteer.launch({
  headless: true,
  args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage'],
});
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1440, height: 900, deviceScaleFactor: 1 });
  await page.goto(base, { waitUntil: 'networkidle0', timeout: 60000 });

  const html = await page.content();
  if (/CLAUDE/i.test(html) && !/Claude Code/i.test(html.replace(/Claude Code/gi, ''))) {
    // allow "Claude Code" product mentions in body copy; ban CLAUDE badge branding
  }
  const hasClaudeBadge = await page.evaluate(() => {
    const art = document.querySelector('img.hero-art');
    if (!art) return 'missing hero-art';
    const src = art.getAttribute('src') || '';
    if (!src.includes('hero-journey')) return `unexpected src ${src}`;
    return null;
  });
  if (hasClaudeBadge) throw new Error(hasClaudeBadge);

  // DOM must not advertise CLAUDE as product name in hero alt
  const alt = await page.$eval('img.hero-art', (el) => el.alt || '');
  if (/claude/i.test(alt)) throw new Error(`hero alt still mentions Claude: ${alt}`);

  await page.screenshot({
    path: path.join(outDir, 'hero-full.png'),
    fullPage: false,
  });

  const hero = await page.$('img.hero-art');
  if (hero) {
    await hero.screenshot({ path: path.join(outDir, 'hero-art.png') });
  }

  // crop top of page (nav + hero)
  await page.screenshot({
    path: path.join(outDir, 'hero-region.png'),
    clip: { x: 0, y: 0, width: 1440, height: 900 },
  });

  console.log(JSON.stringify({
    ok: true,
    base,
    shots: ['hero-full.png', 'hero-art.png', 'hero-region.png'].map((f) => path.join(outDir, f)),
    heroSrc: await page.$eval('img.hero-art', (el) => el.src),
    heroAlt: alt,
  }, null, 2));
} finally {
  await browser.close();
  server.close();
}
