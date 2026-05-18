// CTOX web-unlock helper: dump all external <script src> URLs of a target.
// Use this when you need to read the detection-site's actual JS source to
// understand a failing probe. Provide the URL via a pre-defined input
// variable `targetUrl` if you wrap this in a calling skill, or edit the
// fallback below.
const targetUrl = globalThis.targetUrl || 'https://bot.incolumitas.com/';

await page.goto(targetUrl, { waitUntil: 'networkidle', timeout: 60000 });
await page.waitForTimeout(3000);

const found = await page.evaluate(() => {
  const out = { external: [], inlineSamples: [] };
  for (const s of document.querySelectorAll('script')) {
    if (s.src) {
      out.external.push(s.src);
    } else {
      const code = s.textContent || '';
      if (code.length > 200) {
        out.inlineSamples.push({
          length: code.length,
          head: code.slice(0, 240),
        });
      }
    }
  }
  return out;
});

return { targetUrl, ...found };
