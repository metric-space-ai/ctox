// CTOX web-unlock probe: bot.sannysoft.com.
// Returns the two test tables and the screenshot path.
// Invoked via: ctox web browser-automation --script-file <this> --timeout-ms 60000
const fs = await import('node:fs/promises');
const os = await import('node:os');
const outDir = `${os.tmpdir()}/ctox-web-unlock`;
await fs.mkdir(outDir, { recursive: true });

await page.goto('https://bot.sannysoft.com/', { waitUntil: 'networkidle', timeout: 45000 });
await page.waitForTimeout(2500);

const screenshot = await page.screenshot({ fullPage: true, type: 'png' });
await fs.writeFile(`${outDir}/sannysoft.png`, screenshot);

const results = await page.evaluate(() => {
  const out = { headless: [], fingerprint: [] };
  const tables = document.querySelectorAll('table');
  for (let t = 0; t < Math.min(tables.length, 2); t += 1) {
    const rows = tables[t].querySelectorAll('tr');
    const dest = t === 0 ? out.headless : out.fingerprint;
    for (const row of rows) {
      const cells = row.querySelectorAll('td');
      if (cells.length < 2) continue;
      dest.push({
        name: cells[0].textContent.trim(),
        result: cells[1].textContent.trim(),
        cls: cells[1].className || '',
      });
    }
  }
  return out;
});

const failed = [
  ...results.headless.filter((r) => /failed/i.test(r.cls)),
  ...results.fingerprint.filter((r) => /failed/i.test(r.cls)),
];

return {
  site: 'bot.sannysoft.com',
  url: page.url(),
  title: await page.title(),
  screenshot: `${outDir}/sannysoft.png`,
  totals: {
    headless: results.headless.length,
    fingerprint: results.fingerprint.length,
    failed: failed.length,
  },
  failed,
  results,
};
