#!/usr/bin/env node

import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');
const outputDir = process.env.BUSINESS_OS_INTERACTIVE_OUTPUT_DIR
  || path.join(repoRoot, 'output/playwright', `business-os-interactive-window-${timestampForPath()}`);
const reportPath = path.join(outputDir, 'business-os-interactive-window-qa.json');
const targetUrl = process.env.BUSINESS_OS_INTERACTIVE_URL || 'http://127.0.0.1:8765/?rxdbSmoke=1';
const headless = process.env.BUSINESS_OS_INTERACTIVE_HEADLESS === '1';
const readyTimeoutMs = positiveInteger(process.env.BUSINESS_OS_INTERACTIVE_READY_TIMEOUT_MS || '90000');
const localAssetPrefixes = String(process.env.BUSINESS_OS_INTERACTIVE_LOCAL_ASSET_PREFIXES || '')
  .split(',')
  .map((value) => value.trim().replace(/^business-os\//, ''))
  .filter(Boolean);
const appIds = String(process.env.BUSINESS_OS_INTERACTIVE_APP_IDS || '')
  .split(',')
  .map((value) => value.trim())
  .filter(Boolean);

fs.mkdirSync(outputDir, { recursive: true });

const inventory = loadBusinessOsAppInventory();
const windowApps = inventory.allApps.filter((app) => app.kind !== 'shell-surface');
const selectedApps = appIds.length
  ? windowApps.filter((app) => appIds.includes(app.id))
  : windowApps;
const unknownIds = appIds.filter((id) => !windowApps.some((app) => app.id === id));
if (unknownIds.length) throw new Error(`Unknown interactive QA app id(s): ${unknownIds.join(', ')}`);

const { chromium } = require(resolvePlaywrightModule());
const report = {
  ok: false,
  startedAt: new Date().toISOString(),
  endedAt: null,
  url: targetUrl,
  headless,
  localAssetPrefixes,
  expectedApps: selectedApps.map((app) => app.id),
  shell: null,
  desktopLabels: null,
  mobileDesktopLabels: null,
  apps: [],
  compactShell: null,
  mobileShell: null,
  mobileApp: null,
  mobilePaneAccess: null,
  mobileApps: [],
  consoleEvents: [],
  failures: [],
  bootstrapDiagnostics: null,
};

const browser = await chromium.launch({
  headless,
  executablePath: resolveChromiumExecutable(chromium),
  // Chrome's Local Network Access checks can reject the loopback WebSocket
  // even though the page itself is served by the local CTOX installation.
  // This QA runner intentionally exercises that local browser data plane.
  args: ['--disable-features=LocalNetworkAccessChecks,LocalNetworkAccessForNavigations'],
});

let activePage = null;
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  activePage = page;
  attachDiagnostics(page);
  if (localAssetPrefixes.length) await attachLocalAssetRouting(page);
  await page.goto(targetUrl, { waitUntil: 'domcontentloaded', timeout: readyTimeoutMs });
  await waitForShell(page);
  await closeAllWindows(page);
  await page.waitForTimeout(250);

  report.shell = await collectShellGeometry(page);
  assertShellGeometry(report.shell, 'initial shell');
  report.desktopLabels = await inspectDesktopLabels(page);
  for (const message of report.desktopLabels.failures) {
    report.failures.push({ scope: 'desktop labels', message });
  }
  await page.screenshot({ path: path.join(outputDir, '00-shell-initial.png'), scale: 'css' });

  let ordinal = 0;
  for (const app of selectedApps) {
    ordinal += 1;
    const result = await exerciseApp(page, app, ordinal);
    report.apps.push(result);
    if (!result.ok) {
      for (const failure of result.failures) {
        report.failures.push({ scope: app.id, message: failure });
      }
    }
    console.log(`${result.ok ? 'OK' : 'FAIL'} ${ordinal}/${selectedApps.length} ${app.id}`);
  }

  await closeAllWindows(page);
  await page.setViewportSize({ width: 900, height: 620 });
  await page.waitForTimeout(300);
  report.compactShell = await collectShellGeometry(page);
  assertShellGeometry(report.compactShell, 'compact shell');
  await page.screenshot({ path: path.join(outputDir, '99-shell-compact.png'), scale: 'css' });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(300);
  report.mobileShell = await collectShellGeometry(page);
  assertShellGeometry(report.mobileShell, 'mobile shell');
  report.mobileDesktopLabels = await inspectDesktopLabels(page);
  for (const message of report.mobileDesktopLabels.failures) {
    report.failures.push({ scope: 'mobile desktop labels', message });
  }
  await page.screenshot({ path: path.join(outputDir, '100-shell-mobile.png'), scale: 'css' });
  let mobileOrdinal = 0;
  for (const mobileTarget of selectedApps) {
    mobileOrdinal += 1;
    const mobileResult = await exerciseMobileApp(page, mobileTarget, mobileOrdinal);
    report.mobileApps.push(mobileResult);
    if (mobileOrdinal === 1) {
      report.mobileApp = mobileResult.geometry || null;
      report.mobilePaneAccess = mobileResult.paneAccess || null;
    }
    for (const message of mobileResult.failures) {
      report.failures.push({ scope: `mobile ${mobileTarget.id}`, message });
    }
    console.log(`${mobileResult.ok ? 'MOBILE OK' : 'MOBILE FAIL'} ${mobileOrdinal}/${selectedApps.length} ${mobileTarget.id}`);
  }

  const fatalConsole = report.consoleEvents.filter((event) => (
    ['error', 'pageerror', 'requestfailed'].includes(event.type)
  ));
  for (const event of fatalConsole) {
    report.failures.push({ scope: 'browser', message: `${event.type}: ${event.text}` });
  }
} catch (error) {
  if (activePage) {
    report.bootstrapDiagnostics = await activePage.evaluate(() => {
      const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
      return {
        url: location.href,
        title: document.title,
        authState: document.body?.dataset?.authState || '',
        moduleLoading: document.body?.dataset?.moduleLoading || '',
        moduleCount: Array.isArray(state?.modules) ? state.modules.length : null,
        hasWindowManager: Boolean(state?.windowManager),
        smokeType: typeof window.ctoxBusinessOsSmoke,
        appType: typeof window.CTOX_BUSINESS_OS_APP,
        matchingGlobalKeys: Object.keys(window).filter((key) => /ctox.*business/i.test(key)).slice(0, 20),
        moduleScripts: [...document.querySelectorAll('script[type="module"][src]')].map((script) => script.src),
        bodyText: document.body?.innerText?.slice(0, 500) || '',
      };
    }).catch((diagnosticError) => ({ error: diagnosticError?.message || String(diagnosticError) }));
    await activePage.screenshot({ path: path.join(outputDir, 'bootstrap-failure.png'), scale: 'css' }).catch(() => {});
  }
  report.failures.push({ scope: 'harness', message: error?.stack || String(error) });
} finally {
  report.endedAt = new Date().toISOString();
  report.ok = report.failures.length === 0
    && report.apps.length === selectedApps.length
    && report.apps.every((app) => app.ok)
    && report.mobileApps.length === selectedApps.length
    && report.mobileApps.every((app) => app.ok);
  fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  await browser.close().catch(() => {});
}

if (!report.ok) {
  console.error(`Interactive Business OS window QA failed with ${report.failures.length} issue(s).`);
  console.error(`Report: ${reportPath}`);
  process.exitCode = 1;
} else {
  console.log(`Interactive Business OS window QA OK: ${report.apps.length} apps.`);
  console.log(`Report: ${reportPath}`);
}

async function exerciseApp(page, app, ordinal) {
  const failures = [];
  const startedAt = Date.now();
  let launchSurface = 'start-menu';
  let windowLocator = null;
  let launchState = null;

  try {
    launchSurface = await launchFromVisibleShell(page, app);
    await page.waitForTimeout(150);
    launchState = await page.evaluate((id) => {
      const shell = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
      const module = shell?.modules?.find?.((item) => item.id === id);
      return {
        hash: location.hash,
        activeModuleId: shell?.activeModule?.id || '',
        windowOwners: shell?.windowManager?.listWindows?.().map((item) => item.ownerId) || [],
        module: module ? {
          launch_kind: module.launch_kind || '',
          presentation: module.presentation || null,
          shell: module.layout?.shell || '',
        } : null,
      };
    }, app.id);
    windowLocator = page.locator(`.shell-window[data-owner-id="desktop-app:${cssEscape(app.id)}"]`);
    try {
      await windowLocator.waitFor({ state: 'visible', timeout: 5000 });
    } catch {
      // A one-shot RxDB/build reload can race a launcher click. Once the shell
      // is stable again, retry the same visible user action exactly once.
      await waitForShell(page);
      if (!await windowLocator.count()) {
        launchSurface = `${await launchFromVisibleShell(page, app)}-retry`;
      }
      await windowLocator.waitFor({ state: 'visible', timeout: 30000 });
    }
    try {
      await waitForAppContent(page, windowLocator, app.id);
    } catch (firstContentError) {
      // A one-shot RxDB/build hand-off can leave an already-created window
      // without its mounted module root. Retry the same visible launch once;
      // a second empty mount remains a hard failure.
      await closeWindowByOwner(page, app.id);
      await page.waitForTimeout(250);
      await waitForShell(page);
      launchSurface = `${await launchFromVisibleShell(page, app)}-content-retry`;
      windowLocator = page.locator(`.shell-window[data-owner-id="desktop-app:${cssEscape(app.id)}"]`);
      await windowLocator.waitFor({ state: 'visible', timeout: 30000 });
      await waitForAppContent(page, windowLocator, app.id).catch((secondContentError) => {
        secondContentError.cause = firstContentError;
        throw secondContentError;
      });
    }

    const initial = await collectWindowGeometry(page, app.id);
    assertWindowInsideViewport(initial, failures, 'initial');
    if (!initial.contentMeaningful) failures.push('initial content is visually empty');
    if (initial.shellGrid?.emptyPanes && initial.shellGrid.mainWidth < initial.shellGrid.rootWidth - 2) {
      failures.push(`empty shell panes leave an unused grid track: ${initial.shellGrid.mainWidth}/${initial.shellGrid.rootWidth}px`);
    }
    if (app.cohort !== 'compatibility') {
      if (!initial.header.version) failures.push('window header has no visible version');
      if (!initial.header.status) failures.push('window header has no visible lifecycle status');
      if (!initial.header.actions.includes('versions')) failures.push('window header has no version-control action');
    }

    const headerActions = ordinal === 1 && app.cohort !== 'compatibility'
      ? await exerciseWindowHeaderActions(page, windowLocator)
      : null;
    const dragAndSnap = await exerciseWindowDragAndSnap(page, windowLocator);

    let paneResize = await exerciseFirstPaneResizer(page, windowLocator);
    if (paneResize && Math.abs(paneResize.after - paneResize.before) < 20) {
      failures.push(`pane resizer did not change ${paneResize.cssVar}: ${paneResize.before} -> ${paneResize.after}`);
    }

    const southShrink = await dragResize(page, windowLocator, 's', { x: 0, y: -120 });
    if (southShrink.before.height - southShrink.after.height < 70 && southShrink.before.height > southShrink.minHeight + 70) {
      failures.push(`south resize did not shrink: ${southShrink.before.height} -> ${southShrink.after.height}`);
    }
    if (southShrink.after.height < southShrink.minHeight - 2) {
      failures.push(`south resize crossed min-height ${southShrink.minHeight}: ${southShrink.after.height}`);
    }

    const southGrow = await dragResize(page, windowLocator, 's', { x: 0, y: 160 });
    if (southGrow.after.height - southGrow.before.height < 70) {
      failures.push(`south resize did not grow: ${southGrow.before.height} -> ${southGrow.after.height}`);
    }

    const cornerShrink = await dragResize(page, windowLocator, 'se', { x: -110, y: -80 });
    if (cornerShrink.before.width - cornerShrink.after.width < 60 && cornerShrink.before.width > cornerShrink.minWidth + 60) {
      failures.push(`corner resize did not shrink width: ${cornerShrink.before.width} -> ${cornerShrink.after.width}`);
    }

    const cornerGrow = await dragResize(page, windowLocator, 'se', { x: 140, y: 100 });
    if (cornerGrow.after.width - cornerGrow.before.width < 60) {
      failures.push(`corner resize did not grow width: ${cornerGrow.before.width} -> ${cornerGrow.after.width}`);
    }

    const beforeMaximize = await collectWindowGeometry(page, app.id);
    await windowLocator.locator('.shell-window-control--maximize').click();
    await page.waitForFunction((id) => {
      const element = document.querySelector(`.shell-window[data-owner-id="desktop-app:${CSS.escape(id)}"]`);
      return element?.classList.contains('is-maximized')
        && element.querySelector('.shell-window-control--maximize')?.getAttribute('aria-label') === 'Wiederherstellen';
    }, app.id, { timeout: 3000 });
    const maximized = await collectWindowGeometry(page, app.id);
    assertWindowInsideViewport(maximized, failures, 'maximized');
    const maximizedControl = await windowLocator.locator('.shell-window-control--maximize').evaluate((button) => ({
      label: button.getAttribute('aria-label'),
      glyph: button.textContent,
      windowClass: button.closest('.shell-window')?.className || '',
    }));
    if (maximizedControl.label !== 'Wiederherstellen' || maximizedControl.glyph !== '❐'
      || !maximizedControl.windowClass.split(/\s+/).includes('is-maximized')) {
      failures.push(`maximize control did not expose restore state: ${JSON.stringify(maximizedControl)}`);
    }
    if (!paneResize) {
      paneResize = await exerciseFirstPaneResizer(page, windowLocator);
      if (paneResize && Math.abs(paneResize.after - paneResize.before) < 20) {
        failures.push(`pane resizer did not change ${paneResize.cssVar} while maximized: ${paneResize.before} -> ${paneResize.after}`);
      }
    }

    await windowLocator.locator('.shell-window-control--maximize').click();
    await page.waitForFunction((id) => {
      const element = document.querySelector(`.shell-window[data-owner-id="desktop-app:${CSS.escape(id)}"]`);
      return element && !element.classList.contains('is-maximized')
        && element.querySelector('.shell-window-control--maximize')?.getAttribute('aria-label') === 'Maximieren';
    }, app.id, { timeout: 3000 });
    const restored = await collectWindowGeometry(page, app.id);
    const restoredControl = await windowLocator.locator('.shell-window-control--maximize').evaluate((button) => ({
      label: button.getAttribute('aria-label'),
      glyph: button.textContent,
      windowClass: button.closest('.shell-window')?.className || '',
    }));
    if (restoredControl.label !== 'Maximieren' || restoredControl.glyph !== '□'
      || restoredControl.windowClass.split(/\s+/).includes('is-maximized')) {
      failures.push(`restore control did not expose maximize state: ${JSON.stringify(restoredControl)}`);
    }
    if (Math.abs(restored.rect.width - beforeMaximize.rect.width) > 3
      || Math.abs(restored.rect.height - beforeMaximize.rect.height) > 3) {
      failures.push(`restore changed geometry unexpectedly: ${JSON.stringify({ before: beforeMaximize.rect, after: restored.rect })}`);
    }

    const shellWithWindow = await collectShellGeometry(page, app.id);
    assertNoInteractiveOverlap(shellWithWindow, failures);
    const screenshot = `${String(ordinal).padStart(2, '0')}-${slug(app.id)}.png`;
    await page.screenshot({ path: path.join(outputDir, screenshot), scale: 'css' });

    await windowLocator.locator('.shell-window-control--minimize').click();
    await windowLocator.waitFor({ state: 'hidden', timeout: 3000 });
    const topAppTab = page.locator(`.module-tab[data-target="${cssEscape(app.id)}"]`).first();
    await topAppTab.waitFor({ state: 'visible', timeout: 3000 });
    await topAppTab.click();
    await windowLocator.waitFor({ state: 'visible', timeout: 3000 });

    await windowLocator.locator('.shell-window-control--close').click();
    await windowLocator.waitFor({ state: 'detached', timeout: 5000 });

    return {
      id: app.id,
      title: app.title,
      ok: failures.length === 0,
      failures,
      launchSurface,
      launchState,
      durationMs: Date.now() - startedAt,
      initial,
      dragAndSnap,
      paneResize,
      headerActions,
      southShrink,
      southGrow,
      cornerShrink,
      cornerGrow,
      maximized,
      restored,
      shellWithWindow,
      screenshot,
    };
  } catch (error) {
    failures.push(error?.stack || String(error));
    if (windowLocator) {
      await windowLocator.locator('.shell-window-control--close').click({ timeout: 1000 }).catch(() => {});
    }
    await closeWindowByOwner(page, app.id).catch(() => {});
    return {
      id: app.id,
      title: app.title,
      ok: false,
      failures,
      launchSurface,
      launchState,
      durationMs: Date.now() - startedAt,
    };
  }
}

async function exerciseMobileApp(page, app, ordinal) {
  const failures = [];
  let mobileWindow = null;
  try {
    await launchFromVisibleShell(page, app);
    mobileWindow = page.locator(`.shell-window[data-owner-id="desktop-app:${cssEscape(app.id)}"]`);
    await mobileWindow.waitFor({ state: 'visible', timeout: 30000 });
    await waitForAppContent(page, mobileWindow, app.id);

    const geometry = await collectWindowGeometry(page, app.id);
    assertWindowInsideViewport(geometry, failures, 'mobile app');
    const mobileSheet = await mobileWindow.evaluate((element) => element.classList.contains('is-mobile-sheet'));
    if (!mobileSheet) failures.push('window did not switch to mobile-sheet presentation');
    const headerOverflow = await mobileWindow.locator('[data-window-header]').evaluate((header) => (
      header.scrollWidth > header.clientWidth + 2
    ));
    if (headerOverflow) failures.push('window header overflows horizontally');
    if (geometry.horizontalOverflow) failures.push('app content has horizontal root overflow');

    const screenshot = `${String(100 + ordinal).padStart(3, '0')}-mobile-${slug(app.id)}.png`;
    await page.screenshot({ path: path.join(outputDir, screenshot), scale: 'css' });
    const paneAccess = await inspectMobilePaneAccess(mobileWindow);
    if (paneAccess && !paneAccess.lastPaneReachable) {
      failures.push('last responsive pane has no reachable mobile scroll/stack path');
    }
    let lastPaneScreenshot = null;
    if (paneAccess?.paneCount > 1) {
      lastPaneScreenshot = `${String(200 + ordinal).padStart(3, '0')}-mobile-${slug(app.id)}-last-pane.png`;
      await page.screenshot({ path: path.join(outputDir, lastPaneScreenshot), scale: 'css' });
    }
    const shell = await collectShellGeometry(page, app.id);
    assertNoInteractiveOverlap(shell, failures);

    await mobileWindow.locator('.shell-window-control--close').click();
    await mobileWindow.waitFor({ state: 'detached', timeout: 5000 });
    return {
      id: app.id,
      title: app.title,
      ok: failures.length === 0,
      failures,
      geometry,
      mobileSheet,
      headerOverflow,
      paneAccess,
      shell,
      screenshot,
      lastPaneScreenshot,
    };
  } catch (error) {
    failures.push(error?.stack || String(error));
    if (mobileWindow) {
      await mobileWindow.locator('.shell-window-control--close').click({ timeout: 1000 }).catch(() => {});
    }
    await closeWindowByOwner(page, app.id).catch(() => {});
    return {
      id: app.id,
      title: app.title,
      ok: false,
      failures,
    };
  }
}

async function launchFromVisibleShell(page, app) {
  const startButton = page.locator('[data-shell-start]');
  await startButton.click();
  const panel = page.locator('.shell-start-menu-panel');
  await panel.waitFor({ state: 'visible', timeout: 3000 });
  const search = panel.locator('.start-menu-search-input');
  await search.fill(app.title || app.id);
  await page.waitForTimeout(100);
  const exact = panel.locator(`.start-menu-item[data-target="${cssEscape(app.id)}"]:visible`).first();
  if (await exact.count()) {
    await exact.click();
    return 'start-menu';
  }

  await search.fill('');
  await startButton.click().catch(() => {});
  const desktopIcon = page.locator(`.desktop-icon[data-target="${cssEscape(app.id)}"]`).first();
  if (await desktopIcon.count()) {
    await desktopIcon.click();
    return 'desktop-icon';
  }
  throw new Error(`No visible launcher found for ${app.id} (${app.title})`);
}

async function waitForAppContent(page, windowLocator, appId) {
  await windowLocator.locator('[data-loading-shadow]').waitFor({ state: 'detached', timeout: 30000 }).catch(() => {});
  await page.waitForFunction((id) => {
    const win = document.querySelector(`.shell-window[data-owner-id="desktop-app:${CSS.escape(id)}"]`);
    const content = win?.querySelector('[data-window-content]');
    if (!content) return false;
    const moduleRoot = content.querySelector('.shell-window-module-root[data-module-root]');
    if (moduleRoot && moduleRoot.dataset.moduleReady !== 'true') return false;
    // Decorative/skeleton SVGs must not make an app look ready. Wait for
    // operable content, an explicit module root, or meaningful text instead.
    const meaningful = content.querySelectorAll('button,input,select,textarea,table,canvas,iframe').length;
    return meaningful > 0 || (content.textContent || '').trim().length > 12;
  }, appId, { timeout: 30000 });
}

async function dragResize(page, windowLocator, direction, delta) {
  const before = await windowLocator.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const win = globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.listWindows?.()
      .find((entry) => entry.id === element.id);
    return {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      minWidth: Number(win?.minWidth || 320),
      minHeight: Number(win?.minHeight || 200),
    };
  });
  const handle = windowLocator.locator(`[data-window-resize="${direction}"]`);
  const box = await handle.boundingBox();
  if (!box) throw new Error(`Resize handle ${direction} has no visible box`);
  const start = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + delta.x, start.y + delta.y, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(80);
  const after = await windowLocator.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  });
  return { direction, before, after, minWidth: before.minWidth, minHeight: before.minHeight };
}

