// CTOX web-unlock probe: humanlike.mjs behavioral primitives.
//
// Runs against a data:URL test page (no network needed) and exercises
// humanClickLocator / humanType / humanScroll, then asserts:
//   1. The DOM side-effect actually happened (click registered, input has
//      the typed text, scroll position moved).
//   2. The behavioral signature is visible (multiple mousemove events
//      proving Bezier curve, multiple keydown events proving per-character
//      timing, multiple wheel events proving burst-wheel inertia).
//
// Output shape matches the 'sannysoft' parser_kind: { failed: [], totals }.
// Each fail entry has { name, cls: 'failed', note? }.

const fs = await import('node:fs/promises');
const os = await import('node:os');
const outDir = `${os.tmpdir()}/ctox-web-unlock`;
await fs.mkdir(outDir, { recursive: true });

// Inline scripts on data:URLs are commonly blocked by default CSP, so the
// HTML carries only markup. The event recorders are installed via
// page.evaluate after navigation.
const html = `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>humanlike-test</title></head>
<body style="margin:0;padding:8px;font:14px sans-serif">
  <input id="typing-input" type="text" autofocus style="font-size:16px;padding:6px;width:300px">
  <br><br>
  <button id="click-target" style="font-size:16px;padding:8px 16px">Click me</button>
  <br><br>
  <div id="scroll-spacer" style="height:6000px;background:linear-gradient(#fff,#eef)">scroll</div>
</body></html>`;
const dataUrl = 'data:text/html;charset=utf-8,' + encodeURIComponent(html);

