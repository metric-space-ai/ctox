#!/usr/bin/env node
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');
const outputDir = process.env.SHELL_CHAT_COMPOSITION_OUTPUT_DIR
  || path.join(repoRoot, 'output/playwright', `shell-chat-composition-${timestampForPath()}`);
const reportPath = path.join(outputDir, 'shell-chat-composition.json');
const screenshotPath = path.join(outputDir, 'shell-chat-composition-expanded.png');
const fixtureMinimum = { width: 640, height: 480 };
fs.mkdirSync(outputDir, { recursive: true });

const { chromium } = require(resolvePlaywrightModule());
const failures = [];
const observations = [];
const consoleEvents = [];
const server = createServer((request, response) => serveRequest(request, response));
let browser;
let page;

try {
  const port = await listen(server);
  const url = `http://127.0.0.1:${port}/`;
  browser = await chromium.launch({
    headless: process.env.SHELL_CHAT_COMPOSITION_HEADLESS !== '0',
    executablePath: existingChromeExecutable(chromium),
    args: ['--disable-gpu'],
  });
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
  page = await context.newPage();
  page.on('console', (message) => consoleEvents.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', (error) => consoleEvents.push({ type: 'pageerror', text: error?.stack || String(error) }));
  page.on('requestfailed', (request) => consoleEvents.push({ type: 'requestfailed', text: `${request.method()} ${request.url()}` }));

  await page.goto(url, { waitUntil: 'load' });
  await page.waitForFunction(() => window.shellHarness?.ready === true, null, { timeout: 5000 });

  const expanded = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-normal', ...expanded });
  expect(expanded.chatExpanded, 'expanded chat must set the shell composition state');
  expect(!expanded.chatSide, 'chat must never move into a right-hand side rail');
  expect(!expanded.chatCompact, 'desktop chat must retain its conversation stage');
  expect(closeRect(expanded.window, expanded.baselineWindow), `expanding chat must not move a normal window: ${JSON.stringify({ actual: expanded.window, expected: expanded.baselineWindow })}`);
  expect(expanded.chat.x >= 0 && expanded.chat.right <= expanded.viewport.width, 'chat root must remain inside the viewport');
  expect(expanded.dock.bottom <= expanded.viewport.height, 'chat dock must remain anchored inside the bottom viewport edge');
  expect(expanded.bottomAppSwitcherPresent === false, 'bottom app switcher must not exist');

  const windowAction = page.locator('[data-window-action]');
  expect(await windowAction.count() === 1, 'window action locator must be unique');
  await windowAction.click();
  expect(await page.evaluate(() => window.shellHarness.windowClicks) === 1, 'real pointer click must reach the window action');

  const topAppTab = page.locator('[data-top-app-tab]');
  expect(await topAppTab.count() === 1, 'top app tab locator must be unique');
  // Shell-V2 exposes the layout menu immediately left of close. Its seven
  // actions replace drag-edge workspace snapping and the old direct title-bar
  // minimize/maximize controls.
  const titleBarControls = await page.evaluate(
    () => [...document.querySelectorAll('.shell-window [data-window-control]')].map((node) => node.dataset.windowControl),
  );
  expect(
    JSON.stringify(titleBarControls) === JSON.stringify(['layout', 'close']),
    `Shell-V2 windows expose layout immediately left of close: ${JSON.stringify(titleBarControls)}`,
  );
  const layoutTrigger = page.locator('.shell-window [data-window-layout-trigger]');
  const triggerBox = await layoutTrigger.boundingBox();
  const closeBox = await page.locator('.shell-window [data-window-control="close"]').boundingBox();
  expect(triggerBox && closeBox && triggerBox.width > 0 && triggerBox.height > 0
    && closeBox.width > 0 && closeBox.height > 0
    && triggerBox.x + triggerBox.width <= closeBox.x + 1,
  'the visible layout trigger must sit left of Close');
  expect(await layoutTrigger.locator('.shell-window-layout-glyph--free').count() === 1,
    'the layout trigger must show its actual framed window glyph');
  await layoutTrigger.click();
  const layoutOptions = await page.locator('.shell-window [data-window-layout-menu] [data-window-layout-control]')
    .evaluateAll((nodes) => nodes.map((node) => node.dataset.windowLayoutControl));
  expect(
    JSON.stringify(layoutOptions) === JSON.stringify(['free', 'maximize', 'minimize', 'left', 'right', 'top', 'bottom']),
    `the layout menu must expose the seven requested actions: ${JSON.stringify(layoutOptions)}`,
  );
  const layoutMenu = page.locator('.shell-window [data-window-layout-menu]');
  const glyphs = await layoutMenu.locator('[data-window-layout-control]').evaluateAll(nodes => nodes.map(node => {
    const glyph = node.querySelector('.shell-window-layout-glyph');
    const rect = glyph?.getBoundingClientRect();
    const style = glyph && getComputedStyle(glyph);
    const detail = glyph && getComputedStyle(glyph, '::after');
    return { action: node.dataset.windowLayoutControl,
      correctVariant: Boolean(glyph?.classList.contains('shell-window-layout-glyph--' + node.dataset.windowLayoutControl)),
      width: rect?.width, height: rect?.height, frameWidth: Number.parseFloat(style?.borderTopWidth),
      frameStyle: style?.borderTopStyle, frameColor: style?.borderTopColor,
      detailContent: detail?.content, detailWidth: Number.parseFloat(detail?.width), detailHeight: Number.parseFloat(detail?.height) };
  }));
  expect(glyphs.length === 7 && glyphs.every(glyph => glyph.correctVariant
    && glyph.width > 0 && glyph.height > 0 && glyph.frameWidth > 0 && glyph.frameStyle !== 'none'
    && !['transparent', 'rgba(0, 0, 0, 0)'].includes(glyph.frameColor)
    && glyph.detailContent !== 'none' && glyph.detailWidth > 0 && glyph.detailHeight > 0),
  'each of the seven choices must render its own nonempty framed window glyph and layout detail');
  observations.push({ phase: 'layout-visible-glyphs', glyphs });
  await topAppTab.click();
  const outsideClickClosed = !(await layoutMenu.isVisible()) && await layoutTrigger.getAttribute('aria-expanded') === 'false';
  expect(outsideClickClosed,
    'clicking outside the window must close the layout menu');
  await layoutTrigger.focus();
  await layoutTrigger.press('Enter');
  const enterOpened = await layoutMenu.isVisible() && await layoutTrigger.getAttribute('aria-expanded') === 'true';
  expect(enterOpened,
    'Enter on the window icon must open the layout menu');
  await layoutMenu.locator('[data-window-layout-control="free"]').focus();
  await page.keyboard.press('Escape');
  const escapeClosed = !(await layoutMenu.isVisible()) && await layoutTrigger.getAttribute('aria-expanded') === 'false';
  expect(escapeClosed,
    'Escape from a layout choice must close the menu');
  const focusReturned = await layoutTrigger.evaluate((node) => document.activeElement === node);
  expect(focusReturned,
    'Escape must return keyboard focus to the window icon');
  observations.push({ phase: 'layout-keyboard-open-dismiss', outsideClickClosed,
    enterOpened, escapeClosed, focusReturned });
  await layoutTrigger.press('Enter');
  await layoutMenu.locator('[data-window-layout-control="minimize"]').focus();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display === 'none');
  observations.push({ phase: 'layout-keyboard-minimize',
    ...await page.evaluate(() => window.shellHarness.collect()) });
  await topAppTab.click();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display !== 'none');
  await page.waitForSelector('[data-top-app-tab][data-state="focused"]');
  const restoredFromTopTab = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-restored-from-top-tab', ...restoredFromTopTab });
  expect(closeRect(restoredFromTopTab.window, restoredFromTopTab.baselineWindow), 'restoring from the top tab must preserve normal window geometry');
  expect(!restoredFromTopTab.chatSide, 'restoring an app must not move chat to the side');
  await chooseLayout(page, 'minimize');
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display === 'none');
  await topAppTab.focus();
  await topAppTab.press('Enter');
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display !== 'none');
  await page.waitForSelector('[data-top-app-tab][data-state="focused"]');
  await page.screenshot({ path: screenshotPath, fullPage: true });

  const chatToggle = page.locator('[data-chat-open]');
  expect(await chatToggle.count() === 1, 'chat toggle locator must be unique');
  await chatToggle.click();
  await page.waitForFunction(() => !document.body.hasAttribute('data-shell-chat-dock-expanded'));
  const collapsed = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'collapsed-restored', ...collapsed });
  expect(!collapsed.chatExpanded, 'collapsed chat must clear the shell composition state');
  expect(closeRect(collapsed.window, collapsed.baselineWindow), `normal window geometry must restore after collapse: ${JSON.stringify({ actual: collapsed.window, expected: collapsed.baselineWindow })}`);

  await chatToggle.click();
  await page.waitForFunction(() => document.body.hasAttribute('data-shell-chat-dock-expanded'));
  await chooseLayout(page, 'maximize');
  const maximized = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-maximized', ...maximized,
    expectedWindow: expectFixedLayout(maximized, await page.evaluate(() => window.shellHarness.workArea()), 'maximize') });
  await chooseLayout(page, 'maximize');
  const maximizedAgain = await page.evaluate(() => window.shellHarness.collect());
  expect(maximizedAgain.windowState === 'maximized' && closeRect(maximizedAgain.window, maximized.window),
    'choosing maximize again must remain maximized, not toggle back to free');
  // Window-neutral overlay contract (shared/shell-chat-composition.js,
  // f0d376fbc 2026-07-17 "chat as window-neutral overlay"): the chat dock
  // floats ABOVE app windows and reserves ZERO work-area inset, so a maximized
  // window fills the full work area and the dock overlaps it by design. The
  // real invariant to guard is that the dock steals no bottom space — a
  // reintroduced inset would shrink the window and fail this.
  expect(maximized.window.bottom >= maximized.viewport.height - 10, `maximized window must fill the full work area under the floating chat dock (no reserved bottom inset): ${JSON.stringify({ windowBottom: maximized.window.bottom, viewportHeight: maximized.viewport.height })}`);
  expect(maximized.window.right <= maximized.viewport.width, 'maximized window must use the full shell width');

  await chooseLayout(page, 'bottom');
  const snapped = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-bottom-snap', ...snapped,
    expectedWindow: expectFixedLayout(snapped, await page.evaluate(() => window.shellHarness.workArea()), 'bottom') });
  expect(snapped.window.bottom >= snapped.viewport.height - 10, `bottom-snapped window must reach the work-area bottom under the floating chat dock (no reserved inset): ${JSON.stringify({ windowBottom: snapped.window.bottom, viewportHeight: snapped.viewport.height })}`);
  expect(snapped.window.height >= 199, `bottom snap must preserve the minimum window height, got ${snapped.window.height}`);

  const layoutEventsBeforeResize = await page.evaluate(() => window.shellHarness.layoutEvents);
  await page.setViewportSize({ width: 900, height: 600 });
  await page.waitForFunction((previous) => window.shellHarness.layoutEvents > previous, layoutEventsBeforeResize);
  const resized = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-resized-viewport', ...resized });
  expect(resized.window.bottom >= resized.viewport.height - 10, `resized snapped window must keep filling the work area under the floating chat dock (no reserved inset): ${JSON.stringify({ windowBottom: resized.window.bottom, viewportHeight: resized.viewport.height })}`);
  expect(!resized.chatSide && !resized.chatCompact, 'resizing must not switch chat into an alternate rail mode');
  expect(resized.chat.right <= resized.viewport.width, 'resized chat must remain inside the viewport');

  await page.setViewportSize({ width: 1200, height: 620 });
  await page.waitForFunction(() => window.shellHarness.collect().viewport.width === 1200);
  await page.evaluate(() => window.shellHarness.addInactiveChatClones());
  const chatWindows = await page.evaluate(() => window.shellHarness.collectChatWindows());
  observations.push({ phase: 'multi-chat-bottom-dock', chats: chatWindows });
  expect(chatWindows.length >= 1, 'bottom chat stage must retain at least the active conversation');
  for (const chat of chatWindows.filter((entry) => entry.active)) {
    expect(chat.x >= 0 && chat.right <= 1200, `active chat must remain inside viewport: ${JSON.stringify(chat)}`);
  }

  // Workspace edges no longer trigger Shell-V2 snapping. Each edge remains a
  // free move until the user explicitly selects its layout-menu action.
  await chooseLayout(page, 'free');
  const freedFromBottom = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'layout-free-after-bottom', ...freedFromBottom });
  expect(freedFromBottom.snapZone === null, 'free layout must release the bottom snap');
  await page.evaluate(({ width, height }) => window.shellHarness.setSize(width, height), fixtureMinimum);
  const work = await page.evaluate(() => window.shellHarness.workArea());
  const inset = 40;

  await dragWindowToLayerPoint(page, work, { left: work.left + 120, top: work.top + inset });
  const freelyMoved = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-move', ...freelyMoved, work });
  expect(freelyMoved.snapZone === null, `moving a window inside the desktop must not force a snap, got ${freelyMoved.snapZone}`);

  await dragWindowToLayerPoint(page, work, { left: work.left + 2, top: work.top + inset });
  const leftEdge = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-left-edge', ...leftEdge });
  expect(leftEdge.snapZone === null, `dragging to the left work edge must remain free, got ${leftEdge.snapZone}`);
  await chooseLayout(page, 'left');
  const leftSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'layout-snap-left', ...leftSnap,
    expectedWindow: expectFixedLayout(leftSnap, work, 'left') });
  expect(leftSnap.snapZone === 'left', `the left menu action must snap left, got ${leftSnap.snapZone}`);

  await chooseLayout(page, 'free');
  await page.evaluate(({ width, height }) => window.shellHarness.setSize(width, height), fixtureMinimum);
  const rightStart = await page.evaluate(() => window.shellHarness.collect());
  await dragWindowToLayerPoint(page, work, { left: work.left + work.width - rightStart.window.width - 2, top: work.top + inset });
  const rightEdge = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-right-edge', ...rightEdge });
  expect(rightEdge.snapZone === null, `dragging to the right work edge must remain free, got ${rightEdge.snapZone}`);
  await chooseLayout(page, 'right');
  const rightSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'layout-snap-right', ...rightSnap,
    expectedWindow: expectFixedLayout(rightSnap, work, 'right') });
  expect(rightSnap.snapZone === 'right', `the right menu action must snap right, got ${rightSnap.snapZone}`);

  await chooseLayout(page, 'free');
  await page.evaluate(({ width, height }) => window.shellHarness.setSize(width, height), fixtureMinimum);
  await dragWindowToLayerPoint(page, work, { left: work.left + 120, top: work.top + 2 });
  const topEdge = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-top-edge', ...topEdge });
  expect(topEdge.snapZone === null, `dragging to the top work edge must remain free, got ${topEdge.snapZone}`);
  await chooseLayout(page, 'top');
  const topSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'layout-snap-top', ...topSnap,
    expectedWindow: expectFixedLayout(topSnap, work, 'top') });
  expect(topSnap.snapZone === 'top', `the top menu action must snap top, got ${topSnap.snapZone}`);
  await chooseLayout(page, 'free');

  await page.evaluate(({ width, height }) => window.shellHarness.setSize(width, height), fixtureMinimum);
  const bottomStart = await page.evaluate(() => window.shellHarness.collect());
  await dragWindowToLayerPoint(page, work, {
    left: work.left + 120,
    top: work.top + work.height - bottomStart.window.height - 2,
  });
  const bottomEdge = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-bottom-edge', ...bottomEdge });
  expect(bottomEdge.snapZone === null, `dragging to the bottom work edge must remain free, got ${bottomEdge.snapZone}`);
  await chooseLayout(page, 'bottom');
  const bottomSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'layout-snap-bottom', ...bottomSnap,
    expectedWindow: expectFixedLayout(bottomSnap, work, 'bottom') });
  expect(bottomSnap.snapZone === 'bottom', `the bottom menu action must snap bottom, got ${bottomSnap.snapZone}`);
  await chooseLayout(page, 'free');

  // Resize through the actual focusable corner, rather than counting a harness
  // style assignment as a user resize. Start with room for both 16px steps.
  await page.evaluate(() => window.shellHarness.setSize(640, 480));
  await dragWindowToLayerPoint(page, work, { left: work.left + 40, top: work.top + 40 });
  const beforeKeyboardResize = await page.evaluate(() => window.shellHarness.collect());
  const resizeCorner = page.locator('.shell-window [data-window-resize="se"]');
  expect(await resizeCorner.count() === 1 && await resizeCorner.isVisible(),
    'the free window must expose its actual southeast resize corner');
  await resizeCorner.focus();
  await resizeCorner.press('ArrowRight');
  await resizeCorner.press('ArrowDown');
  const keyboardResized = await page.evaluate(() => window.shellHarness.collect());
  const expectedResized = { ...beforeKeyboardResize.window,
    width: beforeKeyboardResize.window.width + 16, height: beforeKeyboardResize.window.height + 16 };
  expect(closeRect(keyboardResized.window, expectedResized),
    `corner keyboard resize must grow both dimensions without moving: ${JSON.stringify({ before: beforeKeyboardResize.window, expected: expectedResized, after: keyboardResized.window })}`);
  expect(keyboardResized.snapZone === null && keyboardResized.windowState === 'normal',
    'resizing a free window must not choose a fixed layout');
  observations.push({ phase: 'free-keyboard-resize', before: beforeKeyboardResize.window,
    expected: expectedResized, ...keyboardResized });
  await reloadHarness(page, url);
  const reopenedResized = await page.evaluate(() => window.shellHarness.collect());
  expect(closeRect(reopenedResized.window, keyboardResized.window)
    && reopenedResized.snapZone === null && reopenedResized.windowState === 'normal',
  'the user-resized free window must retain its geometry and free state after reload');
  observations.push({ phase: 'reopened-free-resized', ...reopenedResized });

  // Reopening must use the last explicit menu selection, including returning
  // to free geometry. The harness supplies the real manager persistence port.
  for (const action of ['maximize', 'left', 'right', 'top', 'bottom']) {
    await chooseLayout(page, action);
    const beforeReopen = await page.evaluate(() => window.shellHarness.collect());
    await reloadHarness(page, url);
    const reopened = await page.evaluate(() => window.shellHarness.collect());
    observations.push({ phase: `reopened-${action}`, ...reopened,
      expectedWindow: expectFixedLayout(reopened, await page.evaluate(() => window.shellHarness.workArea()), action) });
    expect(closeRect(reopened.window, beforeReopen.window), `${action} geometry must survive reopening`);
    expect(reopened.snapZone === beforeReopen.snapZone, `${action} snap selection must survive reopening`);
    expect(reopened.windowState === beforeReopen.windowState, `${action} window state must survive reopening`);
    await chooseLayout(page, 'free');
    const freeBeforeReopen = await page.evaluate(() => window.shellHarness.collect());
    const savedFree = await page.evaluate(() => JSON.parse(localStorage.getItem('composition-window-layout')));
    expect(savedFree?.state === 'normal' && !savedFree?.snapZone, `free after ${action} must persist the cleared layout`);
    await reloadHarness(page, url);
    const freeReopened = await page.evaluate(() => window.shellHarness.collect());
    observations.push({ phase: `reopened-free-after-${action}`, ...freeReopened });
    expect(freeReopened.snapZone === null && freeReopened.windowState === 'normal', `free after ${action} must stay free on reopen`);
    expect(closeRect(freeReopened.window, freeBeforeReopen.window), `free geometry after ${action} must survive reopening`);
  }

  const fatalConsole = consoleEvents.filter((event) => ['pageerror', 'requestfailed', 'error'].includes(event.type));
  expect(fatalConsole.length === 0, `browser console/network must stay clean: ${JSON.stringify(fatalConsole)}`);

  fs.writeFileSync(reportPath, JSON.stringify({
    ok: failures.length === 0,
    failures,
    observations,
    consoleEvents,
    screenshotPath,
  }, null, 2));

  if (failures.length) {
    console.error(JSON.stringify({ ok: false, failures, reportPath, screenshotPath }, null, 2));
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify({ ok: true, reportPath, screenshotPath, phases: observations.length }, null, 2));
  }
} catch (error) {
  // A failing click must retain its actual hit target and completed phases
  // before teardown, not leave a stale success report from an earlier run.
  failures.push(error?.stack || String(error));
  let failureSnapshot = null;
  let failureCaptureError = null;
  let captureTimer;
  try {
    if (page) failureSnapshot = await Promise.race([
      page.evaluate(() => {
        const trigger = document.querySelector('.shell-window [data-window-layout-trigger]');
        const rect = trigger?.getBoundingClientRect();
        const describe = node => ({ tag: node.tagName, className: node.className?.baseVal ?? node.className });
        return {
          state: window.shellHarness?.collect(),
          trigger: rect ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height } : null,
          triggerCenterHits: rect ? document.elementsFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)
            .slice(0, 12).map(describe) : [],
          chatWindows: window.shellHarness?.collectChatWindows(),
        };
      }),
      new Promise((_, reject) => { captureTimer = setTimeout(() => reject(new Error('failure snapshot deadline')), 3000); }),
    ]);
    if (page) await page.screenshot({ path: path.join(outputDir, 'shell-chat-composition-failure.png'), fullPage: true, timeout: 3000 });
  } catch (captureError) {
    failureCaptureError = captureError?.message || String(captureError);
  } finally {
    clearTimeout(captureTimer);
  }
  try {
    fs.writeFileSync(reportPath, JSON.stringify({ ok: false, failures, observations, consoleEvents,
      failureSnapshot, failureCaptureError, screenshotPath }, null, 2));
  } catch { /* Evidence failure must not replace the original failure or skip cleanup. */ }
  throw error;
} finally {
  if (browser) await browser.close().catch(() => {});
  if (server.listening) await new Promise((resolve) => server.close(resolve));
}

