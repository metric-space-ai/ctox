// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const root = new URL('../', import.meta.url);
const html = await readFile(new URL('index.html', root), 'utf8');
const css = await readFile(new URL('app.css', root), 'utf8');
const start = html.indexOf('<header class="topbar">');
const end = html.indexOf('</header>', start);
assert.ok(start >= 0 && end > start, 'topbar must exist');
const region = html.slice(start, end);

assert.match(region, /data-shell-start/);
assert.match(region, /data-shell-instance-name/);
assert.match(region, /data-module-tabs/);
assert.match(region, /data-open-account/);
assert.doesNotMatch(region, /data-shell-instance-toggle|aria-haspopup/);
assert.doesNotMatch(
  region,
  /data-shell-recovery-pill|data-shell-release-panel|data-shell-release-status|data-shell-version-label|data-ctox-version|data-open-sync-diagnostics|data-collection-freshness-warning|data-ctox-shell-warning|data-recovery-warning|data-shell-instance-health|data-shell-instance-cause|brand-status|data-status-text|aria-live|Stand unbestätigt|Recovery|Sync-Diagnose|CTOX Verbindung|Aktualisieren/,
);
assert.match(
  css,
  /\.topbar-status-bar \.brand-status\s*\{[^}]*display:\s*none;/s,
  'a reintroduced status line must stay hidden',
);

console.log('ok - shell header is instance name, open apps, and account only');
