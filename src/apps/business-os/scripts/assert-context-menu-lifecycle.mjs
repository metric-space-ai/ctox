#!/usr/bin/env node
// Executes the actual shell menu event handlers in Chromium. Only the command
// boundary is deferred; this is a UI race regression, not native E2E evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright');
const output = process.env.CONTEXT_MENU_PROOF_DIR;
assert.ok(output, 'CONTEXT_MENU_PROOF_DIR must point to disposable test storage');
fs.mkdirSync(output, { recursive: true });
const source = fs.readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const start = source.indexOf('function showGlobalCtoxContextMenu(');
const end = source.indexOf('\nasync function populateGlobalCtoxUserOptions(', start);
assert.ok(start >= 0 && end > start, 'actual shell menu implementation required');
const executablePath = [process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE, chromium.executablePath(),
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'].filter(Boolean).find(fs.existsSync);
const browser = await chromium.launch({ headless: true, executablePath });
const results = [];
try {
  for (const mode of ['ask', 'data', 'app']) {
    const page = await browser.newPage();
    await page.route('http://127.0.0.1/**', route => route.fulfill({
      contentType: 'text/html', body: '<main><div class="ctox-global-context-menu"></div></main>',
    }));
    await page.goto('http://127.0.0.1/context-menu-regression');
    await page.evaluate(({ implementation }) => {
      const menu = document.querySelector('.ctox-global-context-menu');
      const mod = { id: 'tickets', title: 'Tickets' };
      let settle;
      const receipt = new Promise(resolve => { settle = resolve; });
      Object.assign(window, {
        dispatched: 0,
        globalCtoxContextMenuEl: menu,
        state: { modules: [mod], session: {}, commandBus: { dispatch: () => {
          window.dispatched += 1;
          return receipt;
        } } },
        canModifyModule: () => false,
        canSelfExecuteBusinessData: () => false,
        appLifecycleState: () => ({}), appReleaseProjection: () => ({ dataAccess: {} }),
        buildGlobalCtoxAgentScopeView: () => ({}), actorContext: () => ({}),
        shellLang: () => 'en', shellText: () => '', escapeHtml: text => String(text),
        renderBusinessUserDatalistOptions: () => '', renderCompactGlobalCtoxAgentScopeHtml: () => '',
        renderGlobalCtoxContextModeHtml: () => ['data', 'ask', 'app'].map(mode =>
          `<label data-approval-required="${mode !== 'ask'}"><input name="contextMode" type="radio" value="${mode}">${mode}</label>`).join(''),
        populateGlobalCtoxUserOptions: async () => {},
        createContextActionsFacade: () => ({ dispatch: (_mode, options) => {
          window.dispatched += 1;
          options.onPresented();
          return receipt;
        } }),
      });
      // The source supplies both show/hide and their production callbacks.
      (0, eval)(implementation);
      window.race = {
        open: () => showGlobalCtoxContextMenu({ module: 'tickets' }, 10, 10),
        settle: () => settle({ command_id: 'same-command', status: 'queued' }),
      };
      window.race.open();
    }, { implementation: source.slice(start, end) });
    await page.locator(`label:has(input[value="${mode}"])`).click();
    await page.waitForFunction(() => document.activeElement?.matches('.ctox-context-textarea'));
    await page.locator('.ctox-context-textarea').fill('First request');
    if (mode !== 'ask') {
      await page.locator('.ctox-context-user-input').fill('reviewer');
      assert.equal(await page.locator('.ctox-context-user-input').inputValue(), 'reviewer');
    }

    await page.locator('button[type=submit]').click();
    assert.equal(await page.evaluate(() => window.dispatched), 1,
      `${mode} must dispatch once: ${await page.locator('.ctox-context-status').textContent()}`);
    await page.evaluate(() => window.race.open());
    await page.locator('label:has(input[value="app"])').click();
    await page.waitForFunction(() => document.activeElement?.matches('.ctox-context-textarea'));
    await page.locator('.ctox-context-textarea').fill('New unsent draft');
    const before = await page.locator('.ctox-context-textarea').inputValue();
    const began = performance.now();
    await page.evaluate(async () => {
      window.race.settle();
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
    const visible = await page.locator('.ctox-global-context-menu').isVisible();
    const draft = await page.locator('.ctox-context-textarea').inputValue();
    const settleToPaintMs = performance.now() - began;
    await page.screenshot({ path: path.join(output, `${mode}.png`) });
    results.push({ mode, visible, draftPreserved: draft === before, settleToPaintMs });
    assert.ok(visible, `late ${mode} completion must not close the newer menu`);
    assert.equal(draft, 'New unsent draft');
    await page.locator('.ctox-context-close-btn').click();
    assert.equal(await page.locator('.ctox-global-context-menu').isVisible(), false,
      'current close button must still close its own menu');
    await page.close();
  }
} finally {
  fs.writeFileSync(path.join(output, 'context-menu-lifecycle.json'), JSON.stringify({ results }, null, 2));
  await browser.close();
}
console.log(JSON.stringify({ ok: true, browser: 'Chromium', results }));