function expect(condition, message) {
  if (!condition) failures.push(message);
}

function closeRect(actual, expected, tolerance = 1) {
  return ['x', 'y', 'width', 'height'].every((key) => Math.abs(actual[key] - expected[key]) <= tolerance);
}

function expectFixedLayout(observation, work, action) {
  // Independent fixture oracle: a fixed layout fills the work area or anchors
  // a half-size pane to its selected edge, respecting this app's declared min.
  // Do not ask the manager for its target rectangle: that would verify itself.
  expect(work.width >= fixtureMinimum.width && work.height >= fixtureMinimum.height,
    'fixed-layout fixture must have enough work area for its declared minimum');
  const expected = { x: work.originLeft + work.left, y: work.originTop + work.top,
    width: work.width, height: work.height };
  if (action === 'left' || action === 'right') {
    expected.width = Math.max(fixtureMinimum.width, work.width / 2);
    if (action === 'right') expected.x += work.width - expected.width;
  } else if (action === 'top' || action === 'bottom') {
    expected.height = Math.max(fixtureMinimum.height, work.height / 2);
    if (action === 'bottom') expected.y += work.height - expected.height;
  } else if (action !== 'maximize') {
    throw new Error(`No fixed-layout fixture oracle for ${action}`);
  }
  expect(closeRect(observation.window, expected, 2),
    `${action} must apply real anchored bounds, not only a state marker: ${JSON.stringify({ expected, actual: observation.window })}`);
  return expected;
}

