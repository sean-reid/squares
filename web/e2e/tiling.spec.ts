import { expect, test, type Page } from '@playwright/test';

async function testImage(page: Page) {
  const dataUrl = await page.evaluate(() => {
    const c = document.createElement('canvas');
    c.width = 900;
    c.height = 600;
    const ctx = c.getContext('2d')!;
    const g = ctx.createLinearGradient(0, 0, 900, 600);
    g.addColorStop(0, '#2b3a67');
    g.addColorStop(1, '#e8c170');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, 900, 600);
    ctx.fillStyle = '#d33f49';
    ctx.beginPath();
    ctx.arc(600, 260, 160, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = '#f4f0e8';
    ctx.fillRect(120, 340, 220, 180);
    return c.toDataURL('image/png');
  });
  return Buffer.from(dataUrl.split(',')[1]!, 'base64');
}

test('renders a photo as a tiling and exports it', async ({ page }, info) => {
  await page.goto('/');
  await expect(page.getByText(/choose/)).toBeVisible();
  await page.screenshot({ path: info.outputPath('empty.png') });

  const buffer = await testImage(page);
  await page.locator('#file').setInputFiles({ name: 'test.png', mimeType: 'image/png', buffer });

  const canvas = page.locator('#canvas');
  await expect(canvas).toBeVisible();
  await expect(page.locator('#status')).toHaveText(/growing|refining/);
  await expect(page.locator('#status')).toHaveText('', { timeout: 30_000 });
  const aspect = await page
    .locator('#stage')
    .evaluate((el) => el.style.getPropertyValue('--aspect'));
  expect(Number(aspect)).toBeGreaterThan(1.3);
  expect(Number(aspect)).toBeLessThan(1.7);
  await page.screenshot({ path: info.outputPath('tiling.png') });

  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'svg' }).click();
  const svg = await download;
  expect(svg.suggestedFilename()).toMatch(/^squares-\d+\.svg$/);
  const text = await (await svg.createReadStream()).toArray();
  expect(Buffer.concat(text).toString()).toContain('<rect');

  const png = page.waitForEvent('download');
  await page.getByRole('button', { name: 'png' }).click();
  expect((await png).suggestedFilename()).toMatch(/^squares-\d+\.png$/);
});

test('the wordmark returns to the empty page', async ({ page }) => {
  await page.goto('/');
  const buffer = await testImage(page);
  await page.locator('#file').setInputFiles({ name: 'test.png', mimeType: 'image/png', buffer });
  await expect(page.locator('#controls')).toBeVisible();
  await page.getByRole('link', { name: 'squares' }).click();
  await expect(page).toHaveURL('/');
  await expect(page.locator('#controls')).toBeHidden();
  await expect(page.locator('#canvas')).toBeHidden();
  await expect(page.getByText(/choose/)).toBeVisible();
  await expect(page.locator('#stage')).toHaveAttribute('role', 'button');
});

test('a file that does not decode gets one plain line', async ({ page }) => {
  await page.goto('/');
  await page
    .locator('#file')
    .setInputFiles({ name: 'x.heic', mimeType: 'image/heic', buffer: Buffer.from('not an image') });
  await expect(page.locator('#note')).toContainText('did not decode');
});
