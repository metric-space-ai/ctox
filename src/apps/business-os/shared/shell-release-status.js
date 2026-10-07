// SPDX-License-Identifier: MIT OR AGPL-3.0-only

const STATUS_COPY = Object.freeze({
  current: ['✓', 'Aktuell'],
  checking: ['…', 'Prüfung läuft'],
  available: ['↓', 'Update verfügbar'],
  download: ['↓', 'Download'],
  verify: ['◌', 'Verifikation'],
  ready: ['✓', 'Bereit zur Aktivierung'],
  restart: ['↻', 'Neustart erforderlich'],
  failed: ['!', 'Fehlgeschlagen'],
  incompatible: ['!', 'Inkompatibel'],
  blocked: ['×', 'Kein administrativer Zugriff'],
  rollback: ['↶', 'Rollback aktiv'],
  recovery: ['!', 'Recovery-Shell'],
});

const HEALTH_COPY = Object.freeze({
  healthy: 'Gesund',
  degraded: 'Beeinträchtigt',
  unknown: 'Unbekannt',
});

const ACTION_COPY = Object.freeze({
  current: 'Keine Aktion erforderlich',
  checking: 'Vorgang läuft',
  available: 'In Workjet → Business OS → Updates aktualisieren',
  download: 'Vorgang läuft',
  verify: 'Vorgang läuft',
  ready: 'In Workjet → Business OS → Updates aktivieren',
  restart: 'Workjet oder CTOX neu starten',
  failed: 'In Workjet → Business OS → Updates erneut versuchen',
  incompatible: 'Kompatible Version wählen',
  blocked: 'Administratorzugriff herstellen',
  rollback: 'In Workjet → Business OS → Updates prüfen',
  recovery: 'In Workjet → Business OS → Updates wiederherstellen',
});

export function shellChannel(version) {
  const value = String(version || '');
  if (value.includes('-nightly.')) return 'nightly';
  if (value.includes('-')) return 'beta';
  return 'stable';
}

export function normalizeShellVersion(value) {
  const version = String(value || '').trim().replace(/^v/, '');
  return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version) ? version : '';
}

export function normalizeShellUpdateStatus(value) {
  const state = String(value || '');
  return Object.hasOwn(STATUS_COPY, state) ? state : 'failed';
}

export function normalizeShellHealth(value, runtimeStatus = '') {
  if (runtimeStatus === 'ready') return 'healthy';
  if (runtimeStatus === 'failed') return 'degraded';
  return Object.hasOwn(HEALTH_COPY, value) ? value : 'unknown';
}

export function formatShellTimestamp(value) {
  if (typeof value !== 'string' || value.length > 64) return '—';
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return '—';
  return new Intl.DateTimeFormat('de-DE', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(timestamp));
}

export function formatShellCompatibility(value) {
  if (typeof value !== 'object' || value === null) return '—';
  const semver = (candidate) => normalizeShellVersion(candidate);
  const workjetMin = semver(value.workjetMinVersion);
  const workjetMax = value.workjetMaxVersion === null ? '' : semver(value.workjetMaxVersion);
  const ctoxMin = semver(value.ctoxMinVersion);
  const ctoxMax = value.ctoxMaxVersion === null ? '' : semver(value.ctoxMaxVersion);
  if (!workjetMin || !ctoxMin || (value.workjetMaxVersion !== null && !workjetMax) || (value.ctoxMaxVersion !== null && !ctoxMax)) return '—';
  const range = (label, min, max) => `${label} ≥${min}${max ? ` ≤${max}` : ''}`;
  return `${range('Workjet', workjetMin, workjetMax)} · ${range('CTOX', ctoxMin, ctoxMax)}`;
}