async function chooseLayout(page, action) {
  await page.locator('.shell-window [data-window-layout-trigger]').click();
  await page.locator(`.shell-window [data-window-layout-menu] [data-window-layout-control="${action}"]`).click();
}

async function reloadHarness(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await page.waitForFunction(() => window.shellHarness?.ready === true, null, { timeout: 5000 });
}

async function dragWindowHeaderTo(page, targetX, targetY) {
  const grab = await windowDragGrabPoint(page);
  const startX = grab.x;
  const startY = grab.y;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(targetX ?? startX, targetY ?? startY, { steps: 12 });
  await page.mouse.up();
  await page.waitForTimeout(80);
}

// The window manager refuses a drag that starts on a control
// (`interactiveSelector` in window-manager.js), and the Shell-V2 header row
// packs title, meta, actions and controls across its width — the old fixed
// 55%-of-width grab point landed on a button, so every drag silently did
// nothing and the snap assertions failed on an intact product. Grab the first
// spot the shell itself would accept.
async function windowDragGrabPoint(page) {
  const point = await page.evaluate(() => {
    const interactive = 'button, a, input, select, textarea, [contenteditable="true"],'
      + ' [role="button"], [role="tab"], [data-window-controls], [data-window-actions],'
      + ' [data-window-header-action], [data-resizer]';
    // Every region the shell declares as draggable, icon block first: in
    // Shell-V2 the header row is fully covered by title, actions and controls,
    // so the icon block is the move affordance the product actually offers.
    const regions = [...document.querySelectorAll('.shell-window [data-window-drag-region]')]
      .sort((a, b) => Number(b.matches('.shell-window-v2-icon')) - Number(a.matches('.shell-window-v2-icon')));
    for (const region of regions) {
      const rect = region.getBoundingClientRect();
      if (rect.width < 2 || rect.height < 2) continue;
      const y = rect.y + rect.height / 2;
      for (let ratio = 0.05; ratio <= 0.95; ratio += 0.02) {
        const x = rect.x + rect.width * ratio;
        const hit = document.elementFromPoint(x, y);
        if (!hit || !region.contains(hit)) continue;
        // The region itself may carry role="button"; only a control *inside* it
        // blocks the drag.
        const blocker = hit.closest(interactive);
        if (blocker && blocker !== region && region.contains(blocker)) continue;
        return { x, y };
      }
    }
    return null;
  });
  if (!point) throw new Error('no draggable spot on the window header');
  return point;
}