async function exerciseWindowDragAndSnap(page, windowLocator) {
  const before = await readWindowDragState(windowLocator);
  const freeTarget = {
    x: Math.min(before.viewport.width - 180, Math.max(180, before.rect.x + 180)),
    y: Math.min(before.viewport.height - 160, Math.max(before.workspaceTop + 150, before.rect.y + 100)),
  };
  await dragWindowHeader(page, windowLocator, freeTarget);
  const free = await readWindowDragState(windowLocator);
  if (Math.abs(free.rect.x - before.rect.x) < 30 && Math.abs(free.rect.y - before.rect.y) < 30) {
    throw new Error(`Window title drag did not move freely: ${JSON.stringify({ before: before.rect, after: free.rect })}`);
  }
  if (free.snapZone) throw new Error(`Free window drag unexpectedly snapped to ${free.snapZone}`);

  await dragWindowHeader(page, windowLocator, { x: 8, y: Math.max(free.workspaceTop + 140, 240) });
  const left = await readWindowDragState(windowLocator);
  if (left.snapZone !== 'left') throw new Error(`Left snap failed: ${JSON.stringify(left)}`);

  await restoreSnappedWindowToCenter(page, windowLocator, left);
  const afterLeftRestore = await readWindowDragState(windowLocator);
  if (afterLeftRestore.snapZone) throw new Error(`Window stayed snapped after dragging away from left: ${afterLeftRestore.snapZone}`);

  await dragWindowHeader(page, windowLocator, {
    x: afterLeftRestore.viewport.width - 8,
    y: Math.max(afterLeftRestore.workspaceTop + 140, 240),
  });
  const right = await readWindowDragState(windowLocator);
  if (right.snapZone !== 'right') throw new Error(`Right snap failed: ${JSON.stringify(right)}`);

  await restoreSnappedWindowToCenter(page, windowLocator, right);
  const afterRightRestore = await readWindowDragState(windowLocator);
  if (afterRightRestore.snapZone) throw new Error(`Window stayed snapped after dragging away from right: ${afterRightRestore.snapZone}`);

  await dragWindowHeader(page, windowLocator, {
    x: Math.round(afterRightRestore.viewport.width / 2),
    y: afterRightRestore.workspaceTop + 5,
  });
  const top = await readWindowDragState(windowLocator);
  if (top.snapZone !== 'top') throw new Error(`Top snap failed: ${JSON.stringify(top)}`);

  await restoreSnappedWindowToCenter(page, windowLocator, top);
  const restored = await readWindowDragState(windowLocator);
  if (restored.snapZone) throw new Error(`Window stayed snapped after dragging away from top: ${restored.snapZone}`);
  if (restored.rect.x < -2 || restored.rect.right > restored.viewport.width + 2
    || restored.rect.y < restored.workspaceTop - 2 || restored.rect.bottom > restored.viewport.height + 2) {
    throw new Error(`Window restore after snap is outside the viewport: ${JSON.stringify(restored)}`);
  }
  return { before, free, left, right, top, restored };
}

