// Favicon and social preview from the order 9 seed tiling, 32 by 33.
import { chromium } from '@playwright/test';
import { writeFileSync } from 'node:fs';

const sizes = [15, 8, 9, 7, 1, 10, 18, 4, 14];
const width = 32;
const skyline = new Array(width).fill(0);
const squares = [];
for (const s of sizes) {
  const low = Math.min(...skyline);
  const x = skyline.indexOf(low);
  squares.push([x, low, s]);
  for (let c = x; c < x + s; c++) skyline[c] = low + s;
}

const ink = '#1b1a18';
const paper = '#f4f0e8';
const rects = (gap) =>
  squares
    .map(
      ([x, y, s]) =>
        `<rect x="${x + gap}" y="${y + gap}" width="${s - 2 * gap}" height="${s - 2 * gap}" fill="${ink}"/>`,
    )
    .join('');

const favicon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 33"><rect width="32" height="33" fill="${paper}"/>${rects(0.45)}</svg>\n`;
writeFileSync(new URL('../public/favicon.svg', import.meta.url), favicon);

const tile = 14;
const preview = `<!doctype html><body style="margin:0;background:${paper}"><svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630"><g transform="translate(${(1200 - 32 * tile) / 2} ${(630 - 33 * tile) / 2}) scale(${tile})">${rects(0.12)}</g></svg></body>`;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
await page.setContent(preview);
await page.screenshot({ path: new URL('../public/preview.png', import.meta.url).pathname });
await browser.close();