function render(root, detail) {
  const version = normalizeShellVersion(detail?.version);
  const state = normalizeShellUpdateStatus(detail?.state);
  const channel = detail?.channel || (version ? shellChannel(version) : 'recovery');
  const [icon, stateLabel] = STATUS_COPY[state];
  root.dataset.shellState = state;
  const recovery = root.querySelector('[data-shell-recovery-pill]');
  if (recovery) recovery.hidden = state !== 'recovery';
  root.querySelector('[data-shell-version-label]').textContent = version ? `v${version}` : 'Recovery';
  const statusButton = root.querySelector('[data-shell-release-status]');
  statusButton.dataset.state = state;
  statusButton.title = stateLabel;
  statusButton.querySelector('[data-shell-release-icon]').textContent = icon;
  root.querySelector('[data-shell-release-panel-version]').textContent = version ? `v${version}` : 'Recovery-Shell';
  const offeredVersion = normalizeShellVersion(detail?.offeredVersion);
  root.querySelector('[data-shell-release-panel-offered]').textContent = offeredVersion ? `v${offeredVersion}` : '—';
  root.querySelector('[data-shell-release-panel-channel]').textContent = channel;
  root.querySelector('[data-shell-release-panel-state]').textContent = stateLabel;
  const runtimeStatus = globalThis.CTOX_BUSINESS_OS_APP?.dataPlaneReadyStatus;
  root.querySelector('[data-shell-release-panel-health]').textContent = HEALTH_COPY[normalizeShellHealth(detail?.health, runtimeStatus)];
  root.querySelector('[data-shell-release-panel-published]').textContent = formatShellTimestamp(detail?.publishedAt);
  root.querySelector('[data-shell-release-panel-compatibility]').textContent = formatShellCompatibility(detail?.compatibility);
  root.querySelector('[data-shell-release-panel-checked]').textContent = formatShellTimestamp(detail?.lastCheckedAt);
  root.querySelector('[data-shell-release-panel-action]').textContent = ACTION_COPY[state];
}

export function readEmbeddedIdentity(documentRoot) {
  const versions = documentRoot.querySelectorAll('meta[name="ctox-shell-version"]');
  const commits = documentRoot.querySelectorAll('meta[name="ctox-shell-source-commit"]');
  const version = versions.length === 1 ? normalizeShellVersion(versions[0].content) : '';
  const sourceCommit = commits.length === 1 ? commits[0].content : '';
  if (!version || !/^[0-9a-f]{40}$/.test(sourceCommit)) throw new Error('Missing or ambiguous shell document identity');
  return { version, sourceCommit, channel: shellChannel(version), state: 'current' };
}

function start() {
  // The native Workjet host supplies the verified shell-pack identity and
  // lifecycle status across its typed bridge. A relative fetch here would be
  // an accidental HTTP control/data fallback against appassets and must not
  // run in the RxDB/WebRTC-only mobile host.
  const root = document.querySelector('[data-shell-release]');
  if (!root) return;
  const button = root.querySelector('[data-shell-release-status]');
  const panel = root.querySelector('[data-shell-release-panel]');
  let lastDetail = { version: '', channel: 'recovery', state: 'recovery' };
  const versionLabel = root.querySelector('[data-shell-version-label]');
  const instanceButtons = root.querySelectorAll('[data-shell-instance-toggle]');
  const togglePanel = () => {
    panel.hidden = !panel.hidden;
    button.setAttribute('aria-expanded', String(!panel.hidden));
    versionLabel?.setAttribute?.('aria-expanded', String(!panel.hidden));
    instanceButtons.forEach((control) => control.setAttribute('aria-expanded', String(!panel.hidden)));
    if (!panel.hidden) render(root, lastDetail);
  };
  button.addEventListener('click', togglePanel);
  // The instance and Recovery controls expose the existing release menu;
  // version identities remain available in that menu.
  versionLabel?.addEventListener?.('click', togglePanel);
  instanceButtons.forEach((control) => control.addEventListener('click', togglePanel));
  root.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || panel.hidden) return;
    panel.hidden = true;
    button.setAttribute('aria-expanded', 'false');
    versionLabel?.setAttribute?.('aria-expanded', 'false');
    instanceButtons.forEach((control) => control.setAttribute('aria-expanded', 'false'));
    instanceButtons[0]?.focus();
    event.preventDefault();
  });
  document.addEventListener('click', (event) => {
    if (root.contains(event.target)) return;
    panel.hidden = true;
    button.setAttribute('aria-expanded', 'false');
    versionLabel?.setAttribute?.('aria-expanded', 'false');
    instanceButtons.forEach((control) => control.setAttribute('aria-expanded', 'false'));
  });
  window.addEventListener('workjet:shell-update-status', (event) => {
    lastDetail = event.detail;
    render(root, lastDetail);
  });
  try {
    lastDetail = readEmbeddedIdentity(document);
  } catch {
    lastDetail = { version: '', channel: 'recovery', state: 'recovery' };
  }
  render(root, lastDetail);
}

if (typeof document !== 'undefined') start();