// Move the window so its top-left lands on the given work-area (layer)
// coordinates; the pointer carries the window by delta, so the grab point moves
// by the same amount. Layer coordinates become client coordinates through the
// layer origin the window manager reports.
async function dragWindowToLayerPoint(page, work, { left, top }) {
  const grab = await windowDragGrabPoint(page);
  const rect = await page.evaluate(() => {
    const el = document.querySelector('.shell-window').getBoundingClientRect();
    return { left: el.left, top: el.top, width: el.width, height: el.height };
  });
  await page.mouse.move(grab.x, grab.y);
  await page.mouse.down();
  await page.mouse.move(
    grab.x + (work.originLeft + left - rect.left),
    grab.y + (work.originTop + top - rect.top),
    { steps: 12 },
  );
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const during = await page.evaluate(() => window.shellHarness.collect());
  expect(during.snapPreviewVisible === false, 'moving near an edge must not show a workspace snap preview during the drag');
  await page.mouse.up();
  await page.waitForTimeout(80);
  const after = await page.evaluate(() => window.shellHarness.collect());
  const expected = {
    x: work.originLeft + left, y: work.originTop + top,
    width: rect.width, height: rect.height,
  };
  expect(closeRect(after.window, expected, 2), `drag must reach the requested free position without resizing: ${JSON.stringify({ before: rect, expected, after: after.window })}`);
  expect(Math.abs(after.window.x - rect.left) > 1 || Math.abs(after.window.y - rect.top) > 1,
    `drag must actually move the window: ${JSON.stringify({ before: rect, after: after.window })}`);
  expect(after.snapZone === null && after.windowState === 'normal', 'edge drag must preserve free window state');
  expect(after.snapPreviewVisible === false, 'edge drag must not show a workspace snap preview');
  observations.push({ phase: 'coordinate-verified-free-drag', before: rect, expected, after: after.window,
    duringSnapPreviewVisible: during.snapPreviewVisible, afterSnapPreviewVisible: after.snapPreviewVisible });
}