async function restoreSnappedWindowToCenter(page, windowLocator, state) {
  await dragWindowHeader(page, windowLocator, {
    x: Math.max(180, Math.min(320, Math.round(state.viewport.width * 0.25))),
    y: state.workspaceTop + 100,
  });
}

async function dragWindowHeader(page, windowLocator, target) {
  const header = windowLocator.locator('[data-window-header]');
  const title = windowLocator.locator('[data-window-title]');
  const titleBox = await title.boundingBox();
  const headerBox = await header.boundingBox();
  const box = titleBox || headerBox;
  if (!box) throw new Error('Window title/header has no visible drag box');
  const start = {
    x: box.x + Math.min(box.width / 2, 72),
    y: box.y + box.height / 2,
  };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(target.x, target.y, { steps: 10 });
  await page.mouse.up();
  await page.waitForTimeout(90);
}

async function readWindowDragState(windowLocator) {
  return windowLocator.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const workspace = document.querySelector('.workspace-frame')?.getBoundingClientRect();
    return {
      rect: {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
        right: Math.round(rect.right),
        bottom: Math.round(rect.bottom),
      },
      viewport: { width: innerWidth, height: innerHeight },
      workspaceTop: Math.round(workspace?.top || 0),
      snapZone: element.dataset.snapZone || '',
      snapped: element.classList.contains('is-snapped'),
    };
  });
}