await page.goto(dataUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForTimeout(200);

// Install event counters and the click-target handler after navigation.
await page.evaluate(() => {
  window.__events = {
    mousemove: 0, mousedown: 0, mouseup: 0, click: 0,
    keydown: 0, keyup: 0, wheel: 0,
  };
  for (const t of Object.keys(window.__events)) {
    window.addEventListener(t, () => { window.__events[t]++; }, true);
  }
  const btn = document.getElementById('click-target');
  if (btn) {
    btn.addEventListener('click', () => { btn.textContent = 'clicked'; });
  }
});

const failed = [];
const evidence = {};

// Pre-flight: humanlike module must be loaded by the generic runner
if (!globalThis.humanlike) {
  failed.push({ name: 'humanlike-module-loaded', cls: 'failed', note: 'globalThis.humanlike is null' });
  return {
    site: 'humanlike-internal',
    url: page.url(),
    totals: { failed: failed.length },
    failed,
    evidence,
  };
}
evidence.exports = Object.keys(globalThis.humanlike);

// ─────────────────────────────────────────────────────────────────────
// Test 1 — humanClickLocator: click registers + Bezier mouse path visible
// ─────────────────────────────────────────────────────────────────────
try {
  await page.evaluate(() => {
    window.__events.mousemove = 0;
    window.__events.click = 0;
    window.__events.mousedown = 0;
    window.__events.mouseup = 0;
  });
  await globalThis.humanlike.humanClickLocator(
    page,
    page.locator('#click-target'),
    { from: { x: 50, y: 500 } },
  );
  await page.waitForTimeout(200);
  const r = await page.evaluate(() => ({
    events: { ...window.__events },
    buttonText: document.getElementById('click-target').textContent,
  }));
  evidence.click = r;
  if (r.buttonText !== 'clicked') {
    failed.push({ name: 'humanlike.click.fires-click', cls: 'failed', note: `button text "${r.buttonText}"` });
  }
  if (r.events.click < 1) {
    failed.push({ name: 'humanlike.click.click-event-count', cls: 'failed', note: `${r.events.click}` });
  }
  // Bezier mouse path: cubic Bezier with min_steps=25 — expect lots of mousemove.
  if (r.events.mousemove < 10) {
    failed.push({
      name: 'humanlike.click.bezier-path-visible',
      cls: 'failed',
      note: `expected >=10 mousemove events, saw ${r.events.mousemove}`,
    });
  }
  if (r.events.mousedown < 1 || r.events.mouseup < 1) {
    failed.push({
      name: 'humanlike.click.down-up-events',
      cls: 'failed',
      note: `mousedown=${r.events.mousedown} mouseup=${r.events.mouseup}`,
    });
  }
} catch (err) {
  failed.push({ name: 'humanlike.click.threw', cls: 'failed', note: String(err && err.message || err) });
}

// ─────────────────────────────────────────────────────────────────────
// Test 2 — humanType: text lands in input + per-character keydown timing
// ─────────────────────────────────────────────────────────────────────
try {
  await page.evaluate(() => {
    window.__events.keydown = 0;
    window.__events.keyup = 0;
    document.getElementById('typing-input').value = '';
    document.getElementById('typing-input').focus();
  });
  const TYPE = 'Hello-World';
  await globalThis.humanlike.humanType(page.locator('#typing-input'), TYPE);
  await page.waitForTimeout(200);
  const r = await page.evaluate(() => ({
    events: { ...window.__events },
    inputValue: document.getElementById('typing-input').value,
  }));
  evidence.type = r;
  // Allow the value to differ slightly because of the 2% mistype-then-backspace
  // pattern. As long as the cleaned text contains the intended sequence it's OK.
  if (!r.inputValue.includes('Hello') || !r.inputValue.includes('World')) {
    failed.push({
      name: 'humanlike.type.input-value-correct',
      cls: 'failed',
      note: `expected ~"${TYPE}", got "${r.inputValue}"`,
    });
  }
  // One keydown per character minimum. The Hyphen is a shifted char on some
  // layouts but it's actually plain '-'; still expect at least len keydowns.
  if (r.events.keydown < TYPE.length) {
    failed.push({
      name: 'humanlike.type.per-char-keydowns',
      cls: 'failed',
      note: `expected >=${TYPE.length} keydowns, saw ${r.events.keydown}`,
    });
  }
  if (r.events.keyup < TYPE.length) {
    failed.push({
      name: 'humanlike.type.per-char-keyups',
      cls: 'failed',
      note: `expected >=${TYPE.length} keyups, saw ${r.events.keyup}`,
    });
  }
} catch (err) {
  failed.push({ name: 'humanlike.type.threw', cls: 'failed', note: String(err && err.message || err) });
}

// ─────────────────────────────────────────────────────────────────────
// Test 3 — humanScroll: scrollY moves + burst-wheel inertia visible
// ─────────────────────────────────────────────────────────────────────
try {
  await page.evaluate(() => {
    window.__events.wheel = 0;
    document.scrollingElement.scrollTop = 0;
    window.scrollTo(0, 0);
  });
  await globalThis.humanlike.humanScroll(page, 1200);
  await page.waitForTimeout(400);
  const r = await page.evaluate(() => ({
    events: { ...window.__events },
    scrollY: window.scrollY || document.scrollingElement.scrollTop,
  }));
  evidence.scroll = r;
  if (r.scrollY < 100) {
    failed.push({
      name: 'humanlike.scroll.moves-page',
      cls: 'failed',
      note: `expected scrollY >= 100, got ${r.scrollY}`,
    });
  }
  // Burst-wheel inertia: 3 phases × 3-5 sub-chunks each + possible overshoot.
  // Expect a large number of wheel events, definitely more than 1.
  if (r.events.wheel < 5) {
    failed.push({
      name: 'humanlike.scroll.burst-wheel-visible',
      cls: 'failed',
      note: `expected >=5 wheel events, saw ${r.events.wheel}`,
    });
  }
} catch (err) {
  failed.push({ name: 'humanlike.scroll.threw', cls: 'failed', note: String(err && err.message || err) });
}

// Final screenshot for forensics
try {
  const png = await page.screenshot({ fullPage: false, type: 'png' });
  await fs.writeFile(`${outDir}/humanlike.png`, png);
} catch {}

return {
  site: 'humanlike-internal',
  url: 'data:text/html (humanlike test page)',
  title: 'humanlike-test',
  screenshot: `${outDir}/humanlike.png`,
  totals: { failed: failed.length },
  failed,
  evidence,
};
