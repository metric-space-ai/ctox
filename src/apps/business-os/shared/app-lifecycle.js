import {
  BusinessOsPermissions,
  canModifyBusinessModule,
  canUseBusinessExplicitOrAssignedPermission,
} from './permissions.js';

export function parseBusinessAppSemver(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(String(version || '').trim());
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

export function businessAppVersion(moduleLike) {
  const lifecycle = moduleLike?.lifecycle;
  if (lifecycle && Object.prototype.hasOwnProperty.call(lifecycle, 'current_semver')) {
    return String(lifecycle.current_semver || '').trim();
  }
  return String(moduleLike?.version || '').trim();
}

export function isRuntimeInstalledModule(moduleLike) {
  if (typeof moduleLike?.lifecycle?.runtime_installed === 'boolean') {
    return moduleLike.lifecycle.runtime_installed;
  }
  const entry = String(moduleLike?.entry || '').trim();
  return moduleLike?.source === 'installed'
    || moduleLike?.install_scope === 'installed'
    || entry.startsWith('installed-modules/');
}

export function hasPublicAppVersion(moduleLike) {
  const parsed = parseBusinessAppSemver(businessAppVersion(moduleLike));
  return Boolean(parsed && parsed.major >= 1);
}

export function appVersionLabel(moduleLike) {
  const raw = businessAppVersion(moduleLike);
  if (!raw) return '';
  return raw.startsWith('v') ? raw : `v${raw}`;
}

function stringList(value) {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => String(item || '').trim())
    .filter(Boolean);
}

function titleizeIdentifier(value) {
  const words = String(value || '')
    .trim()
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean);
  if (!words.length) return '';
  return words
    .map((word) => {
      const lower = word.toLowerCase();
      if (lower === 'ctox') return 'CTOX';
      if (lower === 'id') return 'ID';
      return lower.charAt(0).toUpperCase() + lower.slice(1);
    })
    .join(' ');
}

export function businessDataAreaLabel(collectionId) {
  const id = String(collectionId || '').trim();
  if (!id) return '';
  const label = titleizeIdentifier(id);
  return label ? `${label} (${id})` : id;
}

function formatBusinessDataAreas(collectionIds) {
  return stringList(collectionIds)
    .map(businessDataAreaLabel)
    .filter(Boolean)
    .join(', ');
}

function releaseVersionLabel(release) {
  if (!release || typeof release !== 'object') return '';
  const targetVersion = String(release.target_version || '').trim();
  if (targetVersion) return targetVersion.startsWith('v') ? targetVersion : `v${targetVersion}`;
  const version = String(release.version || '').trim();
  if (version) return `Release ${version}`;
  const versionId = String(release.version_id || '').trim();
  return versionId || '';
}

function releaseStatusLabel(status) {
  switch (String(status || '').trim()) {
    case 'released':
      return 'Freigegeben';
    case 'rolled_back':
      return 'Zurückgerollt';
    case 'unreleased':
      return 'Noch nicht freigegeben';
    case 'incomplete':
      return 'Unvollständig';
    default:
      return status ? titleizeIdentifier(status) : '';
  }
}

export function appDataAccessSummary(moduleLike) {
  const dataAccess = moduleLike?.lifecycle?.data_access || moduleLike?.data_access || null;
  const declared = stringList(moduleLike?.collections || moduleLike?.permissions);
  if (!dataAccess || typeof dataAccess !== 'object') {
    return {
      hasReview: false,
      status: 'not_reviewed',
      statusLabel: 'Noch nicht geprüft',
      declared,
      granted: [],
      locked: [],
      summary: declared.length
        ? `Deklariert: ${formatBusinessDataAreas(declared)}`
        : 'Keine Datenbereiche deklariert',
      reviewNote: '',
      reviewIsEvidenceOnly: false,
      grantsImplied: false,
    };
  }

  const areas = Array.isArray(dataAccess.areas) ? dataAccess.areas : [];
  const granted = stringList(dataAccess.granted_collection_ids);
  const locked = stringList(dataAccess.locked_collection_ids);
  const areaCollections = stringList(areas.map((area) => area?.collection));
  const declaredFromReview = Array.from(new Set([...areaCollections, ...granted, ...locked]));
  const status = String(dataAccess.status || '').trim() || 'not_reviewed';
  const completed = dataAccess.completed === true || status === 'reviewed';
  const reviewIsEvidenceOnly = dataAccess.review_is_evidence_only === true;
  const grantsImplied = dataAccess.grants_implied === true;

  const summaryParts = [];
  if (granted.length) summaryParts.push(`Freigegeben: ${formatBusinessDataAreas(granted)}`);
  if (locked.length) summaryParts.push(`Gesperrt: ${formatBusinessDataAreas(locked)}`);
  if (!summaryParts.length && declaredFromReview.length) {
    summaryParts.push(`Geprüft: ${formatBusinessDataAreas(declaredFromReview)}`);
  }
  const summary = summaryParts.length
    ? summaryParts.join('; ')
    : (completed ? 'Review abgeschlossen, keine Team-Datenbereiche freigegeben' : 'Datenreview noch nicht abgeschlossen');

  const reviewNote = (reviewIsEvidenceOnly || !grantsImplied)
    ? 'Review ist Nachweis; Datenrechte bleiben explizit.'
    : '';

  return {
    hasReview: true,
    status,
    statusLabel: completed ? 'Geprüft' : 'Noch nicht abgeschlossen',
    declared: declaredFromReview.length ? declaredFromReview : declared,
    granted,
    locked,
    areas,
    summary,
    reviewNote,
    reviewIsEvidenceOnly,
    grantsImplied,
    lockedStateBehavior: String(dataAccess.locked_state_behavior || '').trim(),
  };
}

