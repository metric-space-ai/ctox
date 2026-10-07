// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = process.argv[2];
assert(output, 'Pass an output directory on the build lane');
await mkdir(output, { recursive: true });
const index = await readFile(path.join(root, 'index.html'), 'utf8');
const header = index.match(/<header class="topbar">[\s\S]*?<\/header>/)?.[0];
assert(header, 'The fixture uses the real shell topbar DOM');
const windowLayer = index.match(/<div class="shell-window-layer"[^>]*>/)?.[0];
assert(windowLayer, 'Include the real window layer that can intercept menu clicks');
const styles = index.match(/<link[^>]*rel="stylesheet"[^>]*>/g)?.join('\n') || '';
const fixtureScript = `
import {setTopbarAppItems, installTopbarAvatar} from '/shared/topbar-apps.js';
import {renderCollectionFreshnessWarning} from '/shared/collection-freshness.js';
import '/shared/shell-release-status.js';
const tabs = document.querySelector('[data-module-tabs]');
const account = document.querySelector('[data-open-account]');
account.dataset.authenticated = 'true';
account.querySelector('[data-account-label]').textContent = 'Mona Winter';
installTopbarAvatar(account);
document.querySelector('[data-shell-instance-name]').textContent = 'WELSCH';
document.querySelector('[data-shell-instance-health]').dataset.health = 'healthy';
const launched = [];
const originals = [];
function populate(names = ['Sellify', 'Outbound', 'Mail', 'Crew']) {
  originals.length = 0;
  const buttons = names.map((name, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'module-tab';
    button.dataset.target = String(index);
    button.setAttribute('aria-current', index === 1 ? 'page' : 'false');
    const label = document.createElement('span');
    label.className = 'module-tab-label';
    label.textContent = name;
    button.append(label);
    if (index === 2) {
      const count = document.createElement('span');
      count.className = 'module-tab-count';
      count.textContent = '3';
      button.append(count);
    }
    if (index === 3) {
      const dot = document.createElement('span');
      dot.className = 'module-tab-update';
      dot.setAttribute('aria-label', 'Update verfügbar');
      button.append(dot);
    }
    button.addEventListener('click', () => launched.push(name));
    originals.push(button);
    return button;
  });
  setTopbarAppItems(tabs, buttons);
}
populate();
renderCollectionFreshnessWarning(document.querySelector('[data-collection-freshness-warning]'), {
  compact: true, collections: ['catalogue'], diagnostics: {
    mode: 'webrtc', collections: { catalogue: { frameTransport: {
      collectionFreshnessState: 'live', lastSuccessfulPullAtMs: Date.now()
    }}}
  }
});
window.topbarFixture = { populate, originals, launched };
`;
const html = `<!doctype html><html lang="de" data-theme="dark" data-shell-style="ctox">
<head><meta charset="utf-8"><base href="/"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="ctox-shell-version" content="0.1.0"><meta name="ctox-shell-source-commit" content="0000000000000000000000000000000000000000">
${styles}</head><body data-module-shell="full"><div class="app-shell">${header}
<div style="min-height:0;background:var(--bg)"></div></div>
${windowLayer}<section class="shell-window" style="left:0;top:48px;width:100%;height:calc(100% - 48px)">
<main style="flex:1;background:var(--bg)">Open workspace window</main></section></div>
<div class="drawer-backdrop" hidden data-fixture-modal></div>
<script type="module">${fixtureScript}</script></body></html>`;
const server = createServer(async (request, response) => {
  const pathname = new URL(request.url, 'http://fixture.invalid').pathname;
  try {
    if (pathname === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end(html);
      return;
    }
    const target = path.resolve(root, '.' + decodeURIComponent(pathname));
    if (!target.startsWith(root + path.sep)) { response.writeHead(403); response.end(); return; }
    const data = await readFile(target);
    const extension = path.extname(target);
    const type = { '.css': 'text/css', '.js': 'text/javascript', '.mjs': 'text/javascript', '.svg': 'image/svg+xml', '.png': 'image/png' }[extension] || 'application/octet-stream';
    response.writeHead(200, { 'Content-Type': type });
    response.end(data);
  } catch { response.writeHead(404); response.end(); }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const address = server.address();
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 700 } });
const page = await context.newPage();
const pageErrors = [];
page.on('pageerror', (error) => pageErrors.push(error.message));
const results = [];
try {
  await page.goto('http://127.0.0.1:' + address.port + '/', { waitUntil: 'networkidle' });
  await page.waitForFunction(() => window.topbarFixture?.originals.length === 4 && document.querySelector('[data-module-tabs]').children.length > 0);
  for (const width of [1440, 1000, 390]) {
    await page.setViewportSize({ width, height: 700 });
    await page.waitForFunction(() => {
      const nav = document.querySelector('[data-module-tabs]');
      const right = nav.getBoundingClientRect().right;
      return nav.children.length > 0 && [...nav.children].every((node) => node.className !== 'module-tabs-measure' && node.getBoundingClientRect().right <= right + 1);
    });
    const geometry = await page.evaluate(() => {
      const header = document.querySelector('.topbar').getBoundingClientRect();
      const nav = document.querySelector('[data-module-tabs]').getBoundingClientRect();
      const account = document.querySelector('[data-open-account]').getBoundingClientRect();
      const tabs = [...document.querySelectorAll('[data-module-tabs] > .module-tab')].map((node) => {
        const label = node.querySelector('.module-tab-label');
        return { title: label.textContent, width: label.getBoundingClientRect().width, content: label.scrollWidth, right: node.getBoundingClientRect().right };
      });
      return { height: header.height, navRight: nav.right, accountLeft: account.left, tabs, overflow: document.documentElement.scrollWidth - innerWidth };
    });
    assert.equal(geometry.height, 48, 'Existing outer Shell-V2 topbar height');
    assert(geometry.overflow <= 1, 'No page overflow');
    assert(geometry.navRight <= geometry.accountLeft, 'App row cannot cover account action');
    for (const tab of geometry.tabs) assert(tab.width + 1 >= tab.content, 'Full label: ' + tab.title);
    await page.screenshot({ path: path.join(output, 'topbar-' + width + '.png') });
    results.push({ width, ...geometry });
  }
  await page.waitForSelector('.module-overflow-trigger');
  const trigger = page.locator('.module-overflow-trigger');
  assert.equal(await trigger.textContent(), '+3');
  await trigger.focus();
  await page.keyboard.press('Enter');
  await page.waitForSelector('.module-overflow-menu', { state: 'visible' });
  const menuBox = await page.locator('.module-overflow-menu').boundingBox();
  assert(menuBox.x >= 7 && menuBox.x + menuBox.width <= 391, 'Menu stays within phone viewport: ' + JSON.stringify(menuBox));
  assert.equal(await page.locator('.module-overflow-menu .module-tab').count(), 3);
  await page.locator('.module-overflow-menu .module-tab[data-target="3"]').click();
  assert.deepEqual(await page.evaluate(() => window.topbarFixture.launched), ['Crew'], 'Original launch listener survives overflow');
  await trigger.click();
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.module-overflow').getAttribute('open'), null);
  assert(await trigger.evaluate((node) => node === document.activeElement), 'Escape returns focus');
  await page.evaluate(() => window.topbarFixture.populate(['Sellify', 'Outbound', 'Mail', 'Eine sehr lange Anwendung ohne abgeschnittene Namen: ' + 'W'.repeat(100)]));
  await page.waitForFunction(() => document.querySelector('.module-overflow-trigger')?.textContent === '+3');
  await page.locator('.module-overflow-trigger').click();
  const longName = await page.locator('.module-overflow-menu .module-tab-label').last().evaluate((node) => ({ text: node.textContent, width: node.clientWidth, content: node.scrollWidth }));
  assert(longName.text.endsWith('W'.repeat(100)), 'Menu preserves complete name');
  assert(longName.content <= longName.width + 1, 'Long names wrap within menu');
  await page.screenshot({ path: path.join(output, 'topbar-phone-overflow.png') });
  await page.setViewportSize({ width: 1440, height: 700 });
  await page.evaluate(() => window.topbarFixture.populate());
  await page.waitForFunction(() => document.querySelectorAll('[data-module-tabs] > .module-tab').length === 4);
  assert(await page.evaluate(() => window.topbarFixture.originals.every((button) => button.isConnected)), 'Same buttons remain reachable after resize');
  await page.locator('[data-shell-instance-toggle]').first().click();
  await page.waitForSelector('[data-shell-release-panel]', { state: 'visible' });
  assert.equal(await page.locator('[data-shell-instance-toggle]').first().getAttribute('aria-expanded'), 'true');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('[data-shell-release-panel]').getAttribute('hidden'), '');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('workjet:shell-update-status', { detail: { version: '0.1.0', state: 'recovery', health: 'degraded' } })));
  await page.waitForSelector('[data-shell-recovery-pill]', { state: 'visible' });
  await page.locator('[data-shell-recovery-pill]').click();
  await page.waitForSelector('[data-shell-release-panel]', { state: 'visible' });
  await page.screenshot({ path: path.join(output, 'topbar-recovery.png') });
  await page.keyboard.press('Escape');
  await page.setViewportSize({ width: 390, height: 700 });
  await page.locator('[data-shell-recovery-pill]').click();
  await page.waitForSelector('[data-shell-release-panel]', { state: 'visible' });
  const recoveryBox = await page.locator('[data-shell-release-panel]').boundingBox();
  assert(recoveryBox.x >= 7 && recoveryBox.x + recoveryBox.width <= 391, 'Recovery menu stays within phone viewport: ' + JSON.stringify(recoveryBox));
  await page.getByText('Veröffentlicht', { exact: true }).waitFor({ state: 'visible' });
  await page.getByText('Kompatibilität', { exact: true }).waitFor({ state: 'visible' });
  await page.screenshot({ path: path.join(output, 'topbar-phone-recovery.png') });
  await page.keyboard.press('Escape');
  for (const width of [1000, 390]) {
    await page.setViewportSize({ width, height: 700 });
    for (const loading of [false, true]) {
      await page.evaluate((loading) => {
        if (loading) document.body.dataset.moduleLoading = 'startup';
        else delete document.body.dataset.moduleLoading;
        window.topbarFixture.populate(['Crew', 'Tickets', 'Dokumente', 'Tabellen',
          'Explorer', 'Knowledge', 'App Store', 'Web Research', 'Kalender']);
      }, loading);
      await page.waitForSelector('.module-overflow-trigger', { state: 'visible' });
      const overflow = page.locator('.module-overflow-trigger');
      await overflow.click();
      await page.getByRole('button', { name: 'App Store', exact: true }).click();
      assert.equal(await page.evaluate(() => window.topbarFixture.launched.at(-1)),
        'App Store', 'Pointer launch reaches overflow over a real open window');
      assert.equal(await page.locator('.module-overflow').getAttribute('open'), null);
      await page.locator('[data-shell-recovery-pill]').click();
      const diagnostic = page.getByRole('button', { name: 'Sync-Diagnose öffnen', exact: true });
      await diagnostic.click();
      await page.keyboard.press('Escape');
      await page.evaluate(() => document.querySelector('[data-fixture-modal]').hidden = false);
      const drawerOwnsPointer = await page.locator('[data-shell-recovery-pill]').evaluate((node) => {
        const box = node.getBoundingClientRect();
        return Boolean(document.elementFromPoint(box.x + box.width / 2,
          box.y + box.height / 2)?.closest('[data-fixture-modal]'));
      });
      assert(drawerOwnsPointer, 'Modal drawers remain above the topbar');
      await page.evaluate(() => document.querySelector('[data-fixture-modal]').hidden = true);
      results.push({ width, loading, openWindowPointerLaunch: true, recoveryAction: true,
        modalRetainsPointer: true });
    }
  }
  assert.deepEqual(pageErrors, []);
  await writeFile(path.join(output, 'result.json'), JSON.stringify({ fixture: 'Actual source topbar DOM/CSS + production controllers; no installed acceptance claim', results, keyboard: true, launchesPreserved: true, recoveryMenu: true, pageErrors }, null, 2));
  console.log('TOPBAR_BROWSER_CHECKS_PASS widths1440/1000/390 full names, overflow, pointer launch over open windows/loading, keyboard, recovery, modal priority');
} finally {
  await context.close();
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