async function collectWindowGeometry(page, appId) {
  return page.evaluate((id) => {
    const toRect = (value) => ({
      x: Math.round(value.x),
      y: Math.round(value.y),
      width: Math.round(value.width),
      height: Math.round(value.height),
      right: Math.round(value.right),
      bottom: Math.round(value.bottom),
    });
    const element = document.querySelector(`.shell-window[data-owner-id="desktop-app:${CSS.escape(id)}"]`);
    if (!element) throw new Error(`Window not found for ${id}`);
    const rect = toRect(element.getBoundingClientRect());
    const content = element.querySelector('[data-window-content]');
    const win = globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.listWindows?.()
      .find((entry) => entry.id === element.id);
    const meaningfulElements = content?.querySelectorAll('button,input,select,textarea,table,canvas,iframe,svg,[data-module-root],[data-desktop-app]').length || 0;
    const moduleRoot = content?.querySelector(':scope > .shell-window-module-root');
    const moduleMain = moduleRoot?.querySelector(':scope > .module-content');
    const modulePanes = moduleRoot ? [...moduleRoot.querySelectorAll(':scope > .shell-window-module-pane')] : [];
    const emptyPanes = modulePanes.length > 0 && modulePanes.every((pane) => (
      !pane.children.length && !pane.textContent.trim()
    ));
    return {
      rect,
      viewport: { width: innerWidth, height: innerHeight },
      state: win?.state || '',
      minWidth: Number(win?.minWidth || 320),
      minHeight: Number(win?.minHeight || 200),
      contentTextLength: (content?.textContent || '').trim().length,
      meaningfulElements,
      contentMeaningful: meaningfulElements > 0 || (content?.textContent || '').trim().length > 12,
      horizontalOverflow: Boolean(content && content.scrollWidth > content.clientWidth + 2),
      shellGrid: moduleRoot && moduleMain ? {
        emptyPanes,
        rootWidth: Math.round(moduleRoot.getBoundingClientRect().width),
        mainWidth: Math.round(moduleMain.getBoundingClientRect().width),
      } : null,
      header: {
        version: element.querySelector('.shell-window-header-meta[data-state="version"]')?.textContent?.trim() || '',
        status: element.querySelector('.shell-window-header-meta:not([data-state="version"]) .shell-window-header-item-label')?.textContent?.trim() || '',
        actions: [...element.querySelectorAll('[data-window-header-action]')]
          .map((node) => node.dataset.windowHeaderAction)
          .filter(Boolean),
      },
    };
  }, appId);
}

