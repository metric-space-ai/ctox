import { existsSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { workspaceBrandingStyleText } from '../shared/branding.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const baseUrl = process.env.BUSINESS_OS_DESIGN_LAB_URL || 'http://127.0.0.1:8765/design-lab.html';
const outputDir = process.env.BUSINESS_OS_DESIGN_OUTPUT || path.join(repoRoot, 'output/playwright/business-os-design-matrix');
const widths = [640, 960, 1180];
const themes = ['light', 'dark'];
const locales = ['de', 'en'];
const brands = ['default', 'custom'];

// QA custom-brand fixture: a deliberately non-default but readable identity
// (violet accent on warm paper in light, on deep plum in dark). Distinct
// enough that any token leaking past the branding whitelist is visible in the
// branded captures, while staying readable in both themes. Applied through
// the exact runtime mechanism: workspaceBrandingStyleText() builds the same
// :root[data-workspace-branding="custom"] block branding.js injects, and the
// page sets dataset.workspaceBranding = 'custom' (see applyWorkspaceBranding).
// focus_ring is intentionally unset: the whitelist only accepts bare colors,
// but --focus-ring is a full box-shadow — a color value would break the ring.
const BRAND_FIXTURE = {
  name: 'QA Brand Fixture',
  custom: true,
  light: {
    bg: '#f5f1e8',
    surface: '#fffdf6',
    surface_2: '#eee7d6',
    line: '#d8cdb4',
    text: '#2b2440',
    text_strong: '#17122b',
    muted: '#6d6480',
    accent: '#7c3aed',
    accent_soft: '#ece4fc',
    accent_foreground: '#ffffff',
    danger: '#b91c1c',
    warning: '#a16207',
    success: '#047857',
  },
  dark: {
    bg: '#16121f',
    surface: '#1e1830',
    surface_2: '#282040',
    line: '#3d3358',
    text: '#e9e4f5',
    text_strong: '#ffffff',
    muted: '#a79ec4',
    accent: '#a78bfa',
    accent_soft: 'rgba(167, 139, 250, 0.16)',
    accent_foreground: '#17122b',
    danger: '#f87171',
    warning: '#fbbf24',
    success: '#34d399',
  },
};
const BRAND_FIXTURE_CSS = workspaceBrandingStyleText(BRAND_FIXTURE);
if (!BRAND_FIXTURE_CSS.includes(':root[data-workspace-branding="custom"]')) {
  throw new Error('brand fixture produced no :root[data-workspace-branding="custom"] block');
}

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
        for (const brand of brands) {
          await page.evaluate(({ width, theme, locale, brand, fixtureCss }) => {
            document.documentElement.dataset.theme = theme;
            globalThis.renderDesignLabLocale?.(locale);
            document.querySelector('[data-lab-frame]').style.width = `${width}px`;
            document.querySelector('[data-lab-theme]').value = theme;
            document.querySelector('[data-lab-width]').value = String(width);
            let brandStyle = document.getElementById('ctox-workspace-branding-style');
            if (brand === 'custom') {
              if (!brandStyle) {
                brandStyle = document.createElement('style');
                brandStyle.id = 'ctox-workspace-branding-style';
                brandStyle.dataset.workspaceBranding = 'true';
                document.head.appendChild(brandStyle);
              }
              brandStyle.textContent = fixtureCss;
              document.documentElement.dataset.workspaceBranding = 'custom';
            } else {
              brandStyle?.remove();
              delete document.documentElement.dataset.workspaceBranding;
            }
          }, { width, theme, locale, brand, fixtureCss: BRAND_FIXTURE_CSS });
          await page.waitForFunction((locale) => document.documentElement.dataset.designLabLocale === locale, locale);
          await page.waitForTimeout(50);
          const metrics = await page.locator('[data-lab-frame]').evaluate((frame) => ({
            clientWidth: frame.clientWidth,
            scrollWidth: frame.scrollWidth,
            buttonsWithoutName: Array.from(frame.querySelectorAll('button'))
              .filter((button) => !(button.textContent || '').trim() && !button.getAttribute('aria-label')).length,
          }));
          if (metrics.scrollWidth > metrics.clientWidth + 1) {
            throw new Error(`${width}/${theme}/${locale}/${brand}: horizontal frame overflow ${metrics.scrollWidth} > ${metrics.clientWidth}`);
          }
          if (metrics.buttonsWithoutName) {
            throw new Error(`${width}/${theme}/${locale}/${brand}: ${metrics.buttonsWithoutName} unnamed buttons`);
          }
          const file = `${width}-${theme}-${locale}${brand === 'custom' ? '-brand' : ''}.png`;
          await page.screenshot({
            path: `${outputDir}/${file}`,
            fullPage: true,
            animations: 'disabled',
            caret: 'hide',
          });
          captures.push({ width, theme, locale, brand, file, metrics });
        }
      }
    }
  }
  if (consoleErrors.length) throw new Error(`browser console errors:\n${consoleErrors.join('\n')}`);
  await writeFile(path.join(outputDir, 'design-matrix.json'), `${JSON.stringify({
    schema: 'ctox.business_os.design_matrix.v1',
    baseUrl,
    reducedMotion: true,
    brands,
    captures,
  }, null, 2)}\n`);
  console.log(`Business OS design matrix OK (${widths.length * themes.length * locales.length * brands.length} screenshots)`);
} finally {
  await browser.close();
}