function serveRequest(request, response) {
  const requestUrl = new URL(request.url || '/', 'http://localhost');
  if (requestUrl.pathname === '/favicon.ico') {
    response.writeHead(204).end();
    return;
  }
  if (requestUrl.pathname === '/') {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(harnessHtml());
    return;
  }
  const filePath = path.normalize(path.join(repoRoot, decodeURIComponent(requestUrl.pathname)));
  if (!filePath.startsWith(repoRoot) || !fs.existsSync(filePath)) {
    response.writeHead(404, { 'Content-Type': 'text/plain' });
    response.end('not found');
    return;
  }
  const contentTypes = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml' };
  response.writeHead(200, { 'Content-Type': contentTypes[path.extname(filePath)] || 'application/octet-stream' });
  response.end(fs.readFileSync(filePath));
}

function listen(serverInstance) {
  return new Promise((resolve) => serverInstance.listen(0, '127.0.0.1', () => resolve(serverInstance.address().port)));
}

function resolvePlaywrightModule() {
  for (const candidate of [
    process.env.PLAYWRIGHT_MODULE_PATH,
    'playwright',
    path.join(repoRoot, 'src/apps/business-os/node_modules/playwright'),
    '/tmp/ctox-pw-smoke/node_modules/playwright',
    '/tmp/ctox-chatbar-pw/node_modules/playwright',
  ].filter(Boolean)) {
    try { return require.resolve(candidate); } catch {}
  }
  throw new Error('No Playwright runtime found. Set PLAYWRIGHT_MODULE_PATH.');
}