async function exerciseFirstPaneResizer(page, windowLocator) {
  const handle = windowLocator.locator('.ctox-column-resizer[data-resizer-var]:visible').first();
  if (!await handle.count()) return null;
  const details = await handle.evaluate((node) => {
    const frame = node.closest('[data-resize-frame]');
    const cssVar = node.dataset.resizerVar;
    const side = node.dataset.resizer === 'right' ? 'right' : 'left';
    const fromCss = Number.parseFloat(getComputedStyle(frame).getPropertyValue(cssVar));
    const pane = side === 'right' ? node.nextElementSibling : node.previousElementSibling;
    const before = Number.isFinite(fromCss) ? fromCss : pane?.getBoundingClientRect?.().width;
    return { cssVar, side, before };
  });
  const box = await handle.boundingBox();
  if (!box || !Number.isFinite(details.before)) return null;
  const delta = details.side === 'right' ? -48 : 48;
  await page.mouse.move(box.x + box.width / 2, box.y + Math.min(40, box.height / 2));
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + delta, box.y + Math.min(40, box.height / 2), { steps: 6 });
  await page.waitForTimeout(50);
  await page.mouse.up();
  await page.waitForTimeout(80);
  const after = await handle.evaluate((node, cssVar) => Number.parseFloat(
    getComputedStyle(node.closest('[data-resize-frame]')).getPropertyValue(cssVar),
  ), details.cssVar);
  return { ...details, after };
}

