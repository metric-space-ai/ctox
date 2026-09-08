import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const modulePath = resolve(scriptDir, '../shared/business-chat.js');
const source = readFileSync(modulePath, 'utf8');
const failures = [];

const dockRule = source.match(/\.ctox-chat-dock\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const manyChatsDockRule = source.match(/\.ctox-chat-dock\.has-many-chats\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const oneChatStripRule = source.match(/\.ctox-chat-dock\.has-one-chat\s+\.ctox-chat-strip\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const fewChatsStripRule = source.match(/\.ctox-chat-dock\.has-few-chats\s+\.ctox-chat-strip\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const collapsedRootRules = [...source.matchAll(/\.ctox-chat-root\.is-collapsed\s*\{(?<body>[\s\S]*?)\n\s*\}/g)];
const collapsedDockRules = [...source.matchAll(/\.ctox-chat-dock\.is-collapsed\s*\{(?<body>[\s\S]*?)\n\s*\}/g)];
// Anchored at the line start: the reporter-slot rule
// `body:not([data-shell-chat-dock-side]) .ctox-chat-dock:not(.is-collapsed)`
// shares the suffix and must not shadow the geometry rule.
const expandedDockRule = source.match(/\n\s*\.ctox-chat-dock:not\(\.is-collapsed\)\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const expandedVisibleRule = source.match(/\.ctox-chat-dock\.has-visible-chats:not\(\.is-collapsed\)\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';
const finalCollapsedRootRule = collapsedRootRules.at(-1)?.groups?.body || '';
const finalCollapsedDockRule = collapsedDockRules.at(-1)?.groups?.body || '';
const dateNavigationBlock = source.match(/root\.querySelector\('\[data-chat-date-prev\]'[\s\S]*?root\.querySelector\('\[data-chat-new\]'\)/)?.[0] || '';

expect(dockRule, 'Missing .ctox-chat-dock CSS rule');
expectIncludes(
  dockRule,
  'grid-template-columns: 88px var(--ctox-date-pill-width) 34px;',
  'Default chat dock must stay compact for zero visible chats'
);
expectIncludes(dockRule, '--ctox-date-pill-width: 146px;', 'Date history control must have enough width for a visible label');
expectIncludes(dockRule, 'width: max-content;', 'Default chat dock must shrink to its controls');
rejectIncludes(dockRule, 'justify-self: start;', 'Chat dock must not shrink-wrap to its content');
rejectIncludes(dockRule, 'minmax(0, max-content)', 'Chat tab strip must not use content-sized columns');
rejectMatch(dockRule, /(?:^|\n)\s*width:\s*100%;/, 'Default chat dock must not span the shell for zero or one chat');

expect(manyChatsDockRule, 'Missing .ctox-chat-dock.has-many-chats CSS rule');
expectIncludes(
  manyChatsDockRule,
  'grid-template-columns: 88px var(--ctox-date-pill-width) 28px minmax(0, min(420px, 40dvw)) 28px 34px;',
  'Many-chat dock must reserve a bounded scrollable tab strip'
);
expectIncludes(manyChatsDockRule, 'width: max-content;', 'Many-chat dock must remain content-sized');
expectIncludes(manyChatsDockRule, 'max-width: min(860px, calc(100dvw - 132px));', 'Many-chat dock must preserve free desktop docking space');
rejectMatch(manyChatsDockRule, /(?:^|\n)\s*width:\s*100%;/, 'Many-chat dock must never paint to the right edge');
expect(oneChatStripRule, 'Missing one-chat compact strip rule');
expectIncludes(oneChatStripRule, 'width: 148px;', 'One-chat strip must have stable compact width');
expect(fewChatsStripRule, 'Missing few-chat strip rule');
expectIncludes(fewChatsStripRule, 'max-width:', 'Few-chat strip must cap growth before many-chat mode');

expect(finalCollapsedRootRule, 'Missing final collapsed root geometry');
expectIncludes(finalCollapsedRootRule, 'width: max-content;', 'Collapsed Crew root must shrink to its visible controls');
expectIncludes(finalCollapsedRootRule, 'max-width: max-content;', 'Collapsed Crew root must not inherit a viewport-sized maximum');
expect(finalCollapsedDockRule, 'Missing final collapsed dock geometry');
expectIncludes(finalCollapsedDockRule, 'justify-self: start;', 'Collapsed Crew dock must not stretch in its grid');
expectIncludes(finalCollapsedDockRule, 'width: max-content !important;', 'Collapsed Crew dock must override responsive stretching');
expect(expandedDockRule, 'Missing explicit expanded dock geometry');
expectIncludes(expandedDockRule, 'width: max-content;', 'Zero/one-chat Crew dock must remain content-sized');
expect(expandedVisibleRule, 'Missing expanded visible-chat grid');
expectIncludes(source, '.ctox-chat-dock.has-few-chats:not(.is-collapsed),', 'Multi-member Crew dock needs an explicit expanded geometry');
expectIncludes(source, 'justify-self: stretch;', 'Multi-member Crew dock must absorb the available desktop width');

expectIncludes(source, 'const fitsSideBySide =', 'Chat windows need a side-by-side fit check');
expectIncludes(source, 'const MANY_CHAT_THRESHOLD = 12;', 'Many-chat threshold must be explicit');
expectIncludes(source, 'const MAX_RENDERED_CHAT_TABS = 12;', 'Rendered chat tabs must be capped for busy days');
expectIncludes(source, "openChats.length > 1 && openChats.length < MANY_CHAT_THRESHOLD ? 'has-few-chats' : ''", 'Few-chat mode must include mid-size chat counts without full-width dock');
expectIncludes(source, "openChats.length >= MANY_CHAT_THRESHOLD ? 'has-many-chats' : ''", 'Many-chat mode must not activate before high tab counts');
expectIncludes(source, 'function selectVisibleChats(openChats, activeChat)', 'Busy days must not render every chat tab/window');
expectIncludes(source, 'const expandedChats = openChats.filter((chat) => !chat.minimized);', 'Minimized chats must be removed from the rendered window set');
expectIncludes(source, 'const visibleWindowChats = stageWindowChats(expandedChats, activeExpandedChat);', 'All expanded crew windows must reach the shared stage');
expectIncludes(
  source,
  'const windowShapeUnchanged = existingWindows.length === visibleWindowChats.length',
  'In-place updates must compare against expanded rendered windows only'
);
expectIncludes(source, 'visibleWindowChats.map((chat, idx)', 'Crew stage must render the bounded expanded window set');
expectIncludes(
  source,
  '.ctox-chat-stage-inner.is-side-by-side .ctox-chat-window.is-minimized',
  'Side-by-side layout must not override minimized window hiding'
);
expectIncludes(source, 'chatOverflowItem(hiddenChatCount', 'Busy days need an overflow affordance');
expectIncludes(source, 'function updateChatStripOverflowState(root)', 'Scrollable chat strips need explicit overflow state classes');
expectIncludes(source, '.ctox-chat-strip.is-scrollable::-webkit-scrollbar', 'Scrollable chat strips need a visible scrollbar hint');
expectIncludes(source, '.ctox-chat-strip.is-scrollable:not(.is-at-start):not(.is-at-end)', 'Scrollable chat strips need edge overflow shadows');
expectIncludes(source, 'chatDockClassName(chat, activeChat?.id, taskState)', 'Chat chips must use shared state classes during in-place updates');
expectIncludes(source, 'function chatDockStatusText(chat, taskState = getTaskState(chat))', 'Chat chips need status text for hover and accessibility hints');
expectIncludes(source, '`is-task-${taskState}`', 'Chat chips must include task-state classes');
expectIncludes(source, '.ctox-chat-chip.is-minimized:not(.is-task-idle)', 'Minimized non-idle chats must keep visible status styling');
expectIncludes(source, 'function chatDateAriaLabel(dateStr, total = 0)', 'Date history control needs a clear accessible label');
expectIncludes(source, 'title="${escapeAttr(chatDateAriaLabel(selectedDate, workload.total))}"', 'Date control must explain its scope on hover');
expectIncludes(source, 'chatBusyPanel({ chats: openChats, selectedDate, state })', 'Busy days need a filterable list panel');
expectIncludes(source, 'data-chat-list-filter="source"', 'Busy-day list must include source filtering');
expectIncludes(source, 'data-chat-list-filter="group"', 'Busy-day list must include grouping control');
expectIncludes(source, 'function groupBusyChats(chats, mode = \'auto\')', 'Busy-day list must group related task series');
expectIncludes(source, 'function chatSeriesKey(chat)', 'Busy-day grouping must use stable thread/group metadata');
expectIncludes(source, 'function allocateBusyGroupRows(groups)', 'Busy-day grouping must allocate row budget across groups');
expectIncludes(source, 'const MAX_BUSY_GROUPS = 24;', 'Busy-day grouped lists must cap rendered groups');
expectIncludes(source, 'dateWorkloadPanel({ chats: state.chats, selectedDate })', 'Date selection must expose workload heatmap panel');
expectIncludes(source, 'function workloadDaysAround(chats, selectedDate, count)', 'Date workload panel must aggregate nearby days');
expectIncludes(
  source,
  "stageInner.classList.toggle('is-side-by-side', fitsSideBySide);",
  'Chat stage must mark the side-by-side state'
);
expectIncludes(
  source,
  'const layoutFrame = chatWindowStageFrame(root, stageInner, widestWindow);',
  'Crew windows must be aligned against the shared stage frame'
);
expectIncludes(
  source,
  'function chatWindowStageFrame(root, stageInner, minContentWidth = 0)',
  'Crew window layout needs a viewport-safe stage frame helper'
);
expectIncludes(
  source,
  'function clampChatWindowLeft(left, width, frame)',
  'Chat window layout needs a shared frame clamp'
);
expectIncludes(source, 'const carouselStep =', 'Chat windows need progressive carousel overlap');
expectIncludes(
  source,
  '.ctox-chat-stage-inner.is-side-by-side .ctox-chat-window',
  'Side-by-side windows must neutralize carousel transforms'
);
expectIncludes(source, '.ctox-chat-stage {\n      pointer-events: none;\n      grid-row: 1;\n      display: block;', 'Chat stage must span the dock/root instead of a detached grid column');
expectIncludes(source, '.ctox-chat-stage-inner {\n      position: relative;\n      overflow: visible;\n      width: 100%;', 'Chat stage inner must use the full stage for dock-relative alignment');
rejectMatch(
  source,
  /animation:\s*ctoxChatSlideIn[^;]*\bboth\b/,
  'Slide-in animation must not keep fill-mode transforms after layout alignment'
);
expectIncludes(source, 'renderAndPersistChatState', 'Interactive handlers must render before asynchronous persistence');
rejectIncludes(source, "node.querySelector('[data-chat-new]')", 'Chat-window header must not expose a dead/new-chat plus button');
expect(dateNavigationBlock, 'Missing date navigation handler block');
rejectIncludes(dateNavigationBlock, 'ensureChat', 'Date navigation must not create phantom chats');
expectIncludes(source, '.ctox-chat-window:not(.is-active) .ctox-chat-header-actions *,', 'Inactive window header controls must remain directly clickable');
expectIncludes(source, 'pointer-events: auto !important;', 'Inactive window controls must not require an activation click');
expectIncludes(source, 'bottom: -1px;', 'Header progress must run on the window-frame edge');
expectIncludes(source, '.ctox-progress-visual:not(.is-reviewing) .ctox-progress-review.is-pending', 'Review progress must stay dormant until review starts');
expectIncludes(
  source,
  '.ctox-chat-window:not(.is-active) {\n      opacity: 0.6;\n      visibility: visible;\n      pointer-events: auto;',
  'Inactive desktop windows must remain visible and focusable as a 3D gallery'
);
expectIncludes(source, 'function crewCreatureHtml(chat, taskState = getTaskState(chat), placement = \'dock\')', 'Crew members need deterministic SVG identities');
expectIncludes(source, '@keyframes ctoxCrewWork', 'Crew status must have a working animation');
expectIncludes(
  source,
  'setWindowInteractiveState(node, chat.id === activeChat?.id && !chat.minimized);',
  'Inactive window controls must be removed from keyboard tab order'
);
expectIncludes(source, 'class="ctox-date-picker-trigger" role="button" tabindex="0"', 'Visible date trigger must be keyboard focusable');
expectIncludes(source, 'data-chat-date-picker value="${selectedDate}" max="${maxDateVal}" tabindex="-1" aria-hidden="true"', 'Hidden native date input must not enter tab order');
expectIncludes(source, '@media (max-height: 680px)', 'Chat windows need a compact height breakpoint so app windows retain working space');
expectIncludes(source, '@media (max-height: 479px)', 'Very short viewports must fall back to the dock-only chat surface');
expectIncludes(
  source,
  '@media (max-width: 780px)',
  'Mobile chat layout needs a dedicated viewport breakpoint'
);
expectIncludes(
  source,
  '.ctox-chat-window:not(.is-active) {\n        display: none !important;\n        pointer-events: none !important;',
  'Mobile chat must hide inactive windows so old desktop carousel positions cannot block the shell'
);
expectIncludes(source, "const CHAT_LAYOUT_EVENT = 'ctox-business-os-chat-layout';", 'Chat must publish its measured shell layout contract');
expectIncludes(source, 'left: rect.left,', 'Chat layout contract must expose its left edge for side-dock composition');

if (failures.length) {
  console.error(`Business chat layout guard failed:\n${failures.map((failure) => `- ${failure}`).join('\n')}`);
  process.exit(1);
}

console.log('Business chat layout guard OK');

function expect(value, message) {
  if (!value) {
    failures.push(message);
  }
}

function expectIncludes(value, snippet, message) {
  if (!value.includes(snippet)) {
    failures.push(message);
  }
}

function rejectIncludes(value, snippet, message) {
  if (value.includes(snippet)) {
    failures.push(message);
  }
}

function rejectMatch(value, pattern, message) {
  if (pattern.test(value)) {
    failures.push(message);
  }
}
