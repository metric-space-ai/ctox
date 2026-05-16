// CTOX web-unlock probe: arh.antoinevastel.com/bots/areyouheadless.
// Returns the verdict text and screenshot. Baseline: "You are not Chrome headless".
const fs = await import('node:fs/promises');
const os = await import('node:os');
const outDir = `${os.tmpdir()}/ctox-web-unlock`;
await fs.mkdir(outDir, { recursive: true });

await page.goto('https://arh.antoinevastel.com/bots/areyouheadless', { waitUntil: 'networkidle', timeout: 45000 });
await page.waitForTimeout(2000);

const screenshot = await page.screenshot({ fullPage: true, type: 'png' });
await fs.writeFile(`${outDir}/areyouheadless.png`, screenshot);

const bodyText = await page.evaluate(() => document.body.innerText.trim());
const verdict = bodyText.match(/You are[^.]*headless[^.]*/i);
const isHeadless = verdict ? !/not Chrome headless/i.test(verdict[0]) : null;

return {
  site: 'arh.antoinevastel.com',
  url: page.url(),
  title: await page.title(),
  screenshot: `${outDir}/areyouheadless.png`,
  isHeadless,
  verdict: verdict ? verdict[0] : null,
  bodyExcerpt: bodyText.slice(0, 400),
};
