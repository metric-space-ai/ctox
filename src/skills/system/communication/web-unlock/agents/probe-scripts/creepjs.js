// CTOX web-unlock probe: abrahamjuliot.github.io/creepjs.
// Slow probe (15-25s collection). Returns headless score lines + trust hash.
// Baseline: ~33% headless (structural ceiling for userland stealth).
const fs = await import('node:fs/promises');
const os = await import('node:os');
const outDir = `${os.tmpdir()}/ctox-web-unlock`;
await fs.mkdir(outDir, { recursive: true });

await page.goto('https://abrahamjuliot.github.io/creepjs/', { waitUntil: 'networkidle', timeout: 60000 });
await page.waitForTimeout(20000);

const screenshot = await page.screenshot({ fullPage: true, type: 'png' });
await fs.writeFile(`${outDir}/creepjs.png`, screenshot);

const result = await page.evaluate(() => {
  const out = {};
  const fpEl = document.querySelector('strong.trust-score');
  if (fpEl) out.fpScore = fpEl.textContent.trim();
  const text = document.body.innerText;
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  out.headlessLines = lines.filter((line) =>
    /headless:|like headless:/i.test(line)
  ).slice(0, 10);
  out.uaLeakStrings = lines.filter((line) => /HeadlessChrome/.test(line));
  return out;
});

return {
  site: 'abrahamjuliot.github.io/creepjs',
  url: page.url(),
  title: await page.title(),
  screenshot: `${outDir}/creepjs.png`,
  headlessSignal: result.uaLeakStrings.length > 0 ? 'UA LEAK' : 'clean',
  result,
};
