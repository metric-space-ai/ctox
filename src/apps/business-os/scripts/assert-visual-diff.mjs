#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const args = parseArgs(process.argv.slice(2));
const baselineDir = path.resolve(args.baseline || path.join(repoRoot, 'src/apps/business-os/qa/design-matrix-baseline'));
const actualDir = path.resolve(args.actual || path.join(repoRoot, 'output/playwright/business-os-design-matrix'));
const outputDir = path.resolve(args.output || path.join(repoRoot, 'output/playwright/business-os-design-diff'));
const pixelThreshold = parseRatio(args['pixel-threshold'] || '0.08', '--pixel-threshold');
const maxMismatchRatio = parseRatio(args['max-mismatch-ratio'] || '0.005', '--max-mismatch-ratio');
const updateBaseline = args['update-baseline'] === true;

if (updateBaseline) {
  replaceGeneratedBaseline(actualDir, baselineDir);
  console.log(`business_os_visual_baseline_updated=${baselineDir}`);
  process.exit(0);
}

const baselineFiles = pngFiles(baselineDir);
const actualFiles = pngFiles(actualDir);
if (!baselineFiles.length) throw new Error(`No PNG baselines found in ${baselineDir}`);
assertSameFileSet(baselineFiles, actualFiles);
fs.mkdirSync(outputDir, { recursive: true });

const executablePath = [
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  chromium.executablePath(),
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((candidate) => candidate && fs.existsSync(candidate));
const browser = await chromium.launch({ headless: true, executablePath });
const page = await browser.newPage();
const comparisons = [];
try {
  for (const relativePath of baselineFiles) {
    const baseline = fs.readFileSync(path.join(baselineDir, relativePath)).toString('base64');
    const actual = fs.readFileSync(path.join(actualDir, relativePath)).toString('base64');
    const comparison = await page.evaluate(async ({ baseline, actual, pixelThreshold }) => {
      const [expectedImage, actualImage] = await Promise.all([
        decode(`data:image/png;base64,${baseline}`),
        decode(`data:image/png;base64,${actual}`),
      ]);
      if (expectedImage.width !== actualImage.width || expectedImage.height !== actualImage.height) {
        return {
          dimensionsMatch: false,
          expected: { width: expectedImage.width, height: expectedImage.height },
          actual: { width: actualImage.width, height: actualImage.height },
          mismatchPixels: expectedImage.width * expectedImage.height,
          totalPixels: expectedImage.width * expectedImage.height,
          mismatchRatio: 1,
          diff: '',
        };
      }
      const width = expectedImage.width;
      const height = expectedImage.height;
      const expectedPixels = pixels(expectedImage, width, height);
      const actualPixels = pixels(actualImage, width, height);
      const diffCanvas = document.createElement('canvas');
      diffCanvas.width = width;
      diffCanvas.height = height;
      const diffContext = diffCanvas.getContext('2d');
      const diffImage = diffContext.createImageData(width, height);
      const channelThreshold = Math.round(255 * pixelThreshold);
      let mismatchPixels = 0;
      for (let index = 0; index < expectedPixels.data.length; index += 4) {
        const different = Math.max(
          Math.abs(expectedPixels.data[index] - actualPixels.data[index]),
          Math.abs(expectedPixels.data[index + 1] - actualPixels.data[index + 1]),
          Math.abs(expectedPixels.data[index + 2] - actualPixels.data[index + 2]),
          Math.abs(expectedPixels.data[index + 3] - actualPixels.data[index + 3]),
        ) > channelThreshold;
        if (different) mismatchPixels += 1;
        diffImage.data[index] = different ? 255 : actualPixels.data[index] * 0.25;
        diffImage.data[index + 1] = different ? 32 : actualPixels.data[index + 1] * 0.25;
        diffImage.data[index + 2] = different ? 64 : actualPixels.data[index + 2] * 0.25;
        diffImage.data[index + 3] = 255;
      }
      diffContext.putImageData(diffImage, 0, 0);
      const totalPixels = width * height;
      return {
        dimensionsMatch: true,
        expected: { width, height },
        actual: { width, height },
        mismatchPixels,
        totalPixels,
        mismatchRatio: mismatchPixels / totalPixels,
        diff: diffCanvas.toDataURL('image/png').split(',')[1],
      };

      function pixels(image, width, height) {
        const canvas = document.createElement('canvas');
        canvas.width = width;
        canvas.height = height;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        context.drawImage(image, 0, 0);
        return context.getImageData(0, 0, width, height);
      }

      function decode(url) {
        return new Promise((resolve, reject) => {
          const image = new Image();
          image.onload = () => resolve(image);
          image.onerror = () => reject(new Error('PNG decode failed'));
          image.src = url;
        });
      }
    }, { baseline, actual, pixelThreshold });
    const ok = comparison.dimensionsMatch && comparison.mismatchRatio <= maxMismatchRatio;
    if (!ok && comparison.diff) {
      const diffPath = path.join(outputDir, relativePath.replace(/\.png$/i, '.diff.png'));
      fs.mkdirSync(path.dirname(diffPath), { recursive: true });
      fs.writeFileSync(diffPath, Buffer.from(comparison.diff, 'base64'));
    }
    comparisons.push({ file: relativePath, ok, ...comparison, diff: undefined });
  }
} finally {
  await browser.close();
}

const result = {
  schema: 'ctox.business_os.visual_diff.v1',
  ok: comparisons.every((entry) => entry.ok),
  baselineDir,
  actualDir,
  outputDir,
  pixelThreshold,
  maxMismatchRatio,
  comparisons,
};
fs.writeFileSync(path.join(outputDir, 'visual-diff.json'), `${JSON.stringify(result, null, 2)}\n`);
if (!result.ok) {
  const failed = comparisons.filter((entry) => !entry.ok);
  console.error(`Business OS visual diff failed for ${failed.length}/${comparisons.length} screenshots.`);
  for (const entry of failed) console.error(`- ${entry.file}: ${(entry.mismatchRatio * 100).toFixed(3)}%`);
  process.exit(1);
}
console.log(`business_os_visual_diff_files=${comparisons.length}`);
console.log('business_os_visual_diff_ok=1');

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith('--')) throw new Error(`Unexpected argument: ${value}`);
    const key = value.slice(2);
    if (key === 'update-baseline') parsed[key] = true;
    else parsed[key] = values[++index];
  }
  return parsed;
}