export function appReleaseProjection(moduleLike) {
  const lifecycle = moduleLike?.lifecycle || {};
  const releaseState = lifecycle.release_state || moduleLike?.release_state || {};
  const current = releaseState?.current && typeof releaseState.current === 'object'
    ? releaseState.current
    : null;
  const rollbackTarget = lifecycle.rollback_target && typeof lifecycle.rollback_target === 'object'
    ? lifecycle.rollback_target
    : (releaseState?.rollback_target && typeof releaseState.rollback_target === 'object'
        ? releaseState.rollback_target
        : null);
  const status = String(lifecycle.release_status || releaseState?.status || moduleLike?.release_status || '').trim();
  const statusLabel = releaseStatusLabel(status);
  const currentVersion = releaseVersionLabel(current);
  const rollbackVersion = releaseVersionLabel(rollbackTarget);
  const historyCount = Number.isFinite(Number(releaseState?.history_count))
    ? Number(releaseState.history_count)
    : 0;
  const hasReleaseState = Boolean(status || current || rollbackTarget || historyCount);
  const releaseLine = currentVersion
    ? `Aktuell ${currentVersion}${statusLabel ? ` · ${statusLabel}` : ''}`
    : (statusLabel || 'Noch kein Release projiziert');
  const rollbackLine = rollbackVersion
    ? `Rollback-Ziel ${rollbackVersion}`
    : '';

  return {
    hasReleaseState,
    status,
    statusLabel,
    current,
    currentVersion,
    currentVersionId: String(current?.version_id || '').trim(),
    rollbackTarget,
    rollbackVersion,
    rollbackVersionId: String(rollbackTarget?.version_id || '').trim(),
    historyCount,
    releaseLine,
    rollbackLine,
    dataAccess: appDataAccessSummary(moduleLike),
  };
}

export function appLifecycleState(moduleLike, options = {}) {
  const runtimeInstalled = isRuntimeInstalledModule(moduleLike);
  const parsed = parseBusinessAppSemver(businessAppVersion(moduleLike));
  const versionLabel = appVersionLabel(moduleLike);
  const explicit = moduleLike?.lifecycle?.visibility_state || moduleLike?.visibility_state || '';
  const audience = moduleLike?.lifecycle?.audience || moduleLike?.audience || '';
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || '').trim();
  const canManage = options.canManage === true
    || canModifyBusinessModule(moduleLike, {
      session: options.session,
      governance: options.governance,
    });
  const canAccessNonPublic = options.canAccessNonPublic === true
    || canUseBusinessExplicitOrAssignedPermission({
      session: options.session,
      governance: options.governance,
      permission: BusinessOsPermissions.AppsView,
      scopeType: 'module',
      scopeId: moduleId,
    });

  if (!runtimeInstalled) {
    return {
      state: 'packaged',
      label: 'System',
      shortLabel: versionLabel || 'System',
      versionLabel,
      runtimeInstalled,
      public: true,
      canManage,
      canAccessNonPublic: true,
      reason: 'Packaged Business-OS App.',
    };
  }

  if (!parsed) {
    return {
      state: 'private',
      label: 'Privat',
      shortLabel: 'Privat',
      versionLabel: versionLabel || 'Version fehlt',
      runtimeInstalled,
      public: false,
      canManage,
      canAccessNonPublic,
      warning: true,
      warningCode: moduleLike?.lifecycle?.warning_code || 'invalid_semver',
      reason: 'Diese App hat keine gültige SemVer-Version und bleibt deshalb privat.',
    };
  }

  if (parsed.major >= 1) {
    const restricted = explicit === 'restricted' || audience === 'restricted';
    return {
      state: restricted ? 'restricted' : 'team',
      label: restricted ? 'Eingeschränkt' : 'Team',
      shortLabel: restricted ? 'Schutz' : 'Team',
      versionLabel,
      runtimeInstalled,
      public: !restricted,
      canManage,
      canAccessNonPublic,
      reason: restricted
        ? 'Diese Team-App ist auf eine explizite Zielgruppe eingeschränkt.'
        : 'Diese App ist als Team-Version freigegeben.',
    };
  }

  const preview = explicit === 'preview'
    || audience === 'preview'
    || (Array.isArray(moduleLike?.lifecycle?.preview_user_ids) && moduleLike.lifecycle.preview_user_ids.length > 0);
  return {
    state: preview ? 'preview' : 'private',
    label: preview ? 'Vorschau' : 'Privat',
    shortLabel: preview ? 'Preview' : 'Privat',
    versionLabel,
    runtimeInstalled,
    public: false,
    canManage,
    canAccessNonPublic,
    reason: preview
      ? 'Diese App ist vor dem Team-Release nur für berechtigte Vorschau-Nutzer sichtbar.'
      : 'Diese App ist vor dem Team-Release nur für App-Verantwortliche sichtbar.',
  };
}

export function canSeeModuleForAppVersion(moduleLike, options = {}) {
  const lifecycle = appLifecycleState(moduleLike, options);
  if (!lifecycle.runtimeInstalled) return true;
  if (lifecycle.public) return true;
  return lifecycle.canAccessNonPublic;
}

export function appLifecycleBadge(moduleLike, options = {}) {
  const lifecycle = appLifecycleState(moduleLike, options);
  const version = lifecycle.versionLabel || '';
  const label = lifecycle.label || '';
  return {
    ...lifecycle,
    title: [version, label, lifecycle.reason].filter(Boolean).join(' · '),
    text: label,
    version,
  };
}
