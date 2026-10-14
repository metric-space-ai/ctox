// SPDX-License-Identifier: MIT OR AGPL-3.0-only
// Identity helpers only. The customer shell does not paint release, recovery,
// channel, or health prose; Workjet owns shell-pack status.

const SHELL_UPDATE_STATES = new Set([
  'current',
  'checking',
  'available',
  'download',
  'verify',
  'ready',
  'restart',
  'failed',
  'incompatible',
  'blocked',
  'rollback',
  'recovery',
]);

const SHELL_HEALTH_STATES = new Set(['healthy', 'degraded', 'unknown']);

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
  return SHELL_UPDATE_STATES.has(state) ? state : 'failed';
}

export function normalizeShellHealth(value, runtimeStatus = '') {
  if (runtimeStatus === 'ready') return 'healthy';
  if (runtimeStatus === 'failed') return 'degraded';
  return SHELL_HEALTH_STATES.has(value) ? value : 'unknown';
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

export function readEmbeddedIdentity(documentRoot) {
  const versions = documentRoot.querySelectorAll('meta[name="ctox-shell-version"]');
  const commits = documentRoot.querySelectorAll('meta[name="ctox-shell-source-commit"]');
  const version = versions.length === 1 ? normalizeShellVersion(versions[0].content) : '';
  const sourceCommit = commits.length === 1 ? commits[0].content : '';
  if (!version || !/^[0-9a-f]{40}$/.test(sourceCommit)) throw new Error('Missing or ambiguous shell document identity');
  return { version, sourceCommit, channel: shellChannel(version), state: 'current' };
}
