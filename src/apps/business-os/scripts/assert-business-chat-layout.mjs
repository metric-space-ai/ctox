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
const dateNavigationBlock = source.match(/root\.querySelector\('\[data-chat-date-prev\]'[\s\S]*?root\.querySelector\('\[data-chat-new\]'\)/)?.[0] || '';

expect(dockRule, 'Missing .ctox-chat-dock CSS rule');
expectIncludes(
  dockRule,
  'grid-template-columns: 88px 115px 34px;',
  'Default chat dock must stay compact for zero visible chats'
);
expectIncludes(dockRule, 'width: max-content;', 'Default chat dock must shrink to its controls');
rejectIncludes(dockRule, 'justify-self: start;', 'Chat dock must not shrink-wrap to its content');
rejectIncludes(dockRule, 'minmax(0, max-content)', 'Chat tab strip must not use content-sized columns');
rejectMatch(dockRule, /(?:^|\n)\s*width:\s*100%;/, 'Default chat dock must not span the shell for zero or one chat');

expect(manyChatsDockRule, 'Missing .ctox-chat-dock.has-many-chats CSS rule');
expectIncludes(
  manyChatsDockRule,
  'grid-template-columns: 88px 115px 28px minmax(0, 1fr) 28px 34px;',
  'Many-chat dock must reserve flexible full-width space for scrollable tabs'
);
expectIncludes(manyChatsDockRule, 'width: 100%;', 'Only many-chat dock should span the available shell width');
expect(oneChatStripRule, 'Missing one-chat compact strip rule');
expectIncludes(oneChatStripRule, 'width: 148px;', 'One-chat strip must have stable compact width');
expect(fewChatsStripRule, 'Missing few-chat strip rule');
expectIncludes(fewChatsStripRule, 'max-width:', 'Few-chat strip must cap growth before many-chat mode');

expectIncludes(source, 'const fitsSideBySide =', 'Chat windows need a side-by-side fit check');
expectIncludes(source, 'const MANY_CHAT_THRESHOLD = 12;', 'Many-chat threshold must be explicit');
expectIncludes(source, 'const MAX_RENDERED_CHAT_TABS = 12;', 'Rendered chat tabs must be capped for busy days');
expectIncludes(source, "openChats.length > 1 && openChats.length < MANY_CHAT_THRESHOLD ? 'has-few-chats' : ''", 'Few-chat mode must include mid-size chat counts without full-width dock');
expectIncludes(source, "openChats.length >= MANY_CHAT_THRESHOLD ? 'has-many-chats' : ''", 'Many-chat mode must not activate before high tab counts');
expectIncludes(source, 'function selectVisibleChats(openChats, activeChat)', 'Busy days must not render every chat tab/window');
expectIncludes(source, 'chatOverflowItem(hiddenChatCount', 'Busy days need an overflow affordance');
expectIncludes(source, 'chatBusyPanel({ chats: openChats, selectedDate, state })', 'Busy days need a filterable list panel');
expectIncludes(source, 'data-chat-list-filter="source"', 'Busy-day list must include source filtering');
expectIncludes(source, 'dateWorkloadPanel({ chats: state.chats, selectedDate })', 'Date selection must expose workload heatmap panel');
expectIncludes(source, 'function workloadDaysAround(chats, selectedDate, count)', 'Date workload panel must aggregate nearby days');
expectIncludes(
  source,
  "stageInner.classList.toggle('is-side-by-side', fitsSideBySide);",
  'Chat stage must mark the side-by-side state'
);
expectIncludes(source, 'const carouselStep =', 'Chat windows need progressive carousel overlap');
expectIncludes(
  source,
  '.ctox-chat-stage-inner.is-side-by-side .ctox-chat-window',
  'Side-by-side windows must neutralize carousel transforms'
);
rejectMatch(
  source,
  /animation:\s*ctoxChatSlideIn[^;]*\bboth\b/,
  'Slide-in animation must not keep fill-mode transforms after layout alignment'
);
expectIncludes(source, 'renderAndPersistChatState', 'Interactive handlers must render before asynchronous persistence');
rejectIncludes(source, "node.querySelector('[data-chat-new]')", 'Chat-window header must not expose a dead/new-chat plus button');
expect(dateNavigationBlock, 'Missing date navigation handler block');
rejectIncludes(dateNavigationBlock, 'ensureChat', 'Date navigation must not create phantom chats');
expectIncludes(
  source,
  '.ctox-chat-window:not(.is-active) .ctox-chat-header-actions',
  'Inactive window controls must be hidden instead of visibly dead'
);
expectIncludes(
  source,
  'setWindowInteractiveState(node, chat.id === activeChat?.id && !chat.minimized);',
  'Inactive window controls must be removed from keyboard tab order'
);
expectIncludes(source, 'class="ctox-date-picker-trigger" role="button" tabindex="0"', 'Visible date trigger must be keyboard focusable');
expectIncludes(source, 'data-chat-date-picker value="${selectedDate}" max="${maxDateVal}" tabindex="-1" aria-hidden="true"', 'Hidden native date input must not enter tab order');

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
