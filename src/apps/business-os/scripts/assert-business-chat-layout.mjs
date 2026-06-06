import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const modulePath = resolve(scriptDir, '../shared/business-chat.js');
const source = readFileSync(modulePath, 'utf8');
const failures = [];

const dockRule = source.match(/\.ctox-chat-dock\s*\{(?<body>[\s\S]*?)\n\s*\}/)?.groups?.body || '';

expect(dockRule, 'Missing .ctox-chat-dock CSS rule');
expectIncludes(
  dockRule,
  'grid-template-columns: 88px 115px 28px minmax(0, 1fr) 28px 34px;',
  'Chat dock must reserve flexible full-width space for chat tabs'
);
expectIncludes(dockRule, 'width: 100%;', 'Chat dock must span the available shell width');
rejectIncludes(dockRule, 'justify-self: start;', 'Chat dock must not shrink-wrap to its content');
rejectIncludes(dockRule, 'minmax(0, max-content)', 'Chat tab strip must not use content-sized columns');
rejectIncludes(dockRule, 'width: auto;', 'Chat dock must not revert to auto width');

expectIncludes(source, 'const fitsSideBySide =', 'Chat windows need a side-by-side fit check');
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
