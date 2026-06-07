#!/usr/bin/env node
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const outputDir = process.env.BUSINESS_CHAT_BEHAVIOR_OUTPUT_DIR
  || path.join(repoRoot, 'output/playwright', `business-chat-behavior-${timestampForPath()}`);
const reportPath = path.join(outputDir, 'business-chat-behavior.json');
const screenshotPath = path.join(outputDir, 'business-chat-behavior.png');
const headless = process.env.BUSINESS_CHAT_BEHAVIOR_HEADLESS !== '0';

fs.mkdirSync(outputDir, { recursive: true });

const { chromium } = require(resolvePlaywrightModule());
const failures = [];
const results = [];
const consoleEvents = [];

const contentTypes = new Map([
  ['.html', 'text/html; charset=utf-8'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.svg', 'image/svg+xml'],
]);

const server = createServer((req, res) => {
  serveRequest(req, res).catch((error) => {
    res.writeHead(500, { 'Content-Type': 'text/plain' });
    res.end(error?.stack || String(error));
  });
});

const port = await listen(server);
const url = `http://127.0.0.1:${port}/`;
const browser = await chromium.launch({
  headless,
  executablePath: existingChromeExecutable(chromium),
  args: ['--disable-gpu'],
});

try {
  const context = await browser.newContext({ viewport: { width: 2048, height: 900 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on('console', (message) => {
    consoleEvents.push({ type: message.type(), text: message.text(), location: message.location() });
  });
  page.on('pageerror', (error) => {
    consoleEvents.push({ type: 'pageerror', text: error?.stack || error?.message || String(error) });
  });
  page.on('requestfailed', (request) => {
    const failure = request.failure();
    if (/favicon/i.test(request.url())) return;
    consoleEvents.push({ type: 'requestfailed', text: `${request.method()} ${request.url()} ${failure?.errorText || ''}` });
  });

  await scenario(page, 'zero-chats-compact', { count: 0 }, (m) => {
    expect(m.storedChats === 0, 'zero state must not create stored chats');
    expect(m.windowCount === 0, 'zero state must not render chat windows');
    expect(m.chipCount === 0, 'zero state must not render chips');
    expect(m.navCount === 0, 'zero state must not render carousel nav');
    expect(m.stripCount === 0, 'zero state must not render an empty strip');
    expect(m.dockNewCount === 1, 'zero state keeps one explicit dock new-chat button');
    expect(m.dockWidth < 360, `zero dock should be compact, got ${m.dockWidth}`);
  });

  await scenario(page, 'future-date-no-phantom-chat', { count: 0 }, async (m) => {
    const after = await page.evaluate(async () => {
      document.querySelector('[data-chat-date-next]').click();
      await window.chatHarness.waitForPaint();
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'future-date-after-next-click', metrics: after });
    expect(m.storedChats === 0, 'future phantom setup must start empty');
    expect(after.storedChats === 0, `date next from empty state must not create chats, got ${after.storedChats}`);
    expect(after.windowCount === 0, 'date next from empty state must not render a phantom window');
    expect(after.stripCount === 0, 'date next from empty state must not render a phantom strip');
  });

  await scenario(page, 'one-chat-compact', { count: 1 }, (m) => {
    expect(m.windowCount === 1, 'one chat renders one window');
    expect(m.chipCount === 1, 'one chat renders one chip');
    expect(m.navCount === 0, 'one chat must not show prev/next controls');
    expect(m.stripCount === 1, 'one chat renders one strip');
    expect(m.headerNewCount === 0, 'window header must not contain new-chat plus button');
    expect(m.dockWidth < 520, `one-chat dock should stay compact, got ${m.dockWidth}`);
  });

  await scenario(page, 'date-workload-popover-heatmap', { count: 100, activeIndex: 50 }, async () => {
    const open = await page.evaluate(async () => {
      document.querySelector('.ctox-date-picker-trigger').click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-date-workload-panel]'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'date-workload-popover-open', metrics: open });
    expect(open.datePanelCount === 1, 'date trigger must open workload panel');
    expect(open.heatmapDayCount === 28, `date workload panel must render 28 heatmap days, got ${open.heatmapDayCount}`);
    expect(open.selectedHeatmapIntensity === '4', `selected busy day should have peak intensity, got ${open.selectedHeatmapIntensity}`);
    expect(open.datePanelTaskText.includes('100'), `date panel summary should show 100 tasks, got ${open.datePanelTaskText}`);
  });

  await scenario(page, 'six-chats-not-full-width', { count: 6, activeIndex: 3 }, (m) => {
    expect(m.chipCount === 6, 'six chats render six chips');
    expect(m.navCount === 2, 'six chats show strip nav');
    expect(m.dockWidth > 900, `six-chat dock should keep visible chips, got ${m.dockWidth}`);
    expect(m.dockWidth < m.viewportWidth * 0.75, `six-chat dock should not be full width, ratio ${m.dockRatio}`);
  });

  await scenario(page, 'eight-chats-scrolls-but-not-full-width', { count: 8, activeIndex: 4 }, (m) => {
    expect(m.chipCount === 8, 'eight chats render eight chips');
    expect(m.navCount === 2, 'eight chats show strip nav');
    expect(m.stripHasOverflow === true, 'eight-chat strip must be horizontally scrollable');
    expect(m.dockWidth < m.viewportWidth * 0.75, `eight-chat dock should avoid premature full width, ratio ${m.dockRatio}`);
  });

  await scenario(page, 'twelve-chats-full-width-scroll', { count: 12, activeIndex: 5 }, async (m) => {
    expect(m.chipCount === 12, 'twelve chats render twelve chips');
    expect(m.navCount === 2, 'twelve chats show strip nav');
    expect(m.stripHasOverflow === true, 'twelve-chat strip must be horizontally scrollable');
    expect(m.dockWidth > m.viewportWidth * 0.85, `twelve-chat dock can span shell width, ratio ${m.dockRatio}`);
    await page.screenshot({ path: screenshotPath, fullPage: true });
  });

  await scenario(page, 'thousand-chats-virtualized-overflow', { count: 1000, activeIndex: 500 }, async (m) => {
    expect(m.storedChats === 1000, `thousand-chat setup must keep source data, got ${m.storedChats}`);
    expect(m.chipCount <= 12, `thousand-chat dock must cap rendered chips, got ${m.chipCount}`);
    expect(m.windowCount <= 12, `thousand-chat dock must cap rendered windows, got ${m.windowCount}`);
    expect(m.overflowCount === 1, 'thousand-chat dock must expose one overflow chip');
    expect(m.workloadBadgeText === '1k', `date workload badge should compact 1000, got ${m.workloadBadgeText}`);
    const open = await page.evaluate(async () => {
      document.querySelector('[data-chat-overflow-open]').click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-busy-panel]'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'thousand-chats-overflow-panel-open', metrics: open });
    expect(open.busyPanelCount === 1, 'overflow click must open busy-day panel');
    expect(open.busyRowCount <= 80, `busy-day panel must cap rendered rows, got ${open.busyRowCount}`);
    expect(open.busyMoreText.includes('weitere'), 'busy-day panel must explain remaining hidden matches');
  });

  await scenario(page, 'hundred-chats-virtualized-overflow', { count: 100, activeIndex: 50 }, async (m) => {
    expect(m.storedChats === 100, `hundred-chat setup must keep source data, got ${m.storedChats}`);
    expect(m.chipCount <= 12, `hundred-chat dock must cap rendered chips, got ${m.chipCount}`);
    expect(m.windowCount <= 12, `hundred-chat dock must cap rendered windows, got ${m.windowCount}`);
    expect(m.overflowCount === 1, 'hundred-chat dock must expose one overflow chip');
    const open = await page.evaluate(async () => {
      document.querySelector('[data-chat-overflow-open]').click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-busy-panel]'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'hundred-chats-overflow-panel-open', metrics: open });
    expect(open.busyRowCount <= 80, `hundred-chat panel must cap rendered rows, got ${open.busyRowCount}`);
    expect(open.busyMoreText.includes('20 weitere'), `hundred-chat panel must show the remaining 20 rows, got ${open.busyMoreText}`);
  });

  await scenario(page, 'inactive-window-click-selects', { count: 4, activeIndex: 2 }, async (m) => {
    expect(m.inactiveFocusable === 0, `inactive controls must not be tabbable, got ${m.inactiveFocusable}`);
    expect(m.inactiveVisibleActions === 0, `inactive header actions must be hidden, got ${m.inactiveVisibleActions}`);
    const before = m.activeId;
    const after = await page.evaluate(async () => {
      const inactive = document.querySelector('.ctox-chat-window:not(.is-active)');
      inactive.click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.dataset.chatId === inactive.dataset.chatId);
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'inactive-window-after-click', metrics: after });
    expect(after.activeId !== before, 'clicking inactive preview body must select it');
    expect(after.inactiveFocusable === 0, `post-select inactive controls must stay out of tab order, got ${after.inactiveFocusable}`);
  });

  await scenario(page, 'keyboard-focus-skips-hidden-and-inactive-controls', { count: 4, activeIndex: 1 }, async () => {
    const focusTrace = [];
    for (let i = 0; i < 18; i += 1) {
      await page.keyboard.press('Tab');
      await page.evaluate(() => window.chatHarness.waitForPaint());
      focusTrace.push(await page.evaluate(() => {
        const active = document.activeElement;
        return {
          tag: active?.tagName || '',
          className: active?.className || '',
          inactiveWindow: Boolean(active?.closest?.('.ctox-chat-window:not(.is-active)')),
          label: active?.getAttribute?.('aria-label') || active?.textContent?.trim()?.slice(0, 40) || '',
        };
      }));
    }
    results.push({ scenario: 'keyboard-focus-trace', focusTrace });
    expect(focusTrace.every((item) => !item.inactiveWindow), 'tab focus must not enter inactive chat windows');
    expect(focusTrace.every((item) => !String(item.className).includes('ctox-date-native-picker')), 'tab focus must not enter hidden native date input');
  });

  await scenario(page, 'active-controls-render-before-db-delay', { count: 1, dbDelay: 180 }, async () => {
    const maximizeLatency = await page.evaluate(async () => {
      const start = performance.now();
      document.querySelector('.ctox-chat-window.is-active [data-chat-maximize]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.classList.contains('is-maximized'));
      return performance.now() - start;
    });
    const minimizeLatency = await page.evaluate(async () => {
      const start = performance.now();
      document.querySelector('.ctox-chat-window.is-active [data-chat-minimize]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-minimized'));
      return performance.now() - start;
    });
    results.push({ scenario: 'active-control-latency-ms', maximizeLatency, minimizeLatency });
    expect(maximizeLatency < 150, `maximize must render before persistence delay, got ${maximizeLatency.toFixed(1)}ms`);
    expect(minimizeLatency < 150, `minimize must render before persistence delay, got ${minimizeLatency.toFixed(1)}ms`);
  });

  await scenario(page, 'chip-selection-render-before-db-delay', { count: 4, activeIndex: 0, dbDelay: 180 }, async () => {
    const chipLatency = await page.evaluate(async () => {
      const start = performance.now();
      document.querySelector('[data-chat-focus="chat_2"]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.dataset.chatId === 'chat_2');
      return performance.now() - start;
    });
    results.push({ scenario: 'chip-selection-latency-ms', chipLatency });
    expect(chipLatency < 150, `chip selection must render before persistence delay, got ${chipLatency.toFixed(1)}ms`);
  });

  await scenario(page, 'active-input-focus-and-type', { count: 1 }, async () => {
    await page.click('.ctox-chat-window.is-active textarea');
    await page.keyboard.type('Browser Test Aufgabe');
    const draftValue = await page.evaluate(() => document.querySelector('.ctox-chat-window.is-active textarea')?.value || '');
    results.push({ scenario: 'active-input-draft-value', draftValue });
    expect(draftValue === 'Browser Test Aufgabe', `active chat textarea must accept typing, got ${JSON.stringify(draftValue)}`);
  });

  await scenario(page, 'active-message-pane-scrolls', { count: 1, messagesPerChat: 28, longMessages: true }, async (m) => {
    expect(m.messagesScrollHeight > m.messagesClientHeight, 'message pane must have scrollable content in long chat');
    const scroll = await page.evaluate(async () => {
      const pane = document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages');
      pane.scrollTop = 0;
      pane.dispatchEvent(new WheelEvent('wheel', { deltaY: 220, bubbles: true, cancelable: true }));
      pane.scrollTop += 220;
      await window.chatHarness.waitForPaint();
      return { top: pane.scrollTop, clientHeight: pane.clientHeight, scrollHeight: pane.scrollHeight };
    });
    results.push({ scenario: 'active-message-scroll-result', scroll });
    expect(scroll.top > 0, 'active message pane must scroll vertically');
  });

  await viewportScenario(page, 'viewport-1440-eight-chats', { width: 1440, height: 820 }, { count: 8, activeIndex: 4 }, (m) => {
    expect(m.dockWidth <= m.viewportWidth - 90, `1440px dock must leave shell space for eight chats, got ${m.dockWidth}`);
    expect(m.chipCount === 8, `1440px eight-chat state should render eight chips, got ${m.chipCount}`);
  });

  await viewportScenario(page, 'viewport-1024-eight-chats', { width: 1024, height: 760 }, { count: 8, activeIndex: 4 }, (m) => {
    expect(m.dockWidth <= m.viewportWidth - 30, `1024px dock must fit shell, got ${m.dockWidth}`);
    expect(m.chipCount === 8, `1024px eight-chat state should render eight chips, got ${m.chipCount}`);
  });

  await viewportScenario(page, 'viewport-760-eight-chats', { width: 760, height: 760 }, { count: 8, activeIndex: 4 }, (m) => {
    expect(m.dockWidth <= m.viewportWidth - 30, `760px dock must fit mobile shell, got ${m.dockWidth}`);
    expect(m.windowCount <= 8, `760px state should not duplicate windows, got ${m.windowCount}`);
  });

  await viewportScenario(page, 'viewport-390-one-chat', { width: 390, height: 760 }, { count: 1 }, (m) => {
    expect(m.dockWidth <= m.viewportWidth - 30, `390px one-chat dock must fit mobile shell, got ${m.dockWidth}`);
    expect(m.navCount === 0, `390px one-chat state must not show chat nav, got ${m.navCount}`);
  });

  const blockingConsole = consoleEvents.filter((event) => {
    if (event.type === 'warning') return false;
    if (/favicon/i.test(event.text || '')) return false;
    return ['error', 'pageerror', 'requestfailed'].includes(event.type);
  });
  expect(blockingConsole.length === 0, `browser console/request errors: ${JSON.stringify(blockingConsole.slice(0, 5))}`);

  writeReport();
  if (failures.length) {
    console.error(JSON.stringify({ ok: false, failures, reportPath, screenshotPath }, null, 2));
    process.exit(1);
  }
  console.log(JSON.stringify({ ok: true, reportPath, screenshotPath, scenarios: results.length }, null, 2));
} finally {
  await browser.close().catch(() => {});
  await new Promise((resolve) => server.close(resolve));
}

async function scenario(page, name, seedOptions, assertions) {
  await page.goto(url, { waitUntil: 'load' });
  await page.waitForFunction(() => window.chatHarness?.seed, null, { timeout: 2500 });
  await page.evaluate(async (options) => {
    await window.chatHarness.seed(options);
  }, seedOptions);
  const metrics = await page.evaluate(() => window.chatHarness.collect());
  results.push({ scenario: name, metrics });
  const failuresBefore = failures.length;
  await assertions(metrics);
  if (failures.length === failuresBefore) results.push({ scenario: `${name}:pass` });
}

async function viewportScenario(page, name, viewport, seedOptions, assertions) {
  await page.setViewportSize(viewport);
  await scenario(page, name, seedOptions, assertions);
  await page.setViewportSize({ width: 2048, height: 900 });
}

function expect(condition, message) {
  if (!condition) failures.push(message);
}

function writeReport() {
  fs.writeFileSync(reportPath, JSON.stringify({
    ok: failures.length === 0,
    failures,
    results,
    consoleEvents,
    screenshotPath,
  }, null, 2));
}

async function serveRequest(req, res) {
  const requestUrl = new URL(req.url || '/', 'http://localhost');
  if (requestUrl.pathname === '/favicon.ico') {
    res.writeHead(204);
    res.end();
    return;
  }
  if (requestUrl.pathname === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(harnessHtml());
    return;
  }
  const filePath = path.normalize(path.join(repoRoot, decodeURIComponent(requestUrl.pathname)));
  if (!filePath.startsWith(repoRoot) || !fs.existsSync(filePath)) {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('not found');
    return;
  }
  const ext = path.extname(filePath);
  res.writeHead(200, { 'Content-Type': contentTypes.get(ext) || 'application/octet-stream' });
  res.end(fs.readFileSync(filePath));
}

function listen(serverInstance) {
  return new Promise((resolve) => {
    serverInstance.listen(0, '127.0.0.1', () => resolve(serverInstance.address().port));
  });
}

function resolvePlaywrightModule() {
  const candidates = [
    process.env.PLAYWRIGHT_MODULE_PATH,
    'playwright',
    '/tmp/ctox-pw-smoke/node_modules/playwright',
    '/tmp/ctox-chatbar-pw/node_modules/playwright',
  ].filter(Boolean);
  for (const candidate of candidates) {
    try {
      return require.resolve(candidate);
    } catch {
      // Try next candidate.
    }
  }
  throw new Error('No Playwright runtime found. Install playwright or set PLAYWRIGHT_MODULE_PATH.');
}

function existingChromeExecutable(chromiumRuntime) {
  const candidates = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
    chromiumRuntime.executablePath?.(),
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function timestampForPath() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function harnessHtml() {
  return `<!doctype html>
<html lang="de">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>CTOX Chatbar Harness</title>
  <style>
    :root {
      --background: #081014;
      --surface: #10181e;
      --surface-2: #17232b;
      --line: #2a3842;
      --text: #e6edf3;
      --muted: #9aa8b4;
      --accent: #10b981;
      --font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    html, body { margin: 0; width: 100%; height: 100%; background: var(--background); color: var(--text); font-family: var(--font-family); }
    body::before {
      content: "";
      position: fixed;
      inset: 0;
      background-image:
        linear-gradient(color-mix(in srgb, var(--line) 35%, transparent) 1px, transparent 1px),
        linear-gradient(90deg, color-mix(in srgb, var(--line) 35%, transparent) 1px, transparent 1px);
      background-size: 56px 56px;
      opacity: 0.42;
    }
    .harness-app {
      position: relative;
      z-index: 1;
      display: grid;
      grid-template-columns: repeat(4, 128px);
      gap: 42px 64px;
      padding: 48px;
    }
    .harness-module {
      display: grid;
      place-items: center;
      width: 96px;
      height: 96px;
      border: 1px solid color-mix(in srgb, var(--line) 40%, transparent);
      border-radius: 18px;
      background: color-mix(in srgb, var(--surface) 45%, transparent);
      color: var(--muted);
      font-weight: 760;
      text-align: center;
    }
  </style>
</head>
<body>
  <script type="module">
    import { initBusinessChat } from '/src/apps/business-os/shared/business-chat.js';

    const CHAT_STATE_KEY = 'ctox.businessOs.chat.v1';
    const owner = 'test-user';

    window.chatHarness = { seed, collect, waitFor, waitForPaint };

    async function seed(options = {}) {
      const oldRoot = document.querySelector('[data-ctox-chat-root]');
      oldRoot?.__ctoxChatCleanup?.();
      if (window._ctoxChatSchedulerInterval) {
        clearInterval(window._ctoxChatSchedulerInterval);
        window._ctoxChatSchedulerInterval = null;
      }
      document.body.innerHTML = '<main class="harness-app">' + ['Tickets', 'Conversations', 'Notizen', 'Documents', 'Knowledge', 'Kunden', 'App Store', 'Source Editor'].map((name) => '<div class="harness-module">' + name + '</div>').join('') + '</main>';
      localStorage.clear();
      sessionStorage.clear();

      const selectedDate = localDateString(addDays(new Date(), options.selectedOffset || 0));
      const chats = Array.from({ length: options.count || 0 }, (_, index) => makeChat({ index, selectedDate, options }));
      const activeIndex = Math.min(Math.max(options.activeIndex || 0, 0), Math.max(chats.length - 1, 0));
      if (chats.length) chats[activeIndex].minimized = false;
      localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
        selectedDate,
        activeChatId: chats[activeIndex]?.id || '',
        dockCollapsed: false,
        preCollapseExpandedChatIds: [],
        chats,
      }));
      initBusinessChat({
        session: { authenticated: true, user: { id: owner, name: 'Harness User' } },
        commandBus: { dispatch: async () => ({ task_id: 'task_harness', command_id: 'cmd_harness', status: 'queued' }) },
        db: makeDb(chats, options.dbDelay || 0),
        getActiveModule: () => ({ id: 'ctox', name: 'CTOX' }),
      });
      await waitFor(() => document.querySelector('[data-chat-dock]'));
      await waitForPaint();
      return collect();
    }

    function makeChat({ index, selectedDate, options }) {
      const createdAt = dateTimestamp(selectedDate, index);
      const modules = ['ctox', 'documents', 'knowledge', 'research', 'matching', 'reports', 'conversations', 'outbound'];
      const messages = [];
      for (let i = 0; i < (options.messagesPerChat || 0); i += 1) {
        messages.push({
          id: 'msg_' + index + '_' + i,
          role: i % 2 ? 'ctox' : 'user',
          text: options.longMessages ? 'Dies ist eine laengere Testnachricht fuer Scroll-Verhalten im aktiven Chat. '.repeat(3) + i : 'Testnachricht ' + i,
          createdAt: createdAt + i * 1000,
        });
      }
      return {
        id: 'chat_' + index,
        title: index % 3 === 0 ? 'Documents be...' : 'CTOX',
        open: true,
        minimized: false,
        maximized: false,
        owner_user_id: owner,
        lastTrackingId: '',
        messages,
        draft: '',
        contextMeta: { module: modules[index % modules.length] },
        createdAt,
        updated_at_ms: createdAt,
        showFollowUp: false,
        attachments: [],
      };
    }

    function makeDb(chats, delayMs) {
      const store = new Map(chats.map((chat) => [chat.id, structuredClone(chat)]));
      const delay = () => new Promise((resolve) => setTimeout(resolve, delayMs));
      const docFor = (id) => {
        const value = store.get(id);
        if (!value) return null;
        return {
          toJSON: () => structuredClone(value),
          incrementalPatch: async (doc) => { await delay(); store.set(id, structuredClone({ ...value, ...doc })); },
          remove: async () => { await delay(); store.delete(id); },
        };
      };
      return {
        raw: {
          business_chats: {
            $: { subscribe: () => ({ unsubscribe() {} }) },
            find: () => ({ exec: async () => { await delay(); return Array.from(store.keys()).map(docFor).filter(Boolean); } }),
            findOne: (id) => ({ exec: async () => { await delay(); return docFor(id); } }),
            insert: async (doc) => { await delay(); store.set(doc.id, structuredClone(doc)); return docFor(doc.id); },
          },
          business_commands: { $: { subscribe: () => ({ unsubscribe() {} }) } },
          ctox_queue_tasks: { $: { subscribe: () => ({ unsubscribe() {} }) } },
        },
      };
    }

    function collect() {
      const root = document.querySelector('[data-ctox-chat-root]');
      const dock = document.querySelector('[data-chat-dock]');
      const strip = document.querySelector('[data-chat-strip]');
      const activeWindow = document.querySelector('.ctox-chat-window.is-active');
      const stored = JSON.parse(localStorage.getItem(CHAT_STATE_KEY) || '{}');
      const activeMessages = document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages');
      const inactiveActions = Array.from(document.querySelectorAll('.ctox-chat-window:not(.is-active) .ctox-chat-header-actions'));
      const inactiveControls = Array.from(document.querySelectorAll('.ctox-chat-window:not(.is-active) button, .ctox-chat-window:not(.is-active) input, .ctox-chat-window:not(.is-active) textarea, .ctox-chat-window:not(.is-active) select, .ctox-chat-window:not(.is-active) a'));
      const dockRect = box(dock);
      return {
        viewportWidth: window.innerWidth,
        rootWidth: box(root).width,
        dockWidth: dockRect.width,
        dockRatio: dockRect.width / window.innerWidth,
        dockClasses: dock?.className || '',
        stripCount: document.querySelectorAll('[data-chat-strip]').length,
        navCount: document.querySelectorAll('[data-chat-prev], [data-chat-next]').length,
        dockNewCount: document.querySelectorAll('[data-chat-dock] > [data-chat-new]').length,
        headerNewCount: document.querySelectorAll('.ctox-chat-window [data-chat-new]').length,
        overflowCount: document.querySelectorAll('[data-chat-overflow-open]').length,
        busyPanelCount: document.querySelectorAll('[data-chat-busy-panel]').length,
        busyRowCount: document.querySelectorAll('[data-chat-list-focus]').length,
        busyMoreText: document.querySelector('.ctox-chat-busy-more')?.textContent || '',
        workloadBadgeText: document.querySelector('.ctox-date-workload-badge')?.textContent || '',
        datePanelCount: document.querySelectorAll('[data-chat-date-workload-panel]').length,
        heatmapDayCount: document.querySelectorAll('[data-chat-date-select]').length,
        selectedHeatmapIntensity: document.querySelector('.ctox-date-heatmap-day.is-selected')?.dataset.intensity || '',
        datePanelTaskText: document.querySelector('[data-chat-date-workload-panel] header span')?.textContent || '',
        chipCount: document.querySelectorAll('[data-chat-focus]').length,
        windowCount: document.querySelectorAll('.ctox-chat-window').length,
        activeId: activeWindow?.dataset.chatId || '',
        stripClientWidth: strip?.clientWidth || 0,
        stripScrollWidth: strip?.scrollWidth || 0,
        stripHasOverflow: strip ? strip.scrollWidth > strip.clientWidth + 1 : false,
        inactiveFocusable: inactiveControls.filter((node) => node.tabIndex >= 0 && isVisible(node)).length,
        inactiveVisibleActions: inactiveActions.filter(isVisible).length,
        messagesClientHeight: activeMessages?.clientHeight || 0,
        messagesScrollHeight: activeMessages?.scrollHeight || 0,
        storedChats: Array.isArray(stored.chats) ? stored.chats.length : 0,
      };
    }

    async function waitFor(predicate, timeout = 2500) {
      const start = performance.now();
      while (performance.now() - start < timeout) {
        if (predicate()) return true;
        await new Promise((resolve) => setTimeout(resolve, 16));
      }
      throw new Error('Timed out waiting for condition');
    }

    async function waitForPaint() {
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    }

    function box(node) {
      if (!node) return { x: 0, y: 0, width: 0, height: 0 };
      const rect = node.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    }

    function isVisible(node) {
      const style = getComputedStyle(node);
      const rect = node.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity || 1) > 0.01 && rect.width > 0 && rect.height > 0;
    }

    function addDays(date, days) {
      const next = new Date(date);
      next.setDate(next.getDate() + days);
      return next;
    }

    function localDateString(date) {
      return date.getFullYear() + '-' + String(date.getMonth() + 1).padStart(2, '0') + '-' + String(date.getDate()).padStart(2, '0');
    }

    function dateTimestamp(dateStr, index) {
      const [year, month, day] = dateStr.split('-').map(Number);
      const hour = 6 + (Math.floor(index / 4) % 18);
      const minute = (index % 4) * 10;
      return new Date(year, month - 1, day, hour, minute, 0, 0).getTime();
    }
  </script>
</body>
</html>`;
}