function existingChromeExecutable(chromiumRuntime) {
  return [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
    chromiumRuntime.executablePath?.(),
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
  ].filter(Boolean).find((candidate) => fs.existsSync(candidate));
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
  <link rel="stylesheet" href="/src/apps/business-os/app.css">
  <style>
    :root { --bg:#111315; --surface:#171a1d; --surface-2:#1d2125; --line:#30363b; --text:#e6e9eb; --muted:#9ba4aa; --accent:#72b8aa; --accent-soft:#173c38; --hairline:#2a3035; --panel-shadow:none; }
    html, body { margin:0; width:100%; height:100%; overflow:hidden; background:var(--bg); }
    .workspace-frame { position:fixed; inset:52px 0 0; display:block; }
    .harness-topbar { position:fixed; inset:0 0 auto; height:52px; display:flex; align-items:center; padding:0 8px; border-bottom:1px solid var(--line); background:var(--surface); }
    .harness-topbar button { min-height:36px; }
    .harness-window-content { display:flex; align-items:flex-start; justify-content:center; height:100%; padding:12px; }
    .harness-window-content button { min-height:32px; }
  </style>
</head>
<body>
  <header class="harness-topbar"><button type="button" data-top-app-tab>Testfenster</button></header>
  <main class="workspace-frame" data-surface></main>
  <div class="shell-window-layer" data-window-layer><div class="shell-snap-preview" data-snap-preview hidden></div></div>
  <script type="module">
    import { initBusinessChat } from '/src/apps/business-os/shared/business-chat.js';
    import { createEventBus } from '/src/apps/business-os/shared/event-bus.js';
    import { createWindowManager } from '/src/apps/business-os/shared/window-manager.js';
    import { createShellChatCompositionController } from '/src/apps/business-os/shared/shell-chat-composition.js';

    const owner = 'composition-user';
    const chat = {
      id:'chat_composition', title:'Dock composition', open:true, minimized:false, maximized:false,
      owner_user_id:owner, messages:[], draft:'', contextMeta:{ module:'threads' },
      createdAt:Date.now(), updated_at_ms:Date.now(), attachments:[], showFollowUp:false,
    };
    localStorage.setItem('ctox.businessOs.chat.v1', JSON.stringify({
      selectedDate: localDateString(new Date()), activeChatId:chat.id, dockCollapsed:false, chats:[chat],
    }));

    let layoutEvents = 0;
    window.addEventListener('ctox-business-os-chat-layout', () => { layoutEvents += 1; });
    const eventBus = createEventBus();
    const wm = createWindowManager({
      windowLayer:document.querySelector('[data-window-layer]'),
      surfaceEl:document.querySelector('[data-surface]'),
      rootEl:document.documentElement,
      snapPreviewEl:document.querySelector('[data-snap-preview]'),
      eventBus,
      persistence:{
        load:() => JSON.parse(localStorage.getItem('composition-window-layout') || 'null'),
        save:(_ownerId, snapshot) => localStorage.setItem('composition-window-layout', JSON.stringify(snapshot)),
      },
    });
    wm.setInsets({ top:0, right:0, bottom:0, left:0 });
    const controller = createShellChatCompositionController({ windowManager:wm });
    controller.start();
    ['window:opened','window:closed','window:minimized','window:restored'].forEach((name) => eventBus.on(name, () => controller.refresh()));
    const handle = wm.create({
      ownerId:'module:test', title:'Testfenster', x:80, y:60, width:1000, height:610, minWidth:${fixtureMinimum.width}, minHeight:${fixtureMinimum.height},
      content:'<div class="harness-window-content"><button type="button" data-window-action>Freigabe ausführen</button></div>',
    });
    let windowClicks = 0;
    document.querySelector('[data-window-action]').addEventListener('click', () => { windowClicks += 1; });
    const topAppTab = document.querySelector('[data-top-app-tab]');
    const syncTopAppTab = () => {
      const state = wm.describe(handle.id)?.state;
      topAppTab.dataset.state = state === 'minimized' ? 'running' : 'focused';
    };
    topAppTab.addEventListener('click', () => {
      const state = wm.describe(handle.id)?.state;
      if (state === 'minimized') {
        wm.restore(handle.id);
        wm.focus(handle.id);
      } else {
        wm.focus(handle.id);
      }
      syncTopAppTab();
    });
    ['window:minimized','window:restored','window:focused'].forEach((name) => eventBus.on(name, syncTopAppTab));
    syncTopAppTab();
    const shellWindow = document.querySelector('.shell-window');
    await waitForOpeningToSettle(shellWindow);
    const baselineWindow = box(shellWindow);

    initBusinessChat({
      session:{ authenticated:true, user:{ id:owner, name:'Composition User' } },
      commandBus:{ dispatch:async () => ({ command_id:'cmd_test', task_id:'task_test', status:'queued' }) },
      db:makeDb(chat),
      getActiveModule:() => ({ id:'threads', name:'Threads' }),
    });

    window.shellHarness = {
      ready:false,
      get windowClicks(){ return windowClicks; },
      get layoutEvents(){ return layoutEvents; },
      workArea(){ const vp = wm.getViewport(); return { originLeft: vp.originLeft, originTop: vp.originTop, left: vp.left, top: vp.top, width: Math.max(0, vp.w - vp.left - vp.right), height: Math.max(0, vp.h - vp.top - vp.bottom) }; },
      setSize(width, height){ const el = document.querySelector('.shell-window'); el.style.width = width + 'px'; el.style.height = height + 'px'; },
      collect,
      addInactiveChatClones(){ const active=document.querySelector('.ctox-chat-window.is-active'); if(!active) return; ['left','right'].forEach((rel,index) => { const clone=active.cloneNode(true); clone.classList.remove('is-active'); clone.dataset.chatId='clone_'+rel; clone.dataset.chatRel=rel; clone.style.left=(700+index*400)+'px'; active.parentElement.appendChild(clone); }); },
      collectChatWindows(){ return [...document.querySelectorAll('.ctox-chat-window')].filter((node) => { const style=getComputedStyle(node); const rect=node.getBoundingClientRect(); return style.display!=='none' && rect.width>0 && rect.height>0; }).map((node) => ({...box(node),active:node.classList.contains('is-active')})); },
    };

    waitFor(() => document.body.hasAttribute('data-shell-chat-dock-expanded')).then(() => { window.shellHarness.ready = true; });

    function collect() {
      const windowRect=box(document.querySelector('.shell-window'));
      const chatRect=box(document.querySelector('[data-ctox-chat-root]'));
      const dockRect=box(document.querySelector('[data-chat-dock]'));
      return {
        viewport:{ width:innerWidth, height:innerHeight }, baselineWindow,
        window:windowRect, chat:chatRect, dock:dockRect,
        bottomAppSwitcherPresent:Boolean(document.querySelector('[data-shell-taskbar], .shell-taskbar')),
        chatExpanded:document.body.hasAttribute('data-shell-chat-dock-expanded'),
        chatSide:document.body.hasAttribute('data-shell-chat-dock-side'),
        chatCompact:document.body.hasAttribute('data-shell-chat-dock-compact'),
        snapZone:document.querySelector('.shell-window')?.dataset.snapZone || null,
        snapPreviewVisible:(() => { const preview=document.querySelector('[data-snap-preview]'); return Boolean(preview && !preview.hidden && getComputedStyle(preview).display!=='none'); })(),
        windowState:wm.describe(handle.id)?.state,
        overlap:{
          windowChat:intersection(windowRect, chatRect),
          windowDock:intersection(windowRect, dockRect),
        },
      };
    }

    function makeDb(initial) {
      let value=structuredClone(initial);
      const doc=() => ({ toJSON:() => structuredClone(value), incrementalPatch:async (next) => { value={...value,...structuredClone(next)}; } });
      return { raw:{
        business_chats:{ $:{ subscribe:() => ({ unsubscribe(){} }) }, find:() => ({ exec:async () => [doc()] }), findOne:() => ({ exec:async () => doc() }), insert:async (next) => { value=structuredClone(next); return doc(); } },
        business_commands:{ $:{ subscribe:() => ({ unsubscribe(){} }) } },
        ctox_queue_tasks:{ $:{ subscribe:() => ({ unsubscribe(){} }) } },
      } };
    }
    function box(node){ const r=node?.getBoundingClientRect?.(); return r ? { x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom } : { x:0,y:0,width:0,height:0,right:0,bottom:0 }; }
    function intersection(a,b){ return Math.max(0,Math.min(a.right,b.right)-Math.max(a.x,b.x))*Math.max(0,Math.min(a.bottom,b.bottom)-Math.max(a.y,b.y)); }
    function localDateString(date){ return date.getFullYear()+'-'+String(date.getMonth()+1).padStart(2,'0')+'-'+String(date.getDate()).padStart(2,'0'); }
    async function waitForOpeningToSettle(node){
      if(!node?.classList.contains('is-opening')) return;
      await new Promise((resolve) => {
        let poll;
        let timeout;
        const finish=()=>{ clearInterval(poll); clearTimeout(timeout); node.removeEventListener('animationend',onAnimationEnd); resolve(); };
        const onAnimationEnd=(event)=>{ if(event.target===node) finish(); };
        node.addEventListener('animationend',onAnimationEnd);
        poll=setInterval(()=>{ if(!node.classList.contains('is-opening')) finish(); },16);
        timeout=setTimeout(finish,500);
      });
      await new Promise((resolve)=>requestAnimationFrame(()=>resolve()));
    }
    async function waitFor(predicate){ const end=Date.now()+4000; while(Date.now()<end){ if(predicate()) return; await new Promise((resolve)=>setTimeout(resolve,16)); } throw new Error('composition timeout'); }
  </script>
</body>
</html>`;
}
