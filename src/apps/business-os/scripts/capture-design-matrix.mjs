import { existsSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const baseUrl = process.env.BUSINESS_OS_DESIGN_LAB_URL || 'http://127.0.0.1:8765/design-lab.html';
const outputDir = process.env.BUSINESS_OS_DESIGN_OUTPUT || path.join(repoRoot, 'output/playwright/business-os-design-matrix');
const widths = [640, 960, 1180];
const themes = ['light', 'dark'];
const locales = ['de', 'en'];

await mkdir(outputDir, { recursive: true });
const executablePath = [
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  chromium.executablePath(),
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((candidate) => candidate && existsSync(candidate));
const browser = await chromium.launch({
  headless: process.env.HEADFUL !== '1',
  executablePath,
});
const context = await browser.newContext({ viewport: { width: 1280, height: 820 }, reducedMotion: 'reduce' });
const page = await context.newPage();
const consoleErrors = [];
page.on('pageerror', (error) => consoleErrors.push(error.message));
page.on('console', (message) => {
  if (message.type() === 'error') consoleErrors.push(message.text());
});

try {
  await page.goto(baseUrl, { waitUntil: 'networkidle' });
  const captures = [];
  for (const width of widths) {
    for (const theme of themes) {
      for (const locale of locales) {
        await page.evaluate(({ width, theme, locale }) => {
          document.documentElement.dataset.theme = theme;
          globalThis.renderDesignLabLocale?.(locale);
          document.querySelector('[data-lab-frame]').style.width = `${width}px`;
          document.querySelector('[data-lab-theme]').value = theme;
          document.querySelector('[data-lab-width]').value = String(width);
        }, { width, theme, locale });
        await page.waitForFunction((locale) => document.documentElement.dataset.designLabLocale === locale, locale);
        await page.waitForTimeout(50);
        const metrics = await page.locator('[data-lab-frame]').evaluate((frame) => ({
          clientWidth: frame.clientWidth,
          scrollWidth: frame.scrollWidth,
          buttonsWithoutName: Array.from(frame.querySelectorAll('button'))
            .filter((button) => !(button.textContent || '').trim() && !button.getAttribute('aria-label')).length,
        }));
        if (metrics.scrollWidth > metrics.clientWidth + 1) {
          throw new Error(`${width}/${theme}/${locale}: horizontal frame overflow ${metrics.scrollWidth} > ${metrics.clientWidth}`);
        }
        if (metrics.buttonsWithoutName) {
          throw new Error(`${width}/${theme}/${locale}: ${metrics.buttonsWithoutName} unnamed buttons`);
        }
        await page.screenshot({
          path: `${outputDir}/${width}-${theme}-${locale}.png`,
          fullPage: true,
          animations: 'disabled',
          caret: 'hide',
        });
        captures.push({ width, theme, locale, file: `${width}-${theme}-${locale}.png`, metrics });
      }
    }
  }
  if (consoleErrors.length) throw new Error(`browser console errors:\n${consoleErrors.join('\n')}`);
  await writeFile(path.join(outputDir, 'design-matrix.json'), `${JSON.stringify({
    schema: 'ctox.business_os.design_matrix.v1',
    baseUrl,
    reducedMotion: true,
    captures,
  }, null, 2)}\n`);
  console.log(`Business OS design matrix OK (${widths.length * themes.length * locales.length} screenshots)`);
} finally {
  await browser.close();
}