function parseRatio(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1) throw new Error(`${name} must be between 0 and 1`);
  return parsed;
}

function pngFiles(root) {
  if (!fs.existsSync(root)) return [];
  const found = [];
  visit(root, '');
  return found.sort();
  function visit(absolute, relative) {
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      const nextRelative = path.join(relative, entry.name);
      const nextAbsolute = path.join(absolute, entry.name);
      if (entry.isDirectory()) visit(nextAbsolute, nextRelative);
      else if (/\.png$/i.test(entry.name)) found.push(nextRelative);
    }
  }
}

function assertSameFileSet(baselineFiles, actualFiles) {
  const baseline = new Set(baselineFiles);
  const actual = new Set(actualFiles);
  const missing = baselineFiles.filter((file) => !actual.has(file));
  const unexpected = actualFiles.filter((file) => !baseline.has(file));
  if (missing.length || unexpected.length) {
    throw new Error(`Visual matrix file-set mismatch. missing=${missing.join(', ') || 'none'} unexpected=${unexpected.join(', ') || 'none'}`);
  }
}

function replaceGeneratedBaseline(source, target) {
  const files = pngFiles(source);
  if (!files.length) throw new Error(`No PNG screenshots found in ${source}`);
  fs.rmSync(target, { recursive: true, force: true });
  for (const relative of files) {
    const destination = path.join(target, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(path.join(source, relative), destination);
  }
  fs.writeFileSync(path.join(target, 'README.md'), [
    '# Business OS Design Matrix Baseline',
    '',
    'Generated only through `npm run qa:visual-baseline` after human visual review.',
    'Ordinary QA uses the immutable baseline and never updates it implicitly.',
    '',
  ].join('\n'));
}
