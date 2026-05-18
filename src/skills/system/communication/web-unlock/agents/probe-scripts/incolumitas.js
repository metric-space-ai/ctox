// CTOX web-unlock probe: bot.incolumitas.com.
// Parses the on-page JSON blocks (new-tests, detection-tests, webWorkerRes,
// serviceWorkerRes, fpjs, canvas/webgl fingerprints) and surfaces all FAILs.
// Baseline: 37/37 OK.
const fs = await import('node:fs/promises');
const os = await import('node:os');
const outDir = `${os.tmpdir()}/ctox-web-unlock`;
await fs.mkdir(outDir, { recursive: true });

await page.goto('https://bot.incolumitas.com/', { waitUntil: 'networkidle', timeout: 60000 });
// incolumitas runs background tests; wait for the blocks to populate.
await page.waitForTimeout(10000);

const screenshot = await page.screenshot({ fullPage: true, type: 'png' });
await fs.writeFile(`${outDir}/incolumitas.png`, screenshot);

const blocks = await page.evaluate(() => {
  const want = ['new-tests', 'detection-tests', 'webWorkerRes', 'serviceWorkerRes', 'fpjs', 'canvas_fingerprint', 'webgl_fingerprint', 'ip-api-data', 'fp'];
  const out = {};
  for (const id of want) {
    const el = document.getElementById(id);
    out[id] = el ? el.innerText.trim().slice(0, 4000) : null;
  }
  return out;
});

const tryParse = (s) => { try { return JSON.parse(s); } catch { return null; } };
const newTests = tryParse(blocks['new-tests']);
const detection = tryParse(blocks['detection-tests']);
const webWorker = tryParse(blocks['webWorkerRes']);
const serviceWorker = tryParse(blocks['serviceWorkerRes']);

const fails = [];
if (newTests) {
  for (const [k, v] of Object.entries(newTests)) if (v === 'FAIL') fails.push(`new-tests.${k}`);
}
if (detection && detection.intoli) {
  for (const [k, v] of Object.entries(detection.intoli)) if (v === 'FAIL') fails.push(`intoli.${k}`);
}
if (detection && detection.fpscanner) {
  for (const [k, v] of Object.entries(detection.fpscanner)) if (v === 'FAIL') fails.push(`fpscanner.${k}`);
}

// Worker UA consistency check
const sniffWorker = (worker, label) => {
  if (!worker) return null;
  const uaLeak = typeof worker.userAgent === 'string' && /HeadlessChrome/i.test(worker.userAgent);
  return uaLeak ? `${label}.userAgent contains HeadlessChrome` : null;
};
const workerWarnings = [
  sniffWorker(webWorker, 'webWorker'),
  sniffWorker(serviceWorker, 'serviceWorker'),
].filter(Boolean);

return {
  site: 'bot.incolumitas.com',
  url: page.url(),
  title: await page.title(),
  screenshot: `${outDir}/incolumitas.png`,
  totals: {
    failsCount: fails.length,
    workerWarnings: workerWarnings.length,
  },
  fails,
  workerWarnings,
  newTests,
  detection,
  webWorker,
  serviceWorker,
};