async function exerciseWindowHeaderActions(page, windowLocator) {
  const result = {
    lifecycle: false,
    versionsSection: false,
    sourceEditor: false,
    sameWindow: false,
    appCatalogHidden: false,
    appRestored: false,
    contextMenu: false,
    context: null,
  };
  const originalWindowId = await windowLocator.getAttribute('id');
  const windowCountBefore = await page.locator('.shell-window').count();
  const lifecycle = windowLocator.locator('[data-window-header-action="versions"]');
  await lifecycle.click();
  const tools = windowLocator.locator('[data-module-integrated-tools]');
  await tools.waitFor({ state: 'visible', timeout: 5000 });
  const versions = tools.locator('[data-integrated-versions]');
  await versions.waitFor({ state: 'visible', timeout: 5000 });
  result.lifecycle = true;
  const releaseSection = versions.locator('[data-integrated-release]');
  await releaseSection.waitFor({ state: 'visible', timeout: 5000 });
  result.versionsSection = true;
  result.sameWindow = await windowLocator.getAttribute('id') === originalWindowId
    && await page.locator('.shell-window').count() === windowCountBefore;
  if (!result.sameWindow) throw new Error('Versions action left the running app window');
  await tools.locator('[data-integrated-view="app"]').click();
  await windowLocator.locator(':scope > [data-window-content] > [data-module-root]').waitFor({ state: 'visible', timeout: 3000 });

  const source = windowLocator.locator('[data-window-header-action="source"]');
  if (await source.count()) {
    await source.click();
    const editor = windowLocator.locator('.source-editor');
    await editor.waitFor({ state: 'visible', timeout: 10000 });
    result.sourceEditor = true;
    result.appCatalogHidden = await editor.locator('.source-editor-module').count() === 0;
    if (!result.appCatalogHidden) throw new Error('Integrated source editor exposed unrelated apps');
    if (await page.locator('.shell-window[data-owner-id="desktop-app:code-editor"]').count()) {
      throw new Error('Source action opened the retired standalone Source Editor window');
    }
    await tools.locator('[data-integrated-view="app"]').click();
    result.appRestored = await windowLocator.locator(':scope > [data-window-content] > [data-module-root]').isVisible();
    if (!result.appRestored) throw new Error('App view did not restore after Source');
  }

  const contextTarget = await windowLocator.locator('[data-context-record-id]:visible').count()
    ? windowLocator.locator('[data-context-record-id]:visible').first()
    : windowLocator.locator('[data-module-root]:visible').first();
  const contextBox = await contextTarget.boundingBox();
  if (!contextBox) throw new Error('No visible module target for the global context menu');
  const pointer = {
    x: Math.round(contextBox.x + Math.min(contextBox.width / 2, 24)),
    y: Math.round(contextBox.y + Math.min(contextBox.height / 2, 24)),
  };
  result.context = await contextTarget.evaluate((target, point) => {
    const state = globalThis.ctoxBusinessOsSmoke?.state;
    const moduleId = target.closest('[data-module-root]')?.dataset?.moduleRoot || state?.activeModule?.id;
    const mod = state?.modules?.find?.((item) => item.id === moduleId) || state?.activeModule;
    return globalThis.ctoxBusinessOsSmoke?.extractGlobalCtoxContext?.(mod, target, {
      clientX: point.x,
      clientY: point.y,
    })?.context_v2 || null;
  }, pointer);
  await page.mouse.click(pointer.x, pointer.y, { button: 'right' });
  const globalMenu = page.locator('.ctox-global-context-menu:visible');
  await globalMenu.waitFor({ state: 'visible', timeout: 3000 });
  result.contextMenu = true;
  if (!result.context?.app_id || result.context.pointer?.x !== pointer.x || result.context.pointer?.y !== pointer.y) {
    throw new Error(`Global context menu lost its app or pointer context: ${JSON.stringify(result.context)}`);
  }
  if (!result.context.window_instance_id?.startsWith('desktop-app:')) {
    throw new Error(`Global context menu lost its window context: ${JSON.stringify(result.context)}`);
  }
  await page.keyboard.press('Escape');
  return result;
}

async function collectShellGeometry(page, activeAppId = '') {
  return page.evaluate((appId) => {
    const toRect = (value) => ({
      x: Math.round(value.x),
      y: Math.round(value.y),
      width: Math.round(value.width),
      height: Math.round(value.height),
      right: Math.round(value.right),
      bottom: Math.round(value.bottom),
    });
    const rect = (selector) => {
      const element = document.querySelector(selector);
      if (!element || getComputedStyle(element).display === 'none') return null;
      const value = element.getBoundingClientRect();
      if (!value.width || !value.height) return null;
      return toRect(value);
    };
    const visibleChatWindows = [...document.querySelectorAll('.ctox-chat-window')]
      .filter((element) => {
        const style = getComputedStyle(element);
        const value = element.getBoundingClientRect();
        return style.display !== 'none' && style.visibility !== 'hidden' && value.width > 0 && value.height > 0;
      })
      .map((element) => toRect(element.getBoundingClientRect()));
    const appWindow = appId
      ? rect(`.shell-window[data-owner-id="desktop-app:${CSS.escape(appId)}"]`)
      : null;
    return {
      viewport: { width: innerWidth, height: innerHeight },
      bodyAttributes: [...document.body.attributes]
        .filter((attribute) => attribute.name.startsWith('data-shell-chat'))
        .map((attribute) => [attribute.name, attribute.value]),
      appShell: rect('.app-shell'),
      topbar: rect('.topbar'),
      startButton: rect('[data-shell-start]'),
      topbarActions: rect('.topbar-actions'),
      workspace: rect('.workspace-frame'),
      windowLayer: rect('.shell-window-layer'),
      desktop: rect('[data-desktop-surface]') || rect('.desktop-surface'),
      bottomAppSwitcher: rect('[data-shell-taskbar], .shell-taskbar'),
      chatRoot: rect('.ctox-chat-root'),
      chatDock: rect('[data-chat-dock]'),
      chatWindows: visibleChatWindows,
      appWindow,
      documentOverflow: {
        x: document.documentElement.scrollWidth > document.documentElement.clientWidth + 2,
        y: document.documentElement.scrollHeight > document.documentElement.clientHeight + 2,
      },
    };
  }, activeAppId);
}

