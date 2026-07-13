#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const url = process.env.BUSINESS_OS_DESIGN_LAB_URL || 'http://127.0.0.1:8765/design-lab.html';
const output = process.env.BUSINESS_OS_ACCESSIBILITY_OUTPUT
  || path.join(repoRoot, 'output/playwright/business-os-accessibility-contract.json');
const widths = [640, 960, 1180];
const themes = ['light', 'dark'];
const locales = ['de', 'en'];
const executablePath = [
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  chromium.executablePath(),
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((candidate) => candidate && fs.existsSync(candidate));
const browser = await chromium.launch({ headless: true, executablePath });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
const page = await context.newPage();
const browserErrors = [];
page.on('pageerror', (error) => browserErrors.push(error.message));
page.on('console', (message) => {
  if (message.type() === 'error') browserErrors.push(message.text());
});

const results = [];
try {
  await page.goto(url, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => typeof globalThis.renderDesignLabLocale === 'function');
  for (const width of widths) {
    for (const theme of themes) {
      for (const locale of locales) {
        await page.evaluate(({ width, theme, locale }) => {
          document.documentElement.dataset.theme = theme;
          globalThis.renderDesignLabLocale(locale);
          document.querySelector('[data-lab-frame]').style.width = `${width}px`;
        }, { width, theme, locale });
        await page.waitForTimeout(50);
        const contract = await page.locator('[data-lab-frame]').evaluate((root) => {
          const visible = (node) => {
            const style = getComputedStyle(node);
            return style.display !== 'none'
              && style.visibility !== 'hidden'
              && Number(style.opacity) > 0
              && node.getClientRects().length > 0;
          };
          const interactiveSelector = 'button, a[href], input, select, textarea, [role="button"], [role="link"], [tabindex]';
          const interactive = [...root.querySelectorAll(interactiveSelector)].filter(visible);
          const unnamed = interactive.filter((node) => {
            const labelledBy = node.getAttribute('aria-labelledby');
            const labelledText = labelledBy
              ? labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.textContent || '').join(' ')
              : '';
            const associatedLabel = node.id
              ? document.querySelector(`label[for="${CSS.escape(node.id)}"]`)?.textContent || ''
              : '';
            return !String(
              node.getAttribute('aria-label')
              || labelledText
              || associatedLabel
              || node.getAttribute('title')
              || node.getAttribute('placeholder')
              || node.textContent
              || '',
            ).trim();
          }).map(describe);
          const focusFailures = [];
          for (const node of interactive) {
            const before = focusAppearance(node);
            node.focus();
            const after = focusAppearance(node);
            const focusVisible = node.matches(':focus-visible');
            const changed = before.outline !== after.outline
              || before.boxShadow !== after.boxShadow
              || before.borderColor !== after.borderColor
              || before.backgroundColor !== after.backgroundColor;
            const explicitIndicator = after.outlineStyle !== 'none' && parseFloat(after.outlineWidth) >= 1
              || after.boxShadow !== 'none';
            if (document.activeElement !== node || !focusVisible || (!changed && !explicitIndicator)) {
              focusFailures.push({ node: describe(node), active: document.activeElement === node, focusVisible, changed, after });
            }
          }
          document.activeElement?.blur?.();

          const contrastFailures = [];
          const textNodes = [...root.querySelectorAll('*')].filter((node) => (
            visible(node)
            && [...node.childNodes].some((child) => child.nodeType === Node.TEXT_NODE && child.textContent.trim())
          ));
          for (const node of textNodes) {
            const style = getComputedStyle(node);
            const foreground = parseColor(style.color);
            const background = effectiveBackground(node);
            if (!foreground || !background || foreground.a === 0) continue;
            const ratio = contrast(composite(foreground, background), background);
            const fontSize = parseFloat(style.fontSize) || 16;
            const fontWeight = Number(style.fontWeight) || 400;
            const large = fontSize >= 24 || (fontSize >= 18.66 && fontWeight >= 700);
            const required = large ? 3 : 4.5;
            if (ratio + 0.01 < required) {
              contrastFailures.push({ node: describe(node), ratio: Number(ratio.toFixed(2)), required, color: style.color, background });
            }
          }

          const motionFailures = [...root.querySelectorAll('*')].filter(visible).flatMap((node) => {
            const style = getComputedStyle(node);
            const animationMs = maxDuration(style.animationDuration);
            const transitionMs = maxDuration(style.transitionDuration);
            return animationMs > 1 || transitionMs > 1
              ? [{ node: describe(node), animationMs, transitionMs }]
              : [];
          });

          return {
            interactiveCount: interactive.length,
            unnamed,
            focusFailures,
            contrastFailures,
            motionFailures,
            reducedMotion: matchMedia('(prefers-reduced-motion: reduce)').matches,
            horizontalOverflow: root.scrollWidth > root.clientWidth + 1,
          };

          function describe(node) {
            return `${node.tagName.toLowerCase()}${node.id ? `#${node.id}` : ''}${node.classList.length ? `.${[...node.classList].slice(0, 3).join('.')}` : ''}`;
          }
          function focusAppearance(node) {
            const style = getComputedStyle(node);
            return {
              outline: style.outline,
              outlineStyle: style.outlineStyle,
              outlineWidth: style.outlineWidth,
              boxShadow: style.boxShadow,
              borderColor: style.borderColor,
              backgroundColor: style.backgroundColor,
            };
          }
          function maxDuration(value) {
            return Math.max(0, ...String(value).split(',').map((part) => {
              const text = part.trim();
              return text.endsWith('ms') ? parseFloat(text) : parseFloat(text) * 1000;
            }).filter(Number.isFinite));
          }
          function parseColor(value) {
            if (!value) return null;
            const canvas = parseColor.canvas || (parseColor.canvas = document.createElement('canvas'));
            canvas.width = 1;
            canvas.height = 1;
            const context = canvas.getContext('2d', { willReadFrequently: true });
            context.clearRect(0, 0, 1, 1);
            context.fillStyle = '#010203';
            context.fillStyle = String(value);
            context.fillRect(0, 0, 1, 1);
            const [r, g, b, alpha] = context.getImageData(0, 0, 1, 1).data;
            return { r, g, b, a: alpha / 255 };
          }
          function effectiveBackground(node) {
            let color = { r: 255, g: 255, b: 255, a: 1 };
            const layers = [];
            for (let current = node; current; current = current.parentElement) {
              const parsed = parseColor(getComputedStyle(current).backgroundColor);
              if (parsed && parsed.a > 0) layers.push(parsed);
              if (parsed?.a === 1) break;
            }
            for (const layer of layers.reverse()) color = composite(layer, color);
            return color;
          }
          function composite(foreground, background) {
            const alpha = foreground.a + background.a * (1 - foreground.a);
            if (!alpha) return { r: 0, g: 0, b: 0, a: 0 };
            return {
              r: (foreground.r * foreground.a + background.r * background.a * (1 - foreground.a)) / alpha,
              g: (foreground.g * foreground.a + background.g * background.a * (1 - foreground.a)) / alpha,
              b: (foreground.b * foreground.a + background.b * background.a * (1 - foreground.a)) / alpha,
              a: alpha,
            };
          }
          function luminance(color) {
            const channels = [color.r, color.g, color.b].map((value) => {
              const normalized = value / 255;
              return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
            });
            return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
          }
          function contrast(a, b) {
            const first = luminance(a);
            const second = luminance(b);
            return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
          }
        });
        results.push({ width, theme, locale, ...contract });
      }
    }
  }
} finally {
  await browser.close();
}

const failures = results.flatMap((result) => {
  const scopes = [];
  if (!result.reducedMotion) scopes.push('reduced-motion-media');
  if (result.horizontalOverflow) scopes.push('horizontal-overflow');
  if (result.unnamed.length) scopes.push('accessible-name');
  if (result.focusFailures.length) scopes.push('visible-focus');
  if (result.contrastFailures.length) scopes.push('contrast');
  if (result.motionFailures.length) scopes.push('reduced-motion-duration');
  return scopes.length ? [{ width: result.width, theme: result.theme, locale: result.locale, scopes }] : [];
});
const report = {
  schema: 'ctox.business_os.accessibility_contract.v1',
  ok: failures.length === 0 && browserErrors.length === 0,
  url,
  standard: 'WCAG 2.2 AA automated contract subset',
  results,
  browserErrors,
  failures,
};
fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
if (!report.ok) {
  console.error(`Business OS accessibility contract failed (${failures.length} matrix cells, ${browserErrors.length} browser errors).`);
  console.error(`Report: ${output}`);
  process.exit(1);
}
console.log(`business_os_accessibility_matrix_cells=${results.length}`);
console.log('business_os_accessibility_contract_ok=1');
