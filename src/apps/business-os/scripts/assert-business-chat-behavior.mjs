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
const groupedScreenshotPath = path.join(outputDir, 'business-chat-grouped.png');
const progressScreenshotPath = path.join(outputDir, 'business-chat-progress.png');
const compactPromptScreenshotPath = path.join(outputDir, 'business-chat-compact-prompt.png');
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

  await scenario(page, 'collapsed-dock-ignores-old-open-chat', {
    count: 0,
    dockCollapsed: true,
    oldOpenOtherDate: true,
    preCollapseExpandedChatIds: ['chat_old_other_date'],
  }, async (m) => {
    expect(m.storedChats === 1, `old-date setup must start with one stored chat, got ${m.storedChats}`);
    expect(m.windowCount === 0, `old-date collapsed setup must not render today's window, got ${m.windowCount}`);
    const after = await page.evaluate(async () => {
      document.querySelector('[data-chat-open]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active textarea'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'collapsed-dock-after-open-current-date', metrics: after });
    expect(after.windowCount === 1, `opening current date must render one chat window, got ${after.windowCount}`);
    expect(after.activeTextareaCount === 1, `opening current date must render one active composer, got ${after.activeTextareaCount}`);
    expect(after.activeId !== 'chat_old_other_date', 'opening current date must not activate stale chat from another day');
  });

  await scenario(page, 'dock-opens-despite-transient-persist-timeout', {
    count: 0,
    dockCollapsed: true,
    dbTransientError: true,
  }, async () => {
    const after = await page.evaluate(async () => {
      document.querySelector('[data-chat-open]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active textarea'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'dock-opens-after-transient-persist-timeout', metrics: after });
    expect(after.windowCount === 1, `transient chat persistence timeout must not block dock open, got ${after.windowCount}`);
    expect(after.activeTextareaCount === 1, `transient chat persistence timeout must still render composer, got ${after.activeTextareaCount}`);
  });

  await scenario(page, 'one-chat-compact', { count: 1 }, (m) => {
    expect(m.windowCount === 1, 'one chat renders one window');
    expect(m.chipCount === 1, 'one chat renders one chip');
    expect(m.navCount === 0, 'one chat must not show prev/next controls');
    expect(m.stripCount === 1, 'one chat renders one strip');
    expect(m.headerNewCount === 0, 'window header must not contain new-chat plus button');
    expect(m.dateScopeText === '', `date control must stay icon-only, got ${m.dateScopeText}`);
    expect(m.dateTriggerLabel.includes('Crew-Einsätze'), `date trigger needs an accessible mission label, got ${m.dateTriggerLabel}`);
    expect(m.dockWidth < 520, `one-chat dock should stay compact, got ${m.dockWidth}`);
    expect(m.activeChipCenterWithinWindow === true, 'one-chat active chip center must sit under the active window');
    expect(m.activeWindowLeft >= 0 && m.activeWindowRight <= m.viewportWidth, 'one-crew window must stay inside the viewport');
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
    expect(m.dockWidth > 480, `six-chat dock should keep visible creatures, got ${m.dockWidth}`);
    expect(m.dockWidth > m.viewportWidth * 0.85, `six-chat dock should use the available workspace width, ratio ${m.dockRatio}`);
    expect(m.activeChipCenterWithinWindow === true, 'six-chat active chip center must sit under the active window');
    expect(m.activeWindowLeft >= 0 && m.activeWindowRight <= m.viewportWidth, 'six-member gallery must keep its active window inside the viewport');
  });

  await scenario(page, 'eight-chats-scrolls-but-not-full-width', { count: 8, activeIndex: 4 }, (m) => {
    expect(m.chipCount === 8, 'eight chats render eight chips');
    expect(m.navCount === 2, 'eight chats show strip nav');
    expect(m.stripHasOverflow === false, 'eight compact creature chips should fit without needless scrolling');
    expect(m.dockWidth > m.viewportWidth * 0.85, `eight-chat dock should use the available workspace width, ratio ${m.dockRatio}`);
    expect(m.activeChipCenterWithinWindow === true, 'eight-chat active chip center must sit under the active window');
    expect(m.activeWindowLeft >= 0 && m.activeWindowRight <= m.viewportWidth, 'eight-member gallery must keep its active window inside the viewport');
  });

  await scenario(page, 'twelve-chats-full-width-scroll', { count: 12, activeIndex: 5 }, async (m) => {
    expect(m.chipCount === 12, 'twelve chats render twelve chips');
    expect(m.navCount === 2, 'twelve chats show strip nav');
    expect(m.stripHasOverflow === false, 'twelve-chat strip should use available width before scrolling');
    expect(m.dockWidth > m.viewportWidth * 0.85, `twelve-member crew dock must use the available workspace width, ratio ${m.dockRatio}`);
    expect(m.activeChipCenterWithinWindow === true, 'twelve-chat active chip center must sit under the active window');
    expect(m.activeWindowLeft >= 0 && m.activeWindowRight <= m.viewportWidth, 'twelve-member gallery must keep its active window inside the viewport');
    await page.screenshot({ path: screenshotPath, fullPage: true });
  });

  await scenario(page, 'thousand-chats-virtualized-overflow', { count: 1000, activeIndex: 500 }, async (m) => {
    expect(m.storedChats === 1000, `thousand-chat setup must keep source data, got ${m.storedChats}`);
    expect(m.chipCount <= 12, `thousand-chat dock must cap rendered chips, got ${m.chipCount}`);
    expect(m.windowCount <= 12, `thousand-chat dock must cap rendered windows, got ${m.windowCount}`);
    expect(m.overflowCount === 1, 'thousand-chat dock must expose one overflow chip');
    expect(m.dateTriggerLabel.includes('1k Aufgaben'), `date hover hint should expose compact workload, got ${m.dateTriggerLabel}`);
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

  await scenario(page, 'web-research-tasks-grouped', { count: 120, activeIndex: 60, groupedResearch: true }, async () => {
    const open = await page.evaluate(async () => {
      document.querySelector('[data-chat-overflow-open]').click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-busy-panel]'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'web-research-tasks-grouped-open', metrics: open });
    expect(open.busyPanelCount === 1, 'grouped research setup must open busy-day panel');
    expect(open.groupFilterValue === 'auto', `busy panel should default to auto grouping, got ${open.groupFilterValue}`);
    expect(open.busyGroupCount === 1, `related research tasks must collapse into one group, got ${open.busyGroupCount}`);
    expect(open.busyGroupFirstLabel.includes('Web Research'), `group label should name the research series, got ${open.busyGroupFirstLabel}`);
    expect(open.busyRowCount <= 80, `grouped research panel must cap rendered task rows, got ${open.busyRowCount}`);
    expect(open.busyGroupMoreText.includes('40'), `research group should summarize the 40 hidden tasks, got ${open.busyGroupMoreText}`);
    expect(open.busyMoreText.includes('40 weitere'), `busy panel should expose remaining filtered matches, got ${open.busyMoreText}`);
    await page.screenshot({ path: groupedScreenshotPath, fullPage: true });
  });

  await scenario(page, 'expanded-crew-windows-render-side-by-side', { count: 3, activeIndex: 1, crewMembers: 4 }, async (m) => {
    // Regression 08.09.2026: an extra dock child (crew pool row) shifted the chip
    // strip into a 26px grid column, so every chip sat at the right edge and the
    // three windows piled on top of each other while still claiming side-by-side.
    expect(m.fabMemberCount === 4, `the crew button must carry the pool members, got ${m.fabMemberCount}`);
    expect(m.stripWidth >= 120, `the chip strip must keep its own room in the dock, got ${m.stripWidth}px`);
    expect(m.windowOverlap <= 0, `side-by-side windows must not overlap, worst overlap ${m.windowOverlap}px (${m.windowRects.join(' | ')})`);
    expect(m.inactiveFocusable >= 3, `inactive window actions must be directly tabbable, got ${m.inactiveFocusable}`);
    expect(m.inactiveVisibleActions >= 1, `inactive header actions must remain visible, got ${m.inactiveVisibleActions}`);
    expect(m.windowCount === 3, `the stage must render all three expanded crew windows, got ${m.windowCount}`);
    expect(m.renderedWindowIds.join(',') === 'chat_0,chat_1,chat_2', `the rendered crew should preserve its order, got ${m.renderedWindowIds.join(',')}`);
    expect(m.stageClasses.includes('is-side-by-side'), `three crew windows should arrange side by side, got ${m.stageClasses}`);
    expect(m.windowCreatureCount === 3, `each work window needs its own creature, got ${m.windowCreatureCount}`);
    expect(m.dockCreatureCount === 3, `the dock must show the same three crew members, got ${m.dockCreatureCount}`);
    expect(m.dockLabel === 'Crew', `crew navigation label must remain visible, got ${m.dockLabel}`);
  });

  await viewportScenario(page, 'carousel-fan-stays-on-stage-with-left-active-chip', { width: 1000, height: 820 }, { count: 3, activeIndex: 0 }, async (m) => {
    expect(!m.stageClasses.includes('is-side-by-side'), `three 460px windows in a 1000px stage must fan out, got ${m.stageClasses}`);
    expect(m.windowCount === 3, `all three windows must render, got ${m.windowCount}`);
    expect(m.windowMinLeft >= m.stageLeft - 0.5, `the fan must not hang off the left stage edge: min left ${m.windowMinLeft} < stage ${m.stageLeft} (${m.windowRects.join(' | ')})`);
    expect(m.windowMaxRight <= m.stageRight + 0.5, `the fan must not hang off the right stage edge: max right ${m.windowMaxRight} > stage ${m.stageRight}`);
  });

  await scenario(page, 'crew-presence-on-app-window-and-desktop-icon', {
    count: 1,
    activeIndex: 0,
    crewMembers: 3,
    appPresence: true,
    queueTasks: [
      { id: 'task_doc_1', status: 'running', module: 'documents', crew_member_id: 'member_0', updated_at_ms: Date.now() },
      { id: 'task_doc_2', status: 'leased', module: 'documents', crew_member_id: 'member_1', updated_at_ms: Date.now() - 1000 },
      { id: 'task_ctox_done', status: 'succeeded', module: 'ctox', crew_member_id: 'member_2', updated_at_ms: Date.now() - 2000 },
    ],
  }, async () => {
    const after = await page.evaluate(async () => {
      await window.chatHarness.waitFor(() => document.querySelectorAll('[data-crew-presence]').length >= 2);
      await window.chatHarness.waitForPaint();
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'crew-presence-after-load', metrics: after });
    await page.screenshot({ path: path.join(outputDir, 'crew-presence.png'), clip: { x: 280, y: 100, width: 900, height: 400 } });
    const byHost = Object.fromEntries((after.appPresence || []).map((entry) => [entry.host, entry]));
    expect(byHost['window:module:documents']?.creatures === 2, `documents window icon must carry both working members, got ${JSON.stringify(byHost['window:module:documents'])}`);
    expect(byHost['desktop:documents']?.creatures === 2, `documents desktop icon must carry both working members, got ${JSON.stringify(byHost['desktop:documents'])}`);
    expect(/Pico, Nia arbeiten hier/.test(byHost['window:module:documents']?.title || ''), `presence hint must name the members, got ${byHost['window:module:documents']?.title}`);
    expect(byHost['window:module:ctox']?.creatures === 0, `finished tasks must not show presence, got ${JSON.stringify(byHost['window:module:ctox'])}`);
    expect(byHost['desktop:ctox']?.creatures === 0, `finished tasks must not show desktop presence, got ${JSON.stringify(byHost['desktop:ctox'])}`);
    expect((after.appPresence || []).every((entry) => entry.inside), `presence badges must stay inside their icon, got ${JSON.stringify(after.appPresence)}`);
  });

  await scenario(page, 'inactive-window-minimizes-with-one-click', { count: 3, activeIndex: 1 }, async () => {
    const after = await page.evaluate(async () => {
      const button = document.querySelector('.ctox-chat-window[data-chat-id="chat_0"] [data-chat-minimize]');
      if (!button) throw new Error('inactive minimize action is not visible');
      button.click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-focus="chat_0"]')?.classList.contains('is-minimized'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'inactive-window-minimized-after-one-click', metrics: after });
    expect(after.minimizedChipIds.includes('chat_0'), 'one click on an inactive window must minimize that window');
    expect(after.activeId === 'chat_1', `minimizing an inactive window must not steal focus, got ${after.activeId}`);
  });

  await scenario(page, 'crew-task-renders-quiet-visual-progress', {
    count: 1,
    activeIndex: 0,
    groupedResearch: true,
    staticTracking: true,
    progressTracking: true,
  }, async (m) => {
    expect(m.progressCardCount === 1, `running crew task needs one progress card, got ${m.progressCardCount}`);
    expect(m.progressHeaderText === '', `window header must not duplicate progress copy, got ${m.progressHeaderText}`);
    expect(m.progressSegmentCount === 4, `three work steps plus review segment expected, got ${m.progressSegmentCount}`);
    expect(m.progressTooltip.includes('Daten prüfen'), `hover hint must expose the active step, got ${m.progressTooltip}`);
    expect(m.progressTooltip.includes('30% · 4/7 Aktivitäten · Planstand 2'), `hover hint must expose exact progress, got ${m.progressTooltip}`);
    expect(m.progressTooltip.includes('→ Ergebnis schreiben'), `hover hint must expose the next step, got ${m.progressTooltip}`);
    expect(m.progressCurrentText === '' && m.progressNextText === '' && m.progressPlanText === '', 'progress labels must not be persistently visible');
    expect(m.progressInHeader === true, 'visual progress must live inside the window header');
    expect(m.progressClockCount === 1, `header needs one visual turn clock, got ${m.progressClockCount}`);
    expect(m.progressTrackWidth >= 180, `header step progress must remain clearly visible, got ${m.progressTrackWidth}px`);
    expect(m.progressTrackHeight >= 5, `header frame progress must be visibly weighted, got ${m.progressTrackHeight}px`);
    expect(m.progressTrackEdgeDelta <= 1, `progress must span the full header frame, edge delta ${m.progressTrackEdgeDelta}px`);
    expect(m.progressCrewColor !== '', 'window must expose its creature color to progress chrome');
    expect(m.progressFillBackground !== 'rgba(0, 0, 0, 0)', `progress fill must resolve to a visible color, got ${m.progressFillBackground}`);
    expect(m.progressClockBorder !== 'rgb(0, 0, 0)', `turn clock must resolve to a visible crew-colored border, got ${m.progressClockBorder}`);
    expect(m.headerHeight >= 60 && m.headerHeight <= 68, `header must stay readable at about 64px, got ${m.headerHeight}`);
    expect(m.firstMessageTop >= m.headerBottom - 0.5, `chat copy must start below the header: ${m.firstMessageTop} < ${m.headerBottom}`);
    expect(m.activeWindowHeight >= 420, `crew window must retain a readable work area, got ${m.activeWindowHeight}`);
    expect(m.visibleChromeText === '', `status chrome must be text-free, got ${m.visibleChromeText}`);
    await page.screenshot({ path: progressScreenshotPath, fullPage: true });
  });

  await scenario(page, 'inspection-survives-live-projection-while-typing', {
    count: 1, activeIndex: 0, groupedResearch: true, progressTracking: true, crewMembers: 4,
  }, async () => {
    await page.getByPlaceholder('Aufgabe für Pico...').waitFor();
    const inspection = page.locator('.ctox-chat-window.is-active .ctox-chat-inspection');
    await inspection.locator('summary').click();
    const input = page.locator('.ctox-chat-window.is-active textarea');
    await input.fill('Bitte anschließend kurz zusammenfassen.');
    await page.evaluate(async () => window.chatHarness.publishMessage('chat_0', {
      id: 'live-inspection-event', role: 'ctox', kind: 'status',
      taskId: 'task_research_0', commandId: 'task_research_0', status: 'running',
      text: 'Ein neuer Arbeitsschritt läuft.', createdAt: Date.now(),
    }));
    await inspection.getByText('Ein neuer Arbeitsschritt läuft.', { exact: true }).first().waitFor().catch(async (error) => {
      console.error('live-projection-diagnostics', await page.evaluate(() => ({
        stored: JSON.parse(localStorage.getItem('ctox.businessOs.chat.v1') || '{}'),
        inspection: document.querySelector('.ctox-chat-window.is-active .ctox-chat-inspection')?.outerHTML,
        focused: document.activeElement?.tagName,
      })));
      throw error;
    });

    expect(await inspection.getAttribute('open') === '', 'live projection must keep inspection open');
    expect(await input.inputValue() === 'Bitte anschließend kurz zusammenfassen.', 'live projection must preserve the draft');
    expect(await input.evaluate(e => e === document.activeElement), 'live projection must preserve typing focus');
    expect(await input.getAttribute('placeholder') === 'Aufgabe für Pico...', 'live projection retains the assigned member');
    expect((await page.locator('.ctox-chat-window.is-active [data-chat-title]').getAttribute('aria-label')).startsWith('Pico ·'), 'header identifies the actual assigned member');
    expect(JSON.parse(await page.locator('.ctox-chat-window.is-active .ctox-crew-creature').getAttribute('data-crew-identity')).name === 'Pico', 'creature appearance follows the assigned member');
    expect(await inspection.locator('.ctox-chat-inspection-steps li').count() === 3, 'inspection keeps the same plan steps');
    expect(!(await page.locator('.ctox-chat-window.is-active .ctox-chat-messages').innerText()).includes('Ein neuer Arbeitsschritt läuft.'), 'work status must not appear as a crew reply');
  });

  await scenario(page, 'task-link-opens-unobstructed-flow', { count: 1, activeIndex: 0, groupedResearch: true }, async () => {
    await page.locator('.ctox-chat-inspection > summary').click();
    const navigation = await page.evaluate(async () => {
      const link = document.querySelector('.ctox-chat-inspection [data-track-task]');
      if (!link) throw new Error('tracked message has no CTOX task link');
      link.click();
      await window.chatHarness.waitFor(() => location.hash.includes('task_id='));
      return {
        hash: location.hash, title: link.getAttribute('title') || '',
        expandedChats: document.querySelectorAll('.ctox-chat-window:not(.is-minimized)').length,
        stageHeight: document.querySelector('.ctox-chat-stage-inner')?.getBoundingClientRect().height || 0,
      };
    });
    results.push({ scenario: 'task-link-navigation', navigation });
    expect(navigation.hash.includes('#ctox?'), `task link must navigate into CTOX, got ${navigation.hash}`);
    expect(navigation.hash.includes('drawer=0'), `task link must show the flow without a drawer, got ${navigation.hash}`);
    expect(navigation.expandedChats === 0, 'task navigation must minimize overlaying chats');
    expect(navigation.stageHeight === 0, 'minimized chat stage must not cover the flow');
    expect(navigation.title.includes('task_research_0'), `task link hover must expose its stable id, got ${navigation.title}`);
  });

  await scenario(page, 'overflowing-crew-folds-into-3d-gallery', { count: 7, activeIndex: 3 }, async (m) => {
    expect(m.windowCount === 7, `the 3D stage must retain all seven crew windows, got ${m.windowCount}`);
    expect(!m.stageClasses.includes('is-side-by-side'), `seven windows should overflow into the gallery, got ${m.stageClasses}`);
    expect(m.inactiveVisible === true, 'inactive gallery windows must remain visibly folded behind the active window');
    expect(m.inactiveTransform !== 'none', `inactive gallery windows need a 3D transform, got ${m.inactiveTransform}`);
  });

  await scenario(page, 'long-app-prompt-stays-collapsed-and-compact', {
    count: 1,
    messagesPerChat: 1,
    longMessages: true,
  }, async (m) => {
    expect(m.compactPromptCount === 1, `long prompt needs one compact disclosure, got ${m.compactPromptCount}`);
    expect(m.compactPromptOpen === false, 'long prompt must start collapsed');
    expect(m.compactPromptPreviewHeight <= 46, `collapsed prompt preview must stay within two lines, got ${m.compactPromptPreviewHeight}`);
    expect(m.activeWindowHeight >= 420, `collapsed prompt must retain the readable work area, got ${m.activeWindowHeight}`);
    expect(m.composerInline === true, 'attachment, input and send must share one aligned composer row');
    await page.screenshot({ path: compactPromptScreenshotPath, fullPage: true });
  });

  await scenario(page, 'minimized-active-window-leaves-chip-only', { count: 2, activeIndex: 0 }, async () => {
    const after = await page.evaluate(async () => {
      document.querySelector('.ctox-chat-window.is-active [data-chat-minimize]').click();
      await window.chatHarness.waitFor(() => (
        !document.querySelector('.ctox-chat-window[data-chat-id="chat_0"]')
        && document.querySelector('.ctox-chat-window.is-active')?.dataset.chatId === 'chat_1'
      ));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'minimized-active-window-after-click', metrics: after });
    expect(after.windowCount === 1, `minimizing one of two chats must leave one rendered window, got ${after.windowCount}`);
    expect(after.minimizedWindowCount === 0, `minimized chats must not stay rendered as gray windows, got ${after.minimizedWindowCount}`);
    expect(after.minimizedChipCount === 1, `minimized chat must remain available as a chip, got ${after.minimizedChipCount}`);
    expect(after.renderedWindowIds.join(',') === 'chat_1', `remaining rendered window should be chat_1, got ${after.renderedWindowIds.join(',')}`);
  });

  await scenario(page, 'minimized-running-chip-keeps-status', {
    count: 6,
    activeIndex: 0,
    groupedResearch: true,
    staticTracking: true,
  }, async () => {
    const after = await page.evaluate(async () => {
      const runningWindow = document.querySelector('.ctox-chat-window[data-chat-id="chat_0"]');
      const minimizeButton = runningWindow?.querySelector('[data-chat-minimize]');
      if (!minimizeButton) throw new Error('running chat_0 window is not available to minimize');
      minimizeButton.click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-chat-focus="chat_0"]')?.classList.contains('is-minimized'));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'minimized-running-chip-after-click', metrics: after });
    expect(after.minimizedRunningChipCount === 1, `minimized running chat must keep running status styling, got ${after.minimizedRunningChipCount}`);
    expect(after.minimizedRunningTitle.includes('Aktiv'), `running minimized chat hover hint should expose Aktiv, got ${after.minimizedRunningTitle}`);
  });

  await scenario(page, 'keyboard-focus-keeps-inactive-header-actions-operable', { count: 4, activeIndex: 1 }, async () => {
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
          inactiveHeaderControl: Boolean(active?.closest?.('.ctox-chat-window:not(.is-active) .ctox-chat-header-actions, .ctox-chat-window:not(.is-active) .ctox-chat-delegation-card')),
          label: active?.getAttribute?.('aria-label') || active?.textContent?.trim()?.slice(0, 40) || '',
        };
      }));
    }
    results.push({ scenario: 'keyboard-focus-trace', focusTrace });
    expect(focusTrace.every((item) => !item.inactiveWindow || item.inactiveHeaderControl), 'tab focus may enter only the direct header actions of inactive chat windows');
    expect(focusTrace.every((item) => !String(item.className).includes('ctox-date-native-picker')), 'tab focus must not enter hidden native date input');
  });

  await scenario(page, 'maximize-restores-size-and-minimize-clears-stage', { count: 1, activeIndex: 0 }, async () => {
    const normal = await page.locator('.ctox-chat-window.is-active').boundingBox();
    await page.locator('.ctox-chat-window.is-active [data-chat-maximize]').click();
    await page.waitForFunction(() => document.querySelector('.ctox-chat-window.is-maximized')?.getBoundingClientRect().width > 700);
    const maximized = await page.locator('.ctox-chat-window.is-active').boundingBox();
    expect(maximized.width > normal.width * 1.5, 'maximize must visibly grow the window');
    expect(maximized.height > normal.height, 'maximize must use the available height');
    await page.locator('.ctox-chat-window.is-active [data-chat-maximize]').click();
    await page.waitForFunction(() => !document.querySelector('.ctox-chat-window.is-maximized'));
    await page.evaluate(() => window.chatHarness.waitForPaint());
    const restored = await page.locator('.ctox-chat-window.is-active').boundingBox();
    expect(Math.abs(restored.width - normal.width) < 2, 'restore must recover the original width');
    await page.locator('.ctox-chat-window.is-active [data-chat-minimize]').click();
    await page.waitForFunction(() => document.querySelectorAll('.ctox-chat-window').length === 0);
    expect(await page.locator('.ctox-chat-stage-inner').evaluate(node => node.getBoundingClientRect().height) === 0, 'empty stage must not intercept the app');
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
      await window.chatHarness.waitFor(() => (
        document.querySelectorAll('.ctox-chat-window').length === 0
        && document.querySelector('[data-chat-focus="chat_0"]')?.classList.contains('is-minimized')
      ));
      return performance.now() - start;
    });
    results.push({ scenario: 'active-control-latency-ms', maximizeLatency, minimizeLatency });
    expect(maximizeLatency < 150, `maximize must render before persistence delay, got ${maximizeLatency.toFixed(1)}ms`);
    expect(minimizeLatency < 150, `minimize must render before persistence delay, got ${minimizeLatency.toFixed(1)}ms`);
  });

  await scenario(page, 'follow-up-renders-before-db-delay', {
    count: 1,
    activeIndex: 0,
    failedChat: true,
    dbDelay: 500,
  }, async () => {
    const followUpResult = await page.evaluate(async () => {
      // Failed work must leave its input usable without a reveal action,
      // even while database writes are delayed.
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active textarea[name="message"]'));
      return {
        composerVisible: Boolean(document.querySelector('.ctox-chat-window.is-active textarea[name="message"]')),
        triggerVisible: Boolean(document.querySelector('.ctox-chat-window.is-active [data-chat-followup-trigger]')),
      };
    });
    results.push({ scenario: 'follow-up-control-result', followUpResult });
    expect(followUpResult.composerVisible, 'follow-up click must render the follow-up composer');
    expect(!followUpResult.triggerVisible, 'conversation input must not require an extra reveal button');
    const followUpCommand = await page.evaluate(async () => {
      const textarea = document.querySelector('.ctox-chat-window.is-active textarea[name="message"]');
      textarea.value = 'Korrigierte Folgeaufgabe';
      textarea.dispatchEvent(new InputEvent('input', { bubbles: true }));
      document.querySelector('.ctox-chat-window.is-active [data-chat-send]').click();
      await window.chatHarness.waitFor(() => window.chatHarness.lastCommand);
      const command = window.chatHarness.lastCommand;
      return {
        instruction: command.payload.instruction,
        prompt: command.payload.prompt,
        continuation: command.payload.continuation,
        sourceTaskId: command.payload.source_task_id,
        sourceCommandId: command.payload.source_command_id,
      };
    });
    results.push({ scenario: 'follow-up-command-payload', followUpCommand });
    expect(followUpCommand.instruction === 'Korrigierte Folgeaufgabe', `follow-up instruction must use current user text, got ${followUpCommand.instruction}`);
    expect(followUpCommand.prompt === 'Korrigierte Folgeaufgabe', `follow-up prompt must use current user text, got ${followUpCommand.prompt}`);
    expect(followUpCommand.continuation === true, 'follow-up command must declare durable continuation');
    expect(followUpCommand.sourceTaskId === 'task_failed_0', `follow-up command must link source task, got ${followUpCommand.sourceTaskId}`);
    expect(followUpCommand.sourceCommandId === 'task_failed_0', `follow-up command must link source command, got ${followUpCommand.sourceCommandId}`);
  });

  await scenario(page, 'delete-renders-before-db-delay', { count: 1, dbDelay: 500 }, async () => {
    const deleteLatency = await page.evaluate(async () => {
      const start = performance.now();
      document.querySelector('.ctox-chat-window.is-active [data-chat-delete]').click();
      await window.chatHarness.waitFor(() => document.querySelectorAll('.ctox-chat-window').length === 0, 1200);
      return performance.now() - start;
    });
    const after = await page.evaluate(() => window.chatHarness.collect());
    results.push({ scenario: 'delete-control-latency-ms', deleteLatency, metrics: after });
    expect(deleteLatency < 350, `delete must render before persistence delay, got ${deleteLatency.toFixed(1)}ms`);
    expect(after.windowCount === 0, `deleted chat window must disappear, got ${after.windowCount}`);
    expect(after.storedChats === 0, `deleted chat must leave local state, got ${after.storedChats}`);
  });

  await scenario(page, 'delete-non-empty-chat-still-confirms', { count: 1, messagesPerChat: 1 }, async () => {
    const confirmState = await page.evaluate(async () => {
      document.querySelector('.ctox-chat-window.is-active [data-chat-delete]').click();
      await window.chatHarness.waitFor(() => document.querySelector('[data-dialog-confirm]'));
      const metrics = window.chatHarness.collect();
      const hasConfirm = Boolean(document.querySelector('[data-dialog-confirm]'));
      document.querySelector('[data-dialog-cancel]')?.click();
      return { hasConfirm, metrics };
    });
    results.push({ scenario: 'delete-non-empty-confirm-state', confirmState });
    expect(confirmState.hasConfirm === true, 'non-empty chat deletion must still show confirmation');
    expect(confirmState.metrics.windowCount === 1, `non-empty chat must remain visible before confirmation, got ${confirmState.metrics.windowCount}`);
    expect(confirmState.metrics.storedChats === 1, `non-empty chat must remain in local state before confirmation, got ${confirmState.metrics.storedChats}`);
  });

  await scenario(page, 'delete-tombstone-blocks-rxdb-resurrection', { count: 1, dbDeleteError: true }, async () => {
    const after = await page.evaluate(async () => {
      document.querySelector('.ctox-chat-window.is-active [data-chat-delete]').click();
      await window.chatHarness.waitFor(() => document.querySelectorAll('.ctox-chat-window').length === 0);
      await window.chatHarness.emitChats();
      await window.chatHarness.waitForPaint();
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'delete-after-rxdb-resurrection-attempt', metrics: after });
    expect(after.windowCount === 0, `locally deleted chat must not rehydrate from stale RxDB doc, got ${after.windowCount}`);
    expect(after.storedChats === 0, `locally deleted chat must stay out of local state, got ${after.storedChats}`);
    expect(after.deletedChatTombstones === 1, `failed remote remove must leave one local tombstone, got ${after.deletedChatTombstones}`);
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

  for (const pendingRead of [false, true]) {
    await scenario(page, pendingRead ? 'crew-cleanup-during-load' : 'crew-cleanup-after-load', {
      count: 0, dbDelay: pendingRead ? 500 : 0,
    }, async () => {
      const stats = await page.evaluate(async (pending) => {
        if (!pending) await window.chatHarness.waitFor(() => window.chatHarness.chatReadStats.crewSubscriptions === 2);
        const root = document.querySelector('[data-ctox-chat-root]');
        const dbStats = window.chatHarness.chatReadStats;
        const readinessStats = window.chatHarness.readinessStats;
        const busStats = window.chatHarness.eventBusStats;
        root.__ctoxChatCleanup();
        const atDispose = { ...dbStats };
        window.chatHarness.emitCrew();
        window.chatHarness.emitReadiness();
        // Cover both an in-flight load and the first 2-second pool retry.
        await new Promise((resolve) => setTimeout(resolve, 2300));
        window.chatHarness.emitCrew();
        window.chatHarness.emitReadiness();
        await window.chatHarness.waitForPaint();
        return { atDispose, after: { ...dbStats }, readinessSubscriptions: readinessStats.subscriptions, windowObservers: busStats.windowOpenedObservers };
      }, pendingRead);
      results.push({ scenario: pendingRead ? 'crew-late-load-cleanup' : 'crew-ready-cleanup', metrics: stats });
      expect(stats.after.crewSubscriptions === 0, 'disposed crew pool must release its subscriptions and cannot subscribe after a late load');
      expect(stats.readinessSubscriptions === 0, 'disposed crew views must release readiness listeners');
      expect(stats.windowObservers === 0, 'disposed crew presence must release the real window event-bus token');
      expect(stats.after.crewReads === stats.atDispose.crewReads, 'disposed crew views must not restart reads via notifications or retries');
    });
  }

  for (const reuseKnown of [false, true]) {
    await scenario(page, reuseKnown ? 'known-chat-opens-before-storage' : 'new-draft-opens-before-storage', {
      count: 1, groupedResearch: true, messagesPerChat: 2, staticTracking: true, dockCollapsed: true, dbDelay: 1000, holdChatReads: true,
    }, async (initial) => {
      expect(initial.chatReadStats.started > 0 && initial.chatReadStats.completed === 0, 'the initial dock must render while history is still pending');
      expect(initial.initialPaintMs < 150, `the initial dock must paint before storage, got ${initial.initialPaintMs} ms`);
      const opened = await page.evaluate(async (reuse) => {
        const start = performance.now();
        window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail: {
          title: 'Immediate view', reuseActive: false,
          ...(reuse ? { task_id: 'task_research_0' } : { draft: 'Prepared prompt' }),
        } }));
        await window.chatHarness.waitFor(() => {
          const chat = document.querySelector('.ctox-chat-window.is-active');
          if (!chat?.querySelector('.ctox-chat-title')?.getAttribute('aria-label')?.includes('Immediate view')) return false;
          return reuse
            ? chat.dataset.chatId === 'chat_0' && chat.textContent.includes('Testnachricht 0')
            : chat.querySelector('textarea')?.value === 'Prepared prompt';
        });
        await window.chatHarness.waitForPaint();
        const chat = document.querySelector('.ctox-chat-window.is-active');
        const rect = chat.getBoundingClientRect();
        const style = getComputedStyle(chat);
        return { firstPaintMs: performance.now() - start, visible: rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth && style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) > 0, ...window.chatHarness.collect() };
      }, reuseKnown);
      results.push({ scenario: reuseKnown ? 'known-chat-first-paint' : 'new-draft-first-paint', metrics: opened });
      expect(opened.visible, 'the opened chat must be visible');
      expect(opened.chatReadStats.completed === 0, 'opening must not wait for the held history read');
      expect(opened.firstPaintMs < 150, `chat must paint before 1000 ms storage, got ${opened.firstPaintMs} ms`);
      expect(opened.storedChats === (reuseKnown ? 1 : 2), 'opening must preserve existing chats without duplicating a known task');
      if (reuseKnown) expect(opened.activeId === 'chat_0', 'known task must reuse its original chat ID');
      if (!reuseKnown) await page.locator('.ctox-chat-window.is-active textarea').fill('Edited while storage is pending');
      await page.evaluate(() => window.chatHarness.releaseChatReads());
      await page.evaluate(() => window.chatHarness.waitFor(() => window.chatHarness.chatReadStats.completed > 0));
      await page.evaluate(() => window.chatHarness.waitForPaint());
      const hydrated = await page.evaluate(() => ({
        ...window.chatHarness.collect(),
        draft: document.querySelector('.ctox-chat-window.is-active textarea')?.value,
      }));
      expect(hydrated.activeId === opened.activeId, 'late hydration must preserve the selected chat');
      expect(hydrated.storedChats === opened.storedChats, 'late hydration must not duplicate chats');
      if (reuseKnown) {
        expect(hydrated.activeTaskClass.includes('is-task-running'), 'opening a running chat must retain its active task state');
        expect(hydrated.activeMessageText.includes('Testnachricht 0'), 'late hydration must preserve the running chat history');
      } else {
        expect(hydrated.draft === 'Edited while storage is pending', 'late hydration must preserve user edits');
      }
    });
  }

  await scenario(page, 'unloaded-task-reuses-history-after-lookup', {
    count: 1, groupedResearch: true, staticTracking: true, remoteOnly: true, dbDelay: 500, holdChatReads: true,
  }, async () => {
    const resolved = await page.evaluate(async () => {
      window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail: { task_id: 'task_research_0' } }));
      await window.chatHarness.waitFor(() => window.chatHarness.chatReadStats.started >= 2);
      window.chatHarness.releaseChatReads();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.dataset.chatId === 'chat_0');
      return window.chatHarness.collect();
    });
    expect(resolved.storedChats === 1, 'remote-only task must resolve its existing history without a placeholder duplicate');
  });

  await scenario(page, 'delayed-task-lookup-cannot-reopen-over-new-draft', {
    count: 1, groupedResearch: true, staticTracking: true, remoteOnly: true, dbDelay: 500, holdChatReads: true,
  }, async () => {
    const after = await page.evaluate(async () => {
      window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail: { task_id: 'task_research_0' } }));
      // Keep the initial hydration and explicit lookup pending until the newer
      // user intent is visible, independently of CI scheduling delays.
      await window.chatHarness.waitFor(() => window.chatHarness.chatReadStats.started >= 2);
      window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail: { draft: 'Newer user intent', reuseActive: false } }));
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active textarea')?.value === 'Newer user intent');
      const activeId = window.chatHarness.collect().activeId;
      window.chatHarness.releaseChatReads();
      await window.chatHarness.waitFor(() => window.chatHarness.chatReadStats.completed >= 2);
      await window.chatHarness.waitForPaint();
      return { intendedId: activeId, ...window.chatHarness.collect(), draft: document.querySelector('.ctox-chat-window.is-active textarea')?.value };
    });
    expect(after.activeId === after.intendedId && after.activeId !== 'chat_0', 'late task lookup must not steal focus');
    expect(after.draft === 'Newer user intent', 'late task lookup must not overwrite the newer draft');
    expect(after.storedChats === 2, 'remote history and the explicit new draft must each remain once');
  });

  await scenario(page, 'active-input-focus-and-type', { count: 1 }, async () => {
    await page.click('.ctox-chat-window.is-active textarea');
    await page.keyboard.type('Browser Test Aufgabe');
    const draftValue = await page.evaluate(() => document.querySelector('.ctox-chat-window.is-active textarea')?.value || '');
    results.push({ scenario: 'active-input-draft-value', draftValue });
    expect(draftValue === 'Browser Test Aufgabe', `active chat textarea must accept typing, got ${JSON.stringify(draftValue)}`);
  });

  await scenario(page, 'transient-command-timeout-keeps-chat-trackable', {
    count: 1,
    commandError: 'transient',
  }, async () => {
    const after = await page.evaluate(async () => {
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active textarea'));
      const input = document.querySelector('.ctox-chat-window.is-active textarea');
      input.value = 'Bitte als CTOX Task verarbeiten';
      input.dispatchEvent(new InputEvent('input', { bubbles: true }));
      document.querySelector('.ctox-chat-window.is-active [data-chat-send]').click();
      await window.chatHarness.waitFor(() => /Die Crew hat die Annahme noch nicht bestätigt/.test(document.body.innerText || ''));
      return window.chatHarness.collect();
    });
    results.push({ scenario: 'transient-command-timeout-after-send', metrics: after });
    expect(after.activeTaskClass.includes('is-task-queued'), `transient command timeout must keep chat queued, got ${after.activeTaskClass}`);
    expect(!after.activeTaskClass.includes('is-task-failed'), `transient command timeout must not mark failed, got ${after.activeTaskClass}`);
    expect(after.activeInspectionText.includes('Die Crew hat die Annahme noch nicht bestätigt'), 'inspection must explain an unconfirmed submission');
    expect(!after.activeMessageText.includes('Die Crew hat die Annahme noch nicht bestätigt'), 'a transport receipt must not impersonate a crew reply');
    expect(!after.activeMessageText.includes('Task an CTOX übergeben'), 'an unconfirmed timeout must not claim native acceptance');
    expect(!after.activeMessageText.includes('pending_sync'), 'pending receipt enum must not leak as visible text');
    expect(!after.activeMessageText.includes('queued'), `transient status must not leak as chrome text, got ${after.activeMessageText}`);
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
    // The dock deliberately runs the full shell width (18px margins) and keeps
    // the bug-reporter slot free at its right end via padding, so the chip
    // strip must end before that slot instead of the dock shrinking.
    expect(m.dockWidth <= m.viewportWidth - 36, `1440px dock must stay inside the shell margins for eight chats, got ${m.dockWidth}`);
    expect(m.stripRight <= m.viewportWidth - 18 - 58, `1440px chip strip must leave the reporter slot free, ends at ${m.stripRight}`);
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


  await scenario(page, 'terminal-failure-reconciles-delayed-task-and-preserves-composer', {
    count: 1,
    messagesPerChat: 12,
    longMessages: true,
    reviewFailureReconciliation: true,
    crewMembers: 1,
  }, async (m) => {
    expect(m.progressReviewing === true, `running review must use active review styling, got reviewing=${m.progressReviewing}`);
    expect(m.progressPercent === '90', `running review must keep 90 percent, got ${m.progressPercent}`);
    expect(m.activeTaskClass.includes('is-task-running'), `review chat must start running, got ${m.activeTaskClass}`);
    expect(m.takeoverCount === 1, `historical takeover must render once, got ${m.takeoverCount}`);
    const prepared = await page.evaluate(async () => {
      document.querySelector('.ctox-chat-window.is-active [data-chat-maximize]').click();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.classList.contains('is-maximized'));
      const win = document.querySelector('.ctox-chat-window.is-active');
      const textarea = win?.querySelector('textarea[name="message"]');
      const inspection = win?.querySelector('.ctox-chat-inspection');
      if (inspection && inspection.open !== true) inspection.open = true;
      textarea.focus();
      textarea.value = 'Bitte nicht verlieren';
      textarea.dispatchEvent(new InputEvent('input', { bubbles: true }));
      textarea.setSelectionRange(7, 12);
      const pane = win?.querySelector('.ctox-chat-messages');
      if (pane) pane.scrollTop = 12;
      window.__chatTextarea = textarea;
      return {
        focused: document.activeElement === textarea,
        selectionStart: textarea.selectionStart,
        selectionEnd: textarea.selectionEnd,
        scrollTop: pane?.scrollTop || 0,
      };
    });
    results.push({ scenario: 'terminal-failure-prepared', prepared });
    expect(prepared.selectionStart === 7 && prepared.selectionEnd === 12, `caret must remain in the composer, got ${prepared.selectionStart}:${prepared.selectionEnd}`);

    const afterCommand = await page.evaluate(async () => {
      window.chatHarness.upsertCommand({
        id: 'cmd_review_fail',
        execution_phase: 'terminal',
        terminal_status: 'failed',
        status: 'failed',
        error: 'Usage limit exceeded.',
        result: { status: 'succeeded', outbound_text: 'Die Aufgabe ist erledigt.' },
      });
      window.chatHarness.replaceQueueTasks([]);
      window.chatHarness.emitTracking();
      await window.chatHarness.waitFor(() => document.querySelector('.ctox-chat-window.is-active')?.classList.contains('is-task-failed'), 4000);
      await window.chatHarness.waitForPaint();
      const textarea = document.querySelector('.ctox-chat-window.is-active textarea[name="message"]');
      return {
        ...window.chatHarness.collect(),
        sameTextarea: textarea === window.__chatTextarea,
        selectionStart: textarea?.selectionStart,
        selectionEnd: textarea?.selectionEnd,
        scrollTop: document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages')?.scrollTop || 0,
        nestedSuccess: (document.body.innerText || '').includes('Die Aufgabe ist erledigt.'),
        failureCount: (document.querySelector('.ctox-chat-window.is-active')?.innerText.match(/nicht abschließen/g) || []).length,
      };
    });
    results.push({ scenario: 'terminal-failure-command-only', metrics: afterCommand });
    expect(afterCommand.activeTaskClass.includes('is-task-failed'), `terminal failed command must render failed, got ${afterCommand.activeTaskClass}`);
    expect(!afterCommand.activeTaskClass.includes('is-task-review'), 'failed chat must drop active review window styling');
    expect(afterCommand.progressReviewing === false, 'failed chat must drop active review progress styling');
    expect(afterCommand.progressPercent === '90', `failed chat must keep last percent, got ${afterCommand.progressPercent}`);
    expect(!afterCommand.nestedSuccess, 'nested succeeded envelope must not become a reply');
    expect(afterCommand.failureCount === 1, `failure must be delivered once, got ${afterCommand.failureCount}`);
    expect(afterCommand.sameTextarea === true, 'composer textarea identity must survive the command-only failure');
    expect(afterCommand.composerValue === 'Bitte nicht verlieren', `draft must survive command-only failure, got ${afterCommand.composerValue}`);
    expect(afterCommand.inspectionOpen === true, 'inspector fold must survive command-only failure');
    expect(afterCommand.maximized === true, 'maximized state must survive command-only failure');
    expect(afterCommand.scrollTop === 12, `history scroll must survive command-only failure, got ${afterCommand.scrollTop}`);

    const afterTask = await page.evaluate(async () => {
      window.chatHarness.upsertCommand({
        id: 'cmd_review_fail',
        task_id: 'task_review_fail',
        execution_phase: 'terminal',
        terminal_status: 'failed',
        status: 'failed',
        error: 'Usage limit exceeded.',
        result: { status: 'succeeded', outbound_text: 'Die Aufgabe ist erledigt.' },
      });
      window.chatHarness.upsertQueueTask({
        id: 'task_review_fail',
        command_id: 'cmd_review_fail',
        status: 'failed',
        status_note: 'Usage limit exceeded.',
        crew_member_id: 'member_0',
      });
      window.chatHarness.emitTracking();
      await window.chatHarness.waitFor(() => Boolean(document.querySelector('.ctox-chat-window.is-active [data-task-id="task_review_fail"]')), 4000);
      await window.chatHarness.waitForPaint();
      const textarea = document.querySelector('.ctox-chat-window.is-active textarea[name="message"]');
      return {
        ...window.chatHarness.collect(),
        sameTextarea: textarea === window.__chatTextarea,
        selectionStart: textarea?.selectionStart,
        focused: document.activeElement === textarea,
        nestedSuccess: (document.body.innerText || '').includes('Die Aufgabe ist erledigt.'),
        failureCount: ((document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages')?.textContent.match(/nicht abschließen/g) || []).length)
          + ((document.querySelector('.ctox-chat-window.is-active .ctox-chat-inspection')?.textContent.match(/nicht abschließen/g) || []).length),
      };
    });
    results.push({ scenario: 'terminal-failure-delayed-task', metrics: afterTask });
    expect(afterTask.activeTaskClass.includes('is-task-failed'), `delayed failed task must keep failed state, got ${afterTask.activeTaskClass}`);
    expect(afterTask.takeoverCount === 1, `historical takeover must remain unique after delayed member, got ${afterTask.takeoverCount}`);
    expect(!afterTask.nestedSuccess, 'delayed task must not publish nested succeeded text');
    expect(afterTask.progressReviewing === false, 'delayed failed task must not revive review styling');
    expect(afterTask.progressPercent === '90', `delayed failed task must keep 90 percent, got ${afterTask.progressPercent}`);
    expect(afterTask.sameTextarea === true, 'composer textarea identity must survive delayed task evidence');
    expect(afterTask.composerValue === 'Bitte nicht verlieren', `draft must survive delayed task evidence, got ${afterTask.composerValue}`);
    expect(afterTask.inspectionOpen === true, 'inspector fold must survive delayed task evidence');
    expect(afterTask.maximized === true, 'maximized state must survive delayed task evidence');
    expect((afterTask.activeMessageText + afterTask.activeInspectionText).includes('Usage limit exceeded'), 'delayed failed task note must appear');

    const restored = await page.evaluate(async () => {
      const restore = document.querySelector('.ctox-chat-window.is-active [data-chat-maximize]');
      restore?.click();
      await window.chatHarness.waitFor(() => !document.querySelector('.ctox-chat-window.is-active')?.classList.contains('is-maximized'));
      await window.chatHarness.waitForPaint();
      const textarea = document.querySelector('.ctox-chat-window.is-active textarea[name="message"]');
      return {
        ...window.chatHarness.collect(),
        sameTextarea: textarea === window.__chatTextarea,
      };
    });
    results.push({ scenario: 'terminal-failure-restore', metrics: restored });
    expect(restored.maximized === false, 'Restore must leave the maximized state');
    expect(restored.sameTextarea === true, 'Restore must keep the same composer textarea');
    expect(restored.composerValue === 'Bitte nicht verlieren', `Restore must keep the draft, got ${restored.composerValue}`);
    expect(restored.inspectionOpen === true, 'Restore must keep the inspector fold');
    expect(restored.progressPercent === '90', `Restore must keep last progress percent, got ${restored.progressPercent}`);
    expect(restored.progressReviewing === false, 'Restore must not revive review styling');
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
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify({ ok: true, reportPath, screenshotPath, scenarios: results.length }, null, 2));
  }
} catch (error) {
  failures.push(error?.stack || String(error));
  writeReport();
  throw error;
} finally {
  await browser.close().catch(() => {});
  await new Promise((resolve) => server.close(resolve));
}

async function scenario(page, name, seedOptions, assertions) {
  try {
    await page.goto(url, { waitUntil: 'load' });
    // Page load, not an assertion: a busy host may need seconds to serve the module.
    await page.waitForFunction(() => window.chatHarness?.seed, null, { timeout: 20000 });
    await page.evaluate(async (options) => {
      await window.chatHarness.seed(options);
    }, seedOptions);
    const metrics = await page.evaluate(() => window.chatHarness.collect());
    results.push({ scenario: name, metrics });
    const failuresBefore = failures.length;
    await assertions(metrics);
    if (failures.length === failuresBefore) results.push({ scenario: `${name}:pass` });
    else await captureScenarioFailure(page, name, failures.slice(failuresBefore).join('\n'));
  } catch (error) {
    await captureScenarioFailure(page, name, error);
    throw error;
  }
}

async function captureScenarioFailure(page, name, error) {
  const failureScreenshotPath = path.join(outputDir, `${name}-failure.png`);
  const visibleText = await page.locator('body').innerText({ timeout: 3000 }).catch((readError) => String(readError));
  const screenshotError = await page.screenshot({ path: failureScreenshotPath, timeout: 5000 })
    .then(() => null, (captureError) => String(captureError));
  results.push({ scenario: `${name}:failure`, error: error?.stack || String(error), visibleText, failureScreenshotPath, screenshotError });
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
    groupedScreenshotPath,
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
      --background: #0a0a0a;
      --bg: #0a0a0a;
      --surface: #111111;
      --surface-2: #141414;
      --line: #2a2a2a;
      --text: #d4d4d8;
      --text-strong: #f5f5f5;
      --muted: #818181;
      --accent: #346bf1;
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
    import { createEventBus } from '/src/apps/business-os/shared/event-bus.js';

    const CHAT_STATE_KEY = 'ctox.businessOs.chat.v1';
    const owner = 'test-user';

    let chatCollectionSubscribers = new Set();
    window.chatHarness = { seed, collect, waitFor, waitForPaint, emitChats };

    async function seed(options = {}) {
      const oldRoot = document.querySelector('[data-ctox-chat-root]');
      oldRoot?.__ctoxChatCleanup?.();
      if (window._ctoxChatSchedulerInterval) {
        clearInterval(window._ctoxChatSchedulerInterval);
        window._ctoxChatSchedulerInterval = null;
      }
      document.body.innerHTML = '<main class="harness-app">' + ['Tickets', 'Conversations', 'Notizen', 'Documents', 'Knowledge', 'Kunden', 'App Store', 'Source Editor'].map((name) => '<div class="harness-module">' + name + '</div>').join('') + '</main>';
      if (options.appPresence) {
        // Shell stand-ins: a v2 window per app plus the desktop icon grid,
        // shaped like window-manager.js / app.js render them.
        document.body.insertAdjacentHTML('beforeend', ['documents', 'ctox'].map((id) => '<div class="shell-window" data-owner-id="module:' + id + '" style="position:absolute;top:120px;left:' + (id === 'ctox' ? 700 : 300) + 'px;width:320px;height:200px;border:1px solid #333"><div class="shell-window-v2-icon" style="position:absolute;top:0;left:0;width:44px;height:44px;overflow:hidden;background:#222"><span data-window-app-label>' + id[0].toUpperCase() + '</span></div></div>').join('')
          + '<div data-desktop-icons style="position:absolute;top:400px;left:900px;display:flex;gap:24px">' + ['documents', 'ctox'].map((id) => '<button class="desktop-icon" data-target="' + id + '"><span class="desktop-icon-glyph" style="display:block;width:56px;height:56px;border-radius:14px;background:#2a2a2a"></span><span class="desktop-icon-label">' + id + '</span></button>').join('') + '</div>');
      }
      localStorage.clear();
      sessionStorage.clear();
      chatCollectionSubscribers = new Set();
      window.chatHarness.lastCommand = null;
      const eventBus = createEventBus();
      const on = eventBus.on;
      const off = eventBus.off;
      const windowTokens = new Set();
      const busStats = { windowOpenedObservers: 0 };
      eventBus.on = (event, callback) => {
        const token = on(event, callback);
        if (event === 'window:opened') windowTokens.add(token);
        busStats.windowOpenedObservers = windowTokens.size;
        return token;
      };
      eventBus.off = (event, token) => {
        off(event, token);
        windowTokens.delete(token);
        busStats.windowOpenedObservers = windowTokens.size;
      };
      window.CTOX_BUSINESS_OS_APP = { eventBus };
      window.chatHarness.eventBusStats = busStats;

      const selectedDate = localDateString(addDays(new Date(), options.selectedOffset || 0));
      const chats = Array.from({ length: options.count || 0 }, (_, index) => makeChat({ index, selectedDate, options }));
      if (options.oldOpenOtherDate) {
        const oldDate = localDateString(addDays(new Date(), (options.selectedOffset || 0) - 1));
        const oldChat = makeChat({ index: 1000, selectedDate: oldDate, options });
        oldChat.id = 'chat_old_other_date';
        oldChat.title = 'Old CTOX';
        oldChat.minimized = false;
        chats.push(oldChat);
      }
      const activeIndex = Math.min(Math.max(options.activeIndex || 0, 0), Math.max(chats.length - 1, 0));
      if (chats.length) chats[activeIndex].minimized = false;
      localStorage.setItem(CHAT_STATE_KEY, JSON.stringify({
        selectedDate,
        activeChatId: options.remoteOnly ? '' : options.activeChatId || chats[activeIndex]?.id || '',
        dockCollapsed: Boolean(options.dockCollapsed),
        preCollapseExpandedChatIds: Array.isArray(options.preCollapseExpandedChatIds) ? options.preCollapseExpandedChatIds : [],
        chats: options.remoteOnly ? [] : chats,
      }));
      // Progress fixtures represent a real, persisted queue task. Otherwise the
      // normal tracking refresh correctly treats these 06:00 messages as orphaned
      // and clears progress because neither command nor task exists in the DB.
      const queueTasks = Array.isArray(options.queueTasks) ? options.queueTasks
        : options.progressTracking ? chats.flatMap(chat => chat.messages
          .filter(message => message.taskId && message.executionProgress)
          .map(message => ({ id: message.taskId, command_id: message.commandId,
            status: message.status, execution_progress: message.executionProgress,
            ...(options.crewMembers ? { crew_member_id: 'member_0' } : {}) }))) : [];
      const commandDocs = Array.isArray(options.commandDocs) ? options.commandDocs
        : options.reviewFailureReconciliation ? [{
            id: 'cmd_review_fail',
            execution_phase: 'running',
            terminal_status: 'none',
            status: 'running',
          }] : [];
      const initStarted = performance.now();
      initBusinessChat({

        session: { authenticated: true, user: { id: owner, name: 'Harness User' } },
        commandBus: makeCommandBus(options),
        sync: makeReadiness(),
        db: makeDb(chats, options.dbDelay || 0, Boolean(options.dbTransientError), Boolean(options.dbDeleteError), Number(options.crewMembers) || 0, queueTasks, Boolean(options.holdChatReads), commandDocs),
        getActiveModule: () => ({ id: 'ctox', name: 'CTOX' }),
      });
      await waitFor(() => document.querySelector('[data-chat-dock]'));
      await waitForPaint();
      window.chatHarness.initialPaintMs = performance.now() - initStarted;
      return collect();
    }

    function makeChat({ index, selectedDate, options }) {
      const createdAt = dateTimestamp(selectedDate, index);
      const modules = ['ctox', 'documents', 'knowledge', 'research', 'matching', 'reports', 'conversations', 'outbound'];
      const messages = [];
      const groupedResearch = Boolean(options.groupedResearch);
      const moduleName = groupedResearch ? 'research' : modules[index % modules.length];
      for (let i = 0; i < (options.messagesPerChat || 0); i += 1) {
        messages.push({
          id: 'msg_' + index + '_' + i,
          role: i % 2 ? 'ctox' : 'user',
          text: options.longMessages ? 'Dies ist eine laengere Testnachricht fuer Scroll-Verhalten im aktiven Chat. '.repeat(3) + i : 'Testnachricht ' + i,
          createdAt: createdAt + i * 1000,
        });
      }
      const failedChat = Boolean(options.failedChat) && index === Number(options.activeIndex || 0);
      const reviewFail = Boolean(options.reviewFailureReconciliation) && index === Number(options.activeIndex || 0);
      const trackingId = groupedResearch
        ? 'task_research_' + index
        : reviewFail
          ? 'cmd_review_fail'
          : failedChat
            ? 'task_failed_' + index
            : '';
      if (groupedResearch) {
        messages.push({ id: 'request_' + index, role: 'user', text: 'Bitte recherchiere Auftrag ' + (index + 1), createdAt });
        const statuses = ['running', 'queued', 'success', 'success', 'success', 'failed'];
        const trackingMessage = {
          id: 'status_research_' + index,
          role: 'ctox',
          text: 'Web Research Schritt ' + (index + 1) + ' verarbeitet.',
          taskId: trackingId,
          commandId: trackingId,
          status: statuses[index % statuses.length],
          trackable: options.staticTracking ? false : undefined,
          createdAt: createdAt + 500,
        };
        if (options.progressTracking && index === Number(options.activeIndex || 0)) {
          trackingMessage.executionProgress = {
            version: 1,
            revision: 2,
            phase: 'working',
            percent: 30,
            current_step: 2,
            completed_steps: 1,
            total_steps: 3,
            steps: [
              { position: 1, label: 'Daten laden', status: 'completed', activity_turns: 2 },
              { position: 2, label: 'Daten prüfen', status: 'in_progress', activity_turns: 4 },
              { position: 3, label: 'Ergebnis schreiben', status: 'pending', activity_turns: 0 },
            ],
            review: { status: 'pending' },
            activity_turns: { total: 7, thinking: 3, tools: 4, last_kind: 'tool' },
            updated_at_ms: createdAt + 500,
          };
        }
        messages.push(trackingMessage);
      } else if (reviewFail) {
        messages.push({ id: 'request_review_fail', role: 'user', text: 'Bitte prüfe die Rechnung.', createdAt });
        messages.push({
          id: 'status_review_fail',
          role: 'ctox',
          text: 'Die Prüfung läuft.',
          commandId: 'cmd_review_fail',
          status: 'running',
          executionProgress: {
            version: 1,
            revision: 3,
            phase: 'review',
            percent: 90,
            current_step: 3,
            completed_steps: 2,
            total_steps: 3,
            steps: [
              { position: 1, label: 'Lesen', status: 'completed', activity_turns: 2 },
              { position: 2, label: 'Prüfen', status: 'completed', activity_turns: 3 },
              { position: 3, label: 'Antworten', status: 'completed', activity_turns: 1 },
            ],
            review: { status: 'in_progress' },
            activity_turns: { total: 8, thinking: 5, tools: 3, last_kind: 'thinking' },
            updated_at_ms: createdAt + 500,
          },
          createdAt: createdAt + 500,
        });
        messages.push({
          id: 'takeover_review_fail',
          role: 'ctox',
          text: 'Pico übernimmt.',
          takeoverFor: 'cmd_review_fail',
          commandId: 'cmd_review_fail',
          crewMemberId: 'member_0',
          status: 'running',
          createdAt: createdAt + 600,
        });
      } else if (failedChat) {
        messages.push({
          id: 'status_failed_' + index,
          role: 'ctox',
          text: 'CTOX konnte die Aufgabe nicht ausführen.',
          taskId: trackingId,
          commandId: trackingId,
          status: 'failed',
          createdAt: createdAt + 500,
        });
      }
      return {
        id: 'chat_' + index,
        title: groupedResearch ? 'Web Research ' + (index + 1) : index % 3 === 0 ? 'Documents be...' : 'CTOX',
        open: true,
        minimized: false,
        maximized: false,
        owner_user_id: owner,
        lastTrackingId: trackingId,
        messages,
        draft: reviewFail ? 'Bitte nicht verlieren' : '',
        inspectionOpen: Boolean(reviewFail),
        contextMeta: groupedResearch
          ? {
              module: moduleName,
              source_module: 'web_research',
              source_title: 'Web Research Wettbewerberanalyse',
              command_type: 'research.web',
              record_id: 'research_case_42',
              thread_key: 'research/web/wettbewerberanalyse',
              group_key: 'research:wettbewerberanalyse',
            }
          : failedChat
            ? {
                module: moduleName,
                instruction: 'Ursprüngliche Instruktion',
                prompt: 'Ursprünglicher Prompt',
                payload: {
                  instruction: 'Ursprüngliche Payload-Instruktion',
                  prompt: 'Ursprünglicher Payload-Prompt',
                },
              }
            : { module: moduleName },
        createdAt,
        updated_at_ms: createdAt,
        showFollowUp: false,
        attachments: [],
      };
    }

    async function emitChats() {
      for (const callback of Array.from(chatCollectionSubscribers)) callback?.();
      await new Promise((resolve) => setTimeout(resolve, 40));
      await waitForPaint();
    }

    function makeReadiness() {
      const callbacks = new Set();
      const stats = { subscriptions: 0 };
      window.chatHarness.readinessStats = stats;
      window.chatHarness.emitReadiness = () => { for (const token of [...callbacks]) token.callback({ state: 'ready' }); };
      return {
        subscribeCollectionReadiness: (_collection, callback) => {
          const token = { callback };
          callbacks.add(token);
          stats.subscriptions = callbacks.size;
          callback({ state: 'ready' });
          return () => { callbacks.delete(token); stats.subscriptions = callbacks.size; };
        },
      };
    }

    function makeCommandBus(options) {
      return {
        dispatch: async (command) => {
          window.chatHarness.lastCommand = structuredClone(command);
          if (options.commandError === 'transient') {
            throw new Error('Timed out waiting for WebRTC response rxdb.query.fetch');
          }
          return { task_id: 'task_harness', command_id: 'cmd_harness', status: 'queued' };
        },
      };
    }

    function makeCrewMembers(count) {
      const shapes = ['round', 'tall', 'wide', 'blob'];
      const colors = ['#e0a458', '#5aa9e6', '#8fbf7f', '#c77dff'];
      return Array.from({ length: count }, (_, index) => ({
        id: 'member_' + index,
        name: ['Pico', 'Nia', 'Odo', 'Lumi'][index % 4] + (index >= 4 ? ' ' + index : ''),
        shape: shapes[index % shapes.length],
        color: colors[index % colors.length],
        state: index === 0 ? 'on_duty' : 'home',
        domain: [],
        archived: false,
        updated_at_ms: Date.now(),
      }));
    }

    function makeDb(chats, delayMs, transientError, deleteError, crewMemberCount = 0, queueTasks = [], holdChatReads = false, commandDocs = []) {
      const store = new Map(chats.map((chat) => [chat.id, structuredClone(chat)]));
      const readStats = { started: 0, completed: 0, crewReads: 0, crewSubscriptions: 0 };
      const chatReadGate = new Promise((resolve) => {
        window.chatHarness.releaseChatReads = resolve;
        if (!holdChatReads) resolve();
      });
      const crewSubscribers = new Set();
      window.chatHarness.emitCrew = () => { for (const callback of [...crewSubscribers]) callback(); };
      window.chatHarness.chatReadStats = readStats;
      window.chatHarness.publishMessage = async (chatId, message) => {
        const chat = store.get(chatId);
        chat.messages.push(message);
        chat.updated_at_ms = Date.now();
        await emitChats();
      };
      const crewMembers = makeCrewMembers(crewMemberCount);
      const commands = commandDocs.map((doc) => structuredClone(doc));
      const tasks = queueTasks.map((doc) => structuredClone(doc));
      const commandSubscribers = new Set();
      const queueSubscribers = new Set();
      const upsertRow = (rows, doc) => {
        const index = rows.findIndex((row) => row.id === doc.id);
        if (index >= 0) rows[index] = structuredClone(doc);
        else rows.push(structuredClone(doc));
      };
      const docsMatching = (rows, query = {}) => {
        const ids = query?.selector?.id?.$in;
        const commandIds = query?.selector?.command_id?.$in;
        if (Array.isArray(ids) && ids.length) return rows.filter((row) => ids.map(String).includes(String(row.id)));
        if (Array.isArray(commandIds) && commandIds.length) {
          return rows.filter((row) => commandIds.map(String).includes(String(row.command_id || row.commandId || '')));
        }
        return rows;
      };
      window.chatHarness.upsertCommand = (doc) => upsertRow(commands, doc);
      window.chatHarness.upsertQueueTask = (doc) => upsertRow(tasks, doc);
      window.chatHarness.replaceQueueTasks = (docs) => {
        tasks.splice(0, tasks.length, ...docs.map((doc) => structuredClone(doc)));
      };
      window.chatHarness.emitTracking = () => {
        for (const callback of [...commandSubscribers]) callback({ documents: [] });
        for (const callback of [...queueSubscribers]) callback({ documents: [] });
      };
      const delay = () => new Promise((resolve) => setTimeout(resolve, delayMs));
      const maybeThrow = async () => {
        await delay();
        if (transientError) throw new Error('Timed out waiting for WebRTC response rxdb.query.fetch');
      };
      const docFor = (id) => {
        const value = store.get(id);
        if (!value) return null;
        return {
          toJSON: () => structuredClone(value),
          incrementalPatch: async (doc) => { await maybeThrow(); store.set(id, structuredClone({ ...value, ...doc })); },
          remove: async () => {
            await maybeThrow();
            if (deleteError) throw new Error('CTOX_BUSINESS_OS_PERMISSION_DENIED');
            store.delete(id);
          },
        };
      };
      return {
        raw: {
          business_chats: {
            $: {
              subscribe: (callback) => {
                chatCollectionSubscribers.add(callback);
                return { unsubscribe: () => chatCollectionSubscribers.delete(callback) };
              },
            },
            find: () => ({ exec: async () => { readStats.started += 1; await chatReadGate; await maybeThrow(); readStats.completed += 1; return Array.from(store.keys()).map(docFor).filter(Boolean); } }),
            findOne: (id) => ({ exec: async () => { await maybeThrow(); return docFor(id); } }),
            insert: async (doc) => { await maybeThrow(); store.set(doc.id, structuredClone(doc)); return docFor(doc.id); },
          },
          business_commands: {
            $: {
              subscribe: (callback) => {
                commandSubscribers.add(callback);
                return { unsubscribe: () => commandSubscribers.delete(callback) };
              },
            },
            find: (query) => ({ exec: async () => { await maybeThrow(); return docsMatching(commands, query).map((row) => ({ toJSON: () => structuredClone(row) })); } }),
            findOne: (id) => ({ exec: async () => {
              await maybeThrow();
              const row = commands.find((item) => item.id === id);
              return row ? { toJSON: () => structuredClone(row) } : null;
            } }),
          },
          ctox_queue_tasks: {
            $: {
              subscribe: (callback) => {
                queueSubscribers.add(callback);
                return { unsubscribe: () => queueSubscribers.delete(callback) };
              },
            },
            find: (query) => ({ exec: async () => { await maybeThrow(); return docsMatching(tasks, query).map((row) => ({ toJSON: () => structuredClone(row) })); } }),
            findOne: (id) => ({ exec: async () => {
              await maybeThrow();
              const row = tasks.find((item) => item.id === id);
              return row ? { toJSON: () => structuredClone(row) } : null;
            } }),
          },
          ctox_crew_members: {
            $: { subscribe: (callback) => {
              crewSubscribers.add(callback);
              readStats.crewSubscriptions = crewSubscribers.size;
              return { unsubscribe: () => { crewSubscribers.delete(callback); readStats.crewSubscriptions = crewSubscribers.size; } };
            } },
            find: () => ({ exec: async () => { readStats.crewReads += 1; await maybeThrow(); return crewMembers.map((member) => ({ toJSON: () => structuredClone(member) })); } }),
          },
        },
      };
    }

    function collect() {
      const root = document.querySelector('[data-ctox-chat-root]');
      const windowRects = Array.from(document.querySelectorAll('.ctox-chat-window'))
        .map((el) => el.getBoundingClientRect())
        .sort((a, b) => a.left - b.left);
      const dock = document.querySelector('[data-chat-dock]');
      const strip = document.querySelector('[data-chat-strip]');
      const activeWindow = document.querySelector('.ctox-chat-window.is-active');
      const stored = JSON.parse(localStorage.getItem(CHAT_STATE_KEY) || '{}');
      const deletedChatIds = stored.deletedChatIds && typeof stored.deletedChatIds === 'object' && !Array.isArray(stored.deletedChatIds)
        ? stored.deletedChatIds
        : {};
      const activeMessages = document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages');
      const stageInner = document.querySelector('.ctox-chat-stage-inner');
      const firstInactiveWindow = document.querySelector('.ctox-chat-window:not(.is-active)');
      const inactiveActions = Array.from(document.querySelectorAll('.ctox-chat-window:not(.is-active) .ctox-chat-header-actions'));
      const inactiveControls = Array.from(document.querySelectorAll('.ctox-chat-window:not(.is-active) button, .ctox-chat-window:not(.is-active) input, .ctox-chat-window:not(.is-active) textarea, .ctox-chat-window:not(.is-active) select, .ctox-chat-window:not(.is-active) a'));
      const dockRect = box(dock);
      const activeChip = Array.from(document.querySelectorAll('[data-chat-focus]'))
        .find((node) => node.dataset.chatFocus === activeWindow?.dataset.chatId);
      const activeWindowRect = box(activeWindow);
      const activeChipRect = box(activeChip);
      const progressTrackRect = box(document.querySelector('.ctox-chat-window.is-active header .ctox-progress-track'));
      const progressTrack = document.querySelector('.ctox-chat-window.is-active header .ctox-progress-track');
      const progressClock = document.querySelector('.ctox-chat-window.is-active header .ctox-progress-activity');
      const activeHeaderRect = box(document.querySelector('.ctox-chat-window.is-active header'));
      const activeChipCenterX = activeChipRect.width ? activeChipRect.x + activeChipRect.width / 2 : 0;
      const activeWindowCenterX = activeWindowRect.width ? activeWindowRect.x + activeWindowRect.width / 2 : 0;
      return {
        viewportWidth: window.innerWidth,
        rootWidth: box(root).width,
        dockWidth: dockRect.width,
        dockLeft: dockRect.x,
        dockRight: dockRect.x + dockRect.width,
        dockRatio: dockRect.width / window.innerWidth,
        dockClasses: dock?.className || '',
        stripClasses: strip?.className || '',
        activeWindowLeft: activeWindowRect.x,
        activeWindowRight: activeWindowRect.x + activeWindowRect.width,
        activeChipLeft: activeChipRect.x,
        activeChipRight: activeChipRect.x + activeChipRect.width,
        activeChipWindowCenterDelta: activeChipCenterX && activeWindowCenterX ? Math.abs(activeChipCenterX - activeWindowCenterX) : 0,
        activeChipCenterWithinWindow: activeChipCenterX && activeWindowRect.width
          ? activeChipCenterX >= activeWindowRect.x && activeChipCenterX <= activeWindowRect.x + activeWindowRect.width
          : false,
        activeWindowDockOverflow: activeWindowRect.width && dockRect.width
          ? Math.max(0, dockRect.x - activeWindowRect.x, activeWindowRect.x + activeWindowRect.width - (dockRect.x + dockRect.width))
          : 0,
        dateScopeText: document.querySelector('.ctox-date-scope')?.textContent || '',
        dockLabel: (document.querySelector('.ctox-chat-fab span')?.textContent || '').trim(),
        dateTriggerLabel: document.querySelector('.ctox-date-picker-trigger')?.getAttribute('aria-label') || '',
        stripCount: document.querySelectorAll('[data-chat-strip]').length,
        navCount: document.querySelectorAll('[data-chat-prev], [data-chat-next]').length,
        dockNewCount: document.querySelectorAll('[data-chat-dock] > [data-chat-new]').length,
        headerNewCount: document.querySelectorAll('.ctox-chat-window [data-chat-new]').length,
        overflowCount: document.querySelectorAll('[data-chat-overflow-open]').length,
        busyPanelCount: document.querySelectorAll('[data-chat-busy-panel]').length,
        busyRowCount: document.querySelectorAll('.ctox-chat-busy-row[data-chat-list-focus]').length,
        busyFocusTargetCount: document.querySelectorAll('[data-chat-list-focus]').length,
        busyGroupCount: document.querySelectorAll('[data-chat-busy-group]').length,
        groupFilterValue: document.querySelector('[data-chat-list-filter="group"]')?.value || '',
        busyGroupFirstLabel: document.querySelector('.ctox-chat-busy-group-head strong')?.textContent || '',
        busyGroupMoreText: document.querySelector('.ctox-chat-busy-group-more')?.textContent || '',
        busyMoreText: document.querySelector('.ctox-chat-busy-more')?.textContent || '',
        workloadBadgeText: document.querySelector('.ctox-date-workload-badge')?.textContent || '',
        datePanelCount: document.querySelectorAll('[data-chat-date-workload-panel]').length,
        heatmapDayCount: document.querySelectorAll('[data-chat-date-select]').length,
        selectedHeatmapIntensity: document.querySelector('.ctox-date-heatmap-day.is-selected')?.dataset.intensity || '',
        datePanelTaskText: document.querySelector('[data-chat-date-workload-panel] header span')?.textContent || '',
        chipCount: document.querySelectorAll('[data-chat-focus]').length,
        windowCount: document.querySelectorAll('.ctox-chat-window').length,
        windowRects: windowRects.map((r) => Math.round(r.left) + '+' + Math.round(r.width)),
        windowMinLeft: windowRects.length ? Math.round(Math.min(...windowRects.map((r) => r.left))) : 0,
        windowMaxRight: windowRects.length ? Math.round(Math.max(...windowRects.map((r) => r.right))) : 0,
        stageLeft: Math.round(stageInner?.getBoundingClientRect().left || 0),
        stageRight: Math.round(stageInner?.getBoundingClientRect().right || 0),
        windowOverlap: windowRects.slice(1).reduce((worst, rect, index) => Math.max(worst, Math.round(windowRects[index].right - rect.left)), 0),
        stripWidth: Math.round(strip?.getBoundingClientRect().width || 0),
        stripRight: Math.round(strip?.getBoundingClientRect().right || 0),
        fabMemberCount: document.querySelectorAll('.ctox-chat-fab-creatures.is-members .ctox-chat-crew-slot').length,
        appPresence: Array.from(document.querySelectorAll('.shell-window-v2-icon, .desktop-icon-glyph')).map((host) => ({
          host: host.classList.contains('desktop-icon-glyph') ? 'desktop:' + host.closest('.desktop-icon')?.dataset.target : 'window:' + host.closest('.shell-window')?.dataset.ownerId,
          creatures: host.querySelectorAll('[data-crew-presence] .ctox-crew-creature').length,
          title: host.querySelector('[data-crew-presence]')?.getAttribute('title') || '',
          inside: (() => { const badge = host.querySelector('[data-crew-presence]'); if (!badge) return true; const a = host.getBoundingClientRect(); const b = badge.getBoundingClientRect(); return b.left >= a.left - 0.5 && b.right <= a.right + 0.5 && b.top >= a.top - 0.5 && b.bottom <= a.bottom + 0.5; })(),
        })),
        windowCreatureCount: document.querySelectorAll('.ctox-chat-window .ctox-crew-creature').length,
        dockCreatureCount: document.querySelectorAll('.ctox-chat-chip .ctox-crew-creature').length,
        progressCardCount: document.querySelectorAll('.ctox-chat-delegation-card .ctox-progress-visual').length,
        progressHeaderText: document.querySelector('.ctox-chat-progress-head')?.textContent || '',
        progressSegmentCount: document.querySelectorAll('.ctox-progress-segment, .ctox-progress-review').length,
        progressCurrentText: document.querySelector('.ctox-progress-current-copy')?.textContent || '',
        progressNextText: document.querySelector('.ctox-progress-next')?.textContent || '',
        progressPlanText: document.querySelector('.ctox-progress-plan summary')?.textContent || '',
        progressTooltip: document.querySelector('.ctox-progress-visual')?.getAttribute('title') || '',
        progressInHeader: Boolean(document.querySelector('.ctox-chat-window header > .ctox-chat-delegation-card')),
        progressClockCount: document.querySelectorAll('.ctox-chat-window header .ctox-progress-activity').length,
        progressTrackWidth: document.querySelector('.ctox-chat-window header .ctox-progress-track')?.getBoundingClientRect().width || 0,
        progressTrackHeight: progressTrackRect.height,
        progressCrewColor: activeWindow ? getComputedStyle(activeWindow).getPropertyValue('--crew-color').trim() : '',
        progressFillBackground: progressTrack ? getComputedStyle(progressTrack, '::before').backgroundColor : 'rgba(0, 0, 0, 0)',
        progressClockBorder: progressClock ? getComputedStyle(progressClock).borderTopColor : 'rgb(0, 0, 0)',
        progressTrackEdgeDelta: activeHeaderRect.width && progressTrackRect.width
          ? Math.max(Math.abs(progressTrackRect.x - activeHeaderRect.x), Math.abs((progressTrackRect.x + progressTrackRect.width) - (activeHeaderRect.x + activeHeaderRect.width)))
          : 0,
        headerHeight: document.querySelector('.ctox-chat-window header')?.getBoundingClientRect().height || 0,
        headerBottom: document.querySelector('.ctox-chat-window header')?.getBoundingClientRect().bottom || 0,
        firstMessageTop: document.querySelector('.ctox-chat-window .ctox-chat-message')?.getBoundingClientRect().top || 0,
        activeWindowHeight: activeWindowRect.height || 0,
        compactPromptCount: document.querySelectorAll('.ctox-chat-prompt').length,
        compactPromptOpen: Boolean(document.querySelector('.ctox-chat-prompt')?.open),
        compactPromptPreviewHeight: document.querySelector('.ctox-chat-prompt-preview')?.getBoundingClientRect().height || 0,
        composerInline: (() => {
          const clip = document.querySelector('.ctox-chat-window.is-active .ctox-chat-clip-btn')?.getBoundingClientRect();
          const input = document.querySelector('.ctox-chat-window.is-active textarea')?.getBoundingClientRect();
          const send = document.querySelector('.ctox-chat-window.is-active [data-chat-send]')?.getBoundingClientRect();
          if (!clip || !input || !send) return false;
          const centers = [clip, input, send].map((rect) => rect.top + rect.height / 2);
          return clip.right <= input.left && input.right <= send.left && Math.max(...centers) - Math.min(...centers) <= 1;
        })(),
        visibleChromeText: [
          document.querySelector('.ctox-chat-window header')?.innerText || '',
          document.querySelector('.ctox-chat-delegation-card')?.innerText || '',
        ].join('').trim(),
        stageClasses: stageInner?.className || '',
        inactiveVisible: firstInactiveWindow ? isVisible(firstInactiveWindow) : false,
        inactiveTransform: firstInactiveWindow ? getComputedStyle(firstInactiveWindow).transform : 'none',
        activeId: activeWindow?.dataset.chatId || '',
        renderedWindowIds: Array.from(document.querySelectorAll('.ctox-chat-window')).map((node) => node.dataset.chatId || ''),
        stripClientWidth: strip?.clientWidth || 0,
        stripScrollWidth: strip?.scrollWidth || 0,
        stripHasOverflow: strip ? strip.scrollWidth > strip.clientWidth + 1 : false,
        minimizedChipCount: document.querySelectorAll('.ctox-chat-chip.is-minimized').length,
        minimizedChipIds: Array.from(document.querySelectorAll('.ctox-chat-chip.is-minimized')).map((node) => node.dataset.chatFocus || ''),
        minimizedRunningChipCount: document.querySelectorAll('.ctox-chat-chip.is-minimized.is-task-running').length,
        minimizedRunningStatusText: document.querySelector('.ctox-chat-chip.is-minimized.is-task-running .ctox-chat-chip-copy small')?.textContent || '',
        minimizedRunningTitle: document.querySelector('.ctox-chat-chip.is-minimized.is-task-running')?.getAttribute('title') || '',
        minimizedWindowCount: document.querySelectorAll('.ctox-chat-window.is-minimized').length,
        inactiveFocusable: inactiveControls.filter((node) => node.tabIndex >= 0 && isVisible(node)).length,
        inactiveVisibleActions: inactiveActions.filter(isVisible).length,
        messagesClientHeight: activeMessages?.clientHeight || 0,
        messagesScrollHeight: activeMessages?.scrollHeight || 0,
        activeTextareaCount: document.querySelectorAll('.ctox-chat-window.is-active textarea').length,
        activeTaskClass: activeWindow?.className || '',
        activeStatusText: document.querySelector('.ctox-chat-window.is-active .ctox-chat-status-badge')?.textContent?.trim() || '',
        activeMessageText: document.querySelector('.ctox-chat-window.is-active .ctox-chat-messages')?.textContent?.trim() || '',
        activeInspectionText: document.querySelector('.ctox-chat-window.is-active .ctox-chat-inspection')?.textContent?.trim() || '',
        progressReviewing: Boolean(document.querySelector('.ctox-chat-window.is-active .ctox-progress-visual.is-reviewing')),
        progressPercent: document.querySelector('.ctox-chat-window.is-active .ctox-progress-track[aria-valuenow]')?.getAttribute('aria-valuenow') || '',
        takeoverCount: (document.querySelector('.ctox-chat-window.is-active')?.innerText.match(/übernimmt/g) || []).length,
        inspectionOpen: Boolean(document.querySelector('.ctox-chat-window.is-active .ctox-chat-inspection')?.open),
        maximized: Boolean(activeWindow?.classList.contains('is-maximized')),
        composerValue: document.querySelector('.ctox-chat-window.is-active textarea[name="message"]')?.value || '',
        storedChats: Array.isArray(stored.chats) ? stored.chats.length : 0,
        chatReadStats: { ...window.chatHarness.chatReadStats },
        initialPaintMs: window.chatHarness.initialPaintMs,
        deletedChatTombstones: Object.keys(deletedChatIds).length,
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
      const timestamp = new Date(year, month - 1, day, hour, minute, 0, 0).getTime();
      // Today's existing chats must already exist, including before 06:00.
      // A future creation stamp makes submitting a follow-up schedule it.
      // Explicit future dates retain their real scheduling semantics.
      return dateStr === localDateString(new Date())
        ? Math.min(timestamp, Date.now() - 1) : timestamp;
    }
  </script>
</body>
</html>`;
}