async function inspectDesktopLabels(page) {
  return page.evaluate(async () => {
    const failures = [];
    const icons = [...document.querySelectorAll('.desktop-icon')].filter((node) => {
      const rect = node.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    });
    for (const icon of icons) {
      const label = icon.querySelector('.desktop-icon-label');
      if (!label) continue;
      const iconRect = icon.getBoundingClientRect();
      const labelRect = label.getBoundingClientRect();
      const style = getComputedStyle(label);
      const lineHeight = Number.parseFloat(style.lineHeight) || 1;
      const lines = Math.round(labelRect.height / lineHeight);
      if (lines > 2) failures.push(`${label.textContent.trim()}: ${lines} visible lines`);
      if (labelRect.left < iconRect.left - 1 || labelRect.right > iconRect.right + 1 || labelRect.bottom > iconRect.bottom + 1) {
        failures.push(`${label.textContent.trim()}: label escapes icon cell`);
      }
      const surfaceRect = icon.closest('[data-desktop-surface], .desktop-surface')?.getBoundingClientRect();
      if (surfaceRect && (iconRect.left < surfaceRect.left - 1 || iconRect.right > surfaceRect.right + 1)) {
        failures.push(`${label.textContent.trim()}: icon escapes desktop horizontally`);
      }
    }
    for (let leftIndex = 0; leftIndex < icons.length; leftIndex += 1) {
      const left = icons[leftIndex].getBoundingClientRect();
      for (let rightIndex = leftIndex + 1; rightIndex < icons.length; rightIndex += 1) {
        const right = icons[rightIndex].getBoundingClientRect();
        const overlapWidth = Math.max(0, Math.min(left.right, right.right) - Math.max(left.left, right.left));
        const overlapHeight = Math.max(0, Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top));
        if (overlapWidth > 4 && overlapHeight > 4) {
          failures.push(`${icons[leftIndex].title || leftIndex} overlaps ${icons[rightIndex].title || rightIndex}`);
        }
      }
    }
    const probe = icons[0]?.querySelector('.desktop-icon-label');
    let synthetic = null;
    if (probe) {
      const previous = probe.textContent;
      probe.textContent = 'AutomatisierungsfreigabeEnterprise2026';
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const iconRect = probe.closest('.desktop-icon').getBoundingClientRect();
      const labelRect = probe.getBoundingClientRect();
      const lineHeight = Number.parseFloat(getComputedStyle(probe).lineHeight) || 1;
      synthetic = {
        lines: Math.round(labelRect.height / lineHeight),
        inside: labelRect.left >= iconRect.left - 1 && labelRect.right <= iconRect.right + 1 && labelRect.bottom <= iconRect.bottom + 1,
      };
      if (synthetic.lines > 2 || !synthetic.inside) failures.push('synthetic long app name breaks the desktop icon cell');
      probe.textContent = previous;
    }
    return { count: icons.length, synthetic, failures };
  });
}

async function inspectMobilePaneAccess(windowLocator) {
  return windowLocator.evaluate(async (windowElement) => {
    const frame = windowElement.querySelector('[data-resize-frame]');
    if (!frame) return null;
    const panes = [...frame.children].filter((node) => {
      if (node.matches('.ctox-column-resizer, [data-resizer]')) return false;
      const style = getComputedStyle(node);
      return style.display !== 'none' && node.getBoundingClientRect().width > 0;
    });
    const lastPane = panes.at(-1);
    if (!lastPane) return { paneCount: 0, lastPaneReachable: false };
    lastPane.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const paneRect = lastPane.getBoundingClientRect();
    const contentRect = windowElement.querySelector('[data-window-content]')?.getBoundingClientRect();
    const overlapHeight = contentRect
      ? Math.max(0, Math.min(paneRect.bottom, contentRect.bottom) - Math.max(paneRect.top, contentRect.top))
      : 0;
    return {
      paneCount: panes.length,
      lastPaneReachable: paneRect.width > 0 && paneRect.height > 0 && overlapHeight >= Math.min(44, paneRect.height),
      lastPaneRect: { x: paneRect.x, y: paneRect.y, width: paneRect.width, height: paneRect.height },
      contentRect: contentRect ? { x: contentRect.x, y: contentRect.y, width: contentRect.width, height: contentRect.height } : null,
    };
  });
}

function assertWindowInsideViewport(geometry, failures, phase) {
  const { rect, viewport } = geometry;
  if (rect.x < -2 || rect.y < -2 || rect.right > viewport.width + 2 || rect.bottom > viewport.height + 2) {
    failures.push(`${phase} window is outside viewport: ${JSON.stringify({ rect, viewport })}`);
  }
}

function assertShellGeometry(shell, label) {
  if (shell.documentOverflow.x) report.failures.push({ scope: label, message: 'document has horizontal overflow' });
  for (const [name, rect] of [['app shell', shell.appShell], ['topbar', shell.topbar], ['workspace', shell.workspace]]) {
    if (!rect) {
      report.failures.push({ scope: label, message: `${name} is not visible` });
      continue;
    }
    if (Math.abs(rect.x) > 1 || Math.abs(rect.right - shell.viewport.width) > 1) {
      report.failures.push({ scope: label, message: `${name} does not span the browser width: ${JSON.stringify({ rect, viewport: shell.viewport })}` });
    }
  }
  for (const [name, rect] of [['start menu button', shell.startButton], ['account/settings controls', shell.topbarActions]]) {
    if (!rect) {
      report.failures.push({ scope: label, message: `${name} is not visible` });
      continue;
    }
    if (rect.left < -1 || rect.right > shell.viewport.width + 1 || rect.top < -1 || rect.bottom > shell.viewport.height + 1) {
      report.failures.push({ scope: label, message: `${name} is outside viewport: ${JSON.stringify(rect)}` });
    }
  }
  if (shell.appShell && (Math.abs(shell.appShell.y) > 1 || Math.abs(shell.appShell.bottom - shell.viewport.height) > 1)) {
    report.failures.push({ scope: label, message: `app shell does not span the browser height: ${JSON.stringify({ rect: shell.appShell, viewport: shell.viewport })}` });
  }
  if (shell.bottomAppSwitcher) {
    report.failures.push({ scope: label, message: 'retired bottom app switcher is visible' });
  }
  for (const [name, rect] of [['chat dock', shell.chatDock], ...shell.chatWindows.map((value, index) => [`chat window ${index + 1}`, value])]) {
    if (!rect) continue;
    if (rect.x < -2 || rect.y < -2 || rect.right > shell.viewport.width + 2 || rect.bottom > shell.viewport.height + 2) {
      report.failures.push({ scope: label, message: `${name} is outside viewport: ${JSON.stringify(rect)}` });
    }
  }
}

