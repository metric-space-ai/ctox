#!/usr/bin/env node
import assert from "node:assert/strict";
import http from "node:http";
import { readFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const arg = process.argv.indexOf("--output-dir");
if (arg < 0 || !process.argv[arg + 1])
  throw new Error("Pass --output-dir for screenshot evidence");
const output = path.resolve(process.argv[arg + 1]);
mkdirSync(output, { recursive: true });
const mime = {
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".html": "text/html",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
};
const server = http.createServer((req, res) => {
  const url = new URL(req.url, "http://localhost");
  if (url.pathname === "/favicon.ico") {
    res.writeHead(204);
    return res.end();
  }
  if (url.pathname === "/") {
    res.writeHead(200, { "content-type": "text/html" });
    return res.end(
      shellHtml(url.searchParams.get("theme") === "light" ? "light" : "dark"),
    );
  }
  const file = path.resolve(root, "." + decodeURIComponent(url.pathname));
  if (!file.startsWith(root + path.sep) || !existsSync(file)) {
    res.writeHead(404);
    return res.end();
  }
  res.writeHead(200, {
    "content-type": mime[path.extname(file)] || "application/octet-stream",
  });
  res.end(readFileSync(file));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({
  headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
    : {}),
});
const errors = [],
  results = [];
try {
  for (const theme of ["dark", "light"]) {
    for (const state of [
      "loading",
      "missing",
      "offline",
      "error",
      "idle",
      "working",
    ]) {
      const page = await browser.newPage({
        viewport: { width: 1500, height: 860 },
      });
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(`${base}/?state=${state}&theme=${theme}`);
      await page.waitForFunction(() => window.fixtureReady);
      const expected = state === "missing" ? "offline" : state;
      try {
        if (state === "working")
          await page
            .locator(".ctox-flow-creature-slot")
            .first()
            .waitFor({ state: "visible" });
        else
          await page
            .locator(`[data-ctox-data-state="${expected}"]`)
            .waitFor({ state: "visible" });
      } catch (error) {
        await page.screenshot({
          path: path.join(output, `failure-${state}-${theme}.png`),
        });
        throw new Error(
          `${error.message}\n${await page.locator("body").innerText()}\n${errors.join("\n")}`,
        );
      }
      if (state === "error")
        assert.match(
          await page.locator("[data-ctox-data-state]").innerText(),
          /IndexedDB read failed/,
        );
      if (state !== "working")
        await page
          .locator(".ctox-data-placeholder")
          .waitFor({ state: "visible" });
      const geometry = await page
        .locator("[data-ctox-main]")
        .evaluate((el) => {
          const luminance = (value) => {
            const channels = value.match(/[\d.]+/g).slice(0, 3).map(Number).map(v => {
              v /= 255;
              return v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4;
            });
            return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
          };
          const text = luminance(getComputedStyle(el.querySelector('.ctox-pane-title')).color);
          const surface = luminance(getComputedStyle(el.querySelector('.ctox-pane-header')).backgroundColor);
          return { width: el.clientWidth, height: el.clientHeight, titleContrast: (Math.max(text, surface) + .05) / (Math.min(text, surface) + .05) };
        });
      assert.ok(geometry.width > 300 && geometry.height > 400);
      assert.ok(geometry.titleContrast >= 4.5, `${theme}: title contrast ${geometry.titleContrast}`);
      // Resolve the shared token in the browser, including dark-theme overrides.
      const lines = await page.locator('[data-ctox-main]').evaluate((el) => {
        const probe = document.createElement('span');
        probe.style.color = 'var(--line)';
        el.append(probe);
        const expected = getComputedStyle(probe).color;
        probe.remove();
        return {
          expected,
          header: getComputedStyle(el.querySelector('.ctox-pane-header')).borderBottomColor,
          well: getComputedStyle(el.querySelector('.ctox-flow-well')).borderTopColor,
        };
      });
      assert.equal(lines.header, lines.expected, `${theme}: header uses Kit line`);
      assert.equal(lines.well, lines.expected, `${theme}: canvas uses Kit line`);
      if (state === 'working') {
        const selector = page.locator('.ctox-task-selector').first();
        await selector.focus();
        assert.notEqual(await selector.evaluate(el => getComputedStyle(el).boxShadow), 'none', `${theme}: Kit keyboard focus ring`);
        await selector.evaluate(el => el.blur());
      }
      await page.screenshot({
        path: path.join(output, `${state}-${theme}.png`),
      });
      if (state === "working") {
        // Failed refresh retains the last successful rows and shows the reason.
        await page.evaluate(() => window.fixtureSetState("error"));
        await page
          .locator('[data-ctox-data-state="error"]')
          .waitFor({ state: "visible" });
        assert.ok(await page.locator(".ctox-flow-creature-slot").count());
        assert.match(
          await page.locator("[data-ctox-left]").innerText(),
          /Bestehende Crew Darstellung prüfen/,
        );
        await page.screenshot({
          path: path.join(output, `cached-error-${theme}.png`),
        });
        await page.evaluate(() => window.fixtureSetState("working"));
        await page.waitForFunction(
          () => !document.querySelector("[data-ctox-data-state]"),
        );
        await page.evaluate(() => window.fixtureSetState("offline"));
        await page.locator('[data-ctox-data-state="offline"]').waitFor({ state: "visible" });
        await page.locator('.ctox-flow-creature-slot .ctox-crew-creature.is-sleeping').waitFor({ state: "visible" });
        await page.screenshot({ path: path.join(output, `cached-offline-${theme}.png`) });
        await page.evaluate(() => window.fixtureSetState("working"));
        await page.waitForFunction(() => !document.querySelector("[data-ctox-data-state]"));
        await page.locator(".ctox-flow-creature-slot").first().click();
        await page.screenshot({
          path: path.join(output, `task-detail-${theme}.png`),
        });
      }
      results.push({ theme, state, ...geometry });
      await page.evaluate(() => window.fixtureDispose?.());
      await page.close();
    }
  }
  assert.deepEqual(errors, []);
  writeFileSync(
    path.join(output, "report.json"),
    JSON.stringify({ ok: true, results, errors }, null, 2),
  );
  console.log(
    `ctox_data_state_ok=1 scenarios=${results.length} cache_recovery=2 themes=dark,light`,
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
function shellHtml(theme) {
  return `<!doctype html><html lang="de" data-theme="${theme}" data-shell-style="ctox"><head><meta charset="utf-8"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css"><style>html,body{margin:0;height:100%;background:var(--bg)}.lab-desk{position:relative;width:100%;height:100vh;overflow:hidden}</style></head><body><div class="lab-desk"><section class="shell-window is-focused" data-shell-window="true" data-shell-contract="v2" data-shell-window-chrome="shared-v2" data-shell-header-rows="2" data-shell-icon-rows="2" data-owner-id="desktop-app:ctox" data-app-mode="window" style="position:absolute;left:20px;top:20px;width:1440px;height:800px"><div class="shell-window-v2-icon"><img alt="" src="/modules/ctox/icon.svg"></div><header class="shell-window-header" data-window-header></header><div class="shell-window-controls"><button class="shell-window-control shell-window-control--close" aria-label="Schließen">×</button></div><div class="shell-window-content"><div class="module-root shell-window-module-root" data-module-root="ctox"><div class="shell-window-module-pane shell-window-module-pane--left"></div><div class="shell-window-module-column-resizer shell-window-module-column-resizer--left"></div><main class="module-content ctox" data-module-content></main><div class="shell-window-module-column-resizer shell-window-module-column-resizer--right"></div><div class="shell-window-module-pane shell-window-module-pane--right"></div></div></div></section></div><script type="module" src="/scripts/ctox-data-state-fixture.mjs"></script></body></html>`;
}