function assertNoInteractiveOverlap(shell, failures) {
  // Business Chat is intentionally a user-controlled overlay. Reserving its
  // current height or width in the Window Manager caused the severe window
  // jumps that Revision 73 removed. Viewport containment is asserted by
  // assertShellGeometry; overlap with app content is not a failure because the
  // user can collapse the overlay without changing the app's geometry.
  if (shell.bottomAppSwitcher) failures.push('retired bottom app switcher is visible');
}

async function waitForShell(page) {
  await page.waitForFunction(() => {
    const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
    return document.body?.dataset?.authState !== 'locked'
      && state?.windowManager
      && Array.isArray(state?.modules)
      && state.modules.length >= 30
      && !document.body.dataset.moduleLoading;
  }, null, { timeout: readyTimeoutMs });
}

async function closeAllWindows(page) {
  await page.evaluate(() => globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.destroyAll?.());
  await page.waitForFunction(() => (
    (globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.listWindows?.() || []).length === 0
  ), null, { timeout: 5000 }).catch(() => {});
}

async function closeWindowByOwner(page, appId) {
  await page.evaluate((id) => {
    const manager = globalThis.ctoxBusinessOsSmoke?.state?.windowManager;
    for (const win of manager?.listWindows?.() || []) {
      if (win.ownerId === `desktop-app:${id}`) manager.destroy?.(win.id);
    }
  }, appId);
}

function attachDiagnostics(page) {
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) {
      report.consoleEvents.push({ type: message.type(), text: message.text() });
    }
  });
  page.on('pageerror', (error) => report.consoleEvents.push({ type: 'pageerror', text: error?.stack || String(error) }));
  page.on('requestfailed', (request) => report.consoleEvents.push({
    type: request.failure()?.errorText === 'net::ERR_ABORTED'
      ? 'expected-abort'
      : 'requestfailed',
    text: `${request.method()} ${request.url()} ${request.failure()?.errorText || ''}`,
  }));
}

async function attachLocalAssetRouting(page) {
  const assetRoot = path.join(repoRoot, 'src/apps/business-os');
  const installedIndex = localAssetPrefixes.includes('index.html')
    ? await fetch(targetUrl).then((response) => response.text()).catch(() => '')
    : '';
  const bootstrapScript = installedIndex.match(/<script>window\.CTOX_BUSINESS_OS_SESSION=[\s\S]*?<\/script>/)?.[0] || '';
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    const relativePath = decodeURIComponent(url.pathname === '/' ? 'index.html' : url.pathname.slice(1));
    const servedRelativePath = relativePath.replace(/^business-os\//, '');
    if (!localAssetPrefixes.some((prefix) => (
      servedRelativePath === prefix || servedRelativePath.startsWith(`${prefix}/`)
    ))) {
      await route.fallback();
      return;
    }
    const candidate = path.resolve(assetRoot, servedRelativePath);
    if (!candidate.startsWith(`${assetRoot}${path.sep}`)) {
      await route.fallback();
      return;
    }
    try {
      if (!fs.statSync(candidate).isFile()) throw new Error('not a file');
      const contentType = {
        '.html': 'text/html; charset=utf-8',
        '.js': 'text/javascript; charset=utf-8',
        '.mjs': 'text/javascript; charset=utf-8',
        '.css': 'text/css; charset=utf-8',
        '.json': 'application/json; charset=utf-8',
        '.svg': 'image/svg+xml',
      }[path.extname(candidate).toLowerCase()] || 'application/octet-stream';
      let body = fs.readFileSync(candidate);
      if (servedRelativePath === 'index.html' && bootstrapScript) {
        body = Buffer.from(body.toString('utf8').replace('</head>', `${bootstrapScript}</head>`));
      }
      await route.fulfill({ status: 200, contentType, body });
    } catch {
      await route.fallback();
    }
  });
}

function roundedRect(rect) {
  return {
    x: Math.round(rect.x),
    y: Math.round(rect.y),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
    right: Math.round(rect.right),
    bottom: Math.round(rect.bottom),
  };
}

function resolvePlaywrightModule() {
  const candidates = [
    process.env.BUSINESS_OS_PLAYWRIGHT_MODULE,
    path.join(repoRoot, 'runtime/browser/interactive-reference/node_modules/patchright'),
    path.join(repoRoot, 'node_modules/playwright'),
    'playwright',
  ].filter(Boolean);
  for (const candidate of candidates) {
    try {
      require.resolve(candidate);
      return candidate;
    } catch {
      // Continue to the next configured runtime.
    }
  }
  throw new Error('Playwright/Patchright is not installed for interactive Business OS QA');
}

function resolveChromiumExecutable(chromiumInstance) {
  const configured = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  if (configured && fs.existsSync(configured)) return configured;
  const bundled = path.join(
    repoRoot,
    'runtime/browser/interactive-reference/ms-playwright/chromium-1228/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
  );
  if (fs.existsSync(bundled)) return bundled;
  const playwrightExecutable = chromiumInstance.executablePath?.();
  if (playwrightExecutable && fs.existsSync(playwrightExecutable)) return playwrightExecutable;
  throw new Error('No installer-managed Chromium executable is available');
}

function cssEscape(value) {
  return String(value).replace(/(["\\])/g, '\\$1');
}

function slug(value) {
  return String(value).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

function positiveInteger(value) {
  const number = Number(value);
  if (!Number.isInteger(number) || number <= 0) throw new Error(`Expected positive integer, got ${value}`);
  return number;
}

function timestampForPath() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}
