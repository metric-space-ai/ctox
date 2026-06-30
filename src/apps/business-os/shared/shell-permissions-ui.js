export function buildModuleTargetContextItems({
  target,
  pinned = false,
  canModify = false,
  canOpenSource = false,
  labels = {},
  actions = {},
} = {}) {
  if (!target) return [];
  const items = [
    {
      key: 'open',
      label: labels.openApp || 'Öffnen',
      icon: target.glyph || '↗',
      action: actions.open,
    },
    {
      key: pinned ? 'unpin' : 'pin',
      label: pinned
        ? (labels.unpinFromTaskbar || 'Von Bar lösen')
        : (labels.pinToTaskbar || 'An Bar anheften'),
      icon: pinned ? '−' : '+',
      action: actions.togglePin,
    },
  ];
  if (target.kind !== 'module') return items;
  if (canOpenSource || canModify) {
    items.push({ type: 'separator' });
  }
  if (canOpenSource) {
    items.push({
      key: 'source',
      label: labels.openSource || 'Source öffnen',
      icon: '⌘',
      action: actions.openSource,
    });
  }
  if (canModify) {
    items.push({
      key: 'modify-app',
      label: labels.modifyApp || 'App ändern',
      icon: '✎',
      action: actions.modify,
    });
  }
  return items;
}

export function shouldRenderModuleSourceAction({
  module,
  canOpenSource = false,
} = {}) {
  return Boolean(module?.id && module.id !== 'desktop' && canOpenSource);
}

export function buildLifecyclePermissionView({
  canManage = false,
  canOpenSource = false,
  labels = {},
} = {}) {
  if (canManage) {
    return {
      state: 'manager',
      label: labels.managerLabel || 'Verwalten erlaubt',
      description: labels.managerDescription
        || 'Du kannst Sichtbarkeit, Verantwortliche und Releases im App Store verwalten.',
      storeActionLabel: labels.managerStoreAction || 'Im App Store verwalten',
      canManage: true,
      canOpenSource: Boolean(canOpenSource),
    };
  }
  return {
    state: 'readonly',
    label: labels.readonlyLabel || 'Nur Ansicht',
    description: labels.readonlyDescription
      || 'Du siehst Version und Freigabe. Sichtbarkeit, Verantwortliche und Releases ändern berechtigte App-Verantwortliche, Admins oder Owner.',
    storeActionLabel: labels.readonlyStoreAction || 'Details im App Store ansehen',
    canManage: false,
    canOpenSource: Boolean(canOpenSource),
  };
}

export function buildModuleWhyDiagnosticsView({
  actor = {},
  module = {},
  lifecycle = {},
  releaseProjection = {},
  dataAccess = null,
  permissionView = {},
  permissions = {},
  dataPermissions = [],
  labels = {},
} = {}) {
  const moduleId = cleanText(module.id || module.module_id);
  const moduleTitle = cleanText(module.title || module.name || moduleId) || moduleId || labels.unknownApp || 'Unbekannte App';
  const actorId = cleanText(actor.id || actor.user_id);
  const actorRole = cleanText(actor.role || (actor.is_admin ? 'admin' : 'user')) || 'user';
  const actorLabel = cleanText(actor.display_name || actor.name || actorId) || labels.unknownActor || 'Unbekannter Nutzer';
  const visibility = cleanText(lifecycle.label || lifecycle.state) || labels.visibilityUnknown || 'Unklar';
  const version = cleanText(lifecycle.versionLabel || lifecycle.version || module.version);
  const release = releaseProjection || {};
  const access = dataAccess || release.dataAccess || {};
  const canSee = permissions.canSee === true
    || lifecycle.public === true
    || lifecycle.canAccessNonPublic === true
    || lifecycle.state === 'packaged';
  const canOpen = permissions.canOpen === undefined ? canSee : permissions.canOpen === true;
  const canModify = permissions.canModify === true
    || permissionView.canManage === true
    || lifecycle.canManage === true;
  const canOpenSource = permissions.canOpenSource === true
    || permissionView.canOpenSource === true;
  const canRelease = permissions.canRelease === true;
  const canRollback = permissions.canRollback === true;
  const dataSummary = cleanText(access.summary) || labels.noDataAreas || 'Keine Datenbereiche deklariert';
  const releaseLine = cleanText(release.releaseLine || release.statusLabel)
    || labels.noReleaseProjection
    || 'Noch kein Release projiziert';
  const rollbackLine = cleanText(release.rollbackLine)
    || labels.noRollbackProjection
    || 'Noch kein Rollback-Ziel projiziert';

  const rows = [
    {
      key: 'actor',
      label: labels.actor || 'Akteur',
      state: 'info',
      value: actorRole ? `${actorLabel} · ${actorRole}` : actorLabel,
      reason: moduleId
        ? `${labels.actorScope || 'Entscheidungen gelten für diese App'}: ${moduleTitle}`
        : labels.actorScopeUnknown || 'Keine App ausgewählt.',
    },
    {
      key: 'visibility',
      label: labels.visibility || 'Sichtbarkeit',
      state: canSee ? 'allowed' : 'blocked',
      value: [visibility, version].filter(Boolean).join(' · '),
      reason: cleanText(lifecycle.reason)
        || (canSee
          ? labels.visibleReason || 'Diese App ist für diesen Nutzer sichtbar.'
          : labels.hiddenReason || 'Diese App ist für diesen Nutzer nicht sichtbar.'),
    },
    {
      key: 'open',
      label: labels.open || 'App öffnen',
      state: canOpen ? 'allowed' : 'blocked',
      value: decisionLabel(canOpen, labels),
      reason: canOpen
        ? (labels.openAllowedReason || 'Öffnen folgt der App-Sichtbarkeit; Datenzugriff wird danach separat geprüft.')
        : (labels.openBlockedReason || 'Nicht sichtbar: Die App kann für diesen Nutzer nicht geöffnet werden.'),
    },
    {
      key: 'modify',
      label: labels.modify || 'App ändern',
      state: canModify ? 'allowed' : 'blocked',
      value: decisionLabel(canModify, labels),
      reason: canModify
        ? (labels.modifyAllowedReason || 'Dieser Nutzer darf App-Verhalten, Sichtbarkeit und Verantwortliche verwalten.')
        : (labels.modifyBlockedReason || 'Ändern bleibt App-Verantwortlichen, Admins oder Ownern vorbehalten.'),
    },
    {
      key: 'source',
      label: labels.source || 'Source öffnen',
      state: canOpenSource ? 'allowed' : 'blocked',
      value: decisionLabel(canOpenSource, labels),
      reason: canOpenSource
        ? (labels.sourceAllowedReason || 'Source-Zugriff ist für diesen Nutzer freigegeben.')
        : (labels.sourceBlockedReason || 'Source bleibt ohne Source-Recht oder App-Verantwortung verborgen.'),
    },
    {
      key: 'release',
      label: labels.release || 'Freigabe',
      state: canRelease ? 'allowed' : 'blocked',
      value: canRelease
        ? `${decisionLabel(true, labels)} · ${releaseLine}`
        : decisionLabel(false, labels),
      reason: canRelease
        ? (labels.releaseAllowedReason || 'Dieser Nutzer darf Team-Releases vorbereiten oder freigeben.')
        : (labels.releaseBlockedReason || 'Freigaben brauchen ein Release-Recht für diese App.'),
    },
    {
      key: 'rollback',
      label: labels.rollback || 'Rollback',
      state: canRollback ? 'allowed' : 'blocked',
      value: canRollback
        ? `${decisionLabel(true, labels)} · ${rollbackLine}`
        : decisionLabel(false, labels),
      reason: canRollback
        ? (labels.rollbackAllowedReason || 'Dieser Nutzer darf auf ein geprüftes Rollback-Ziel zurückgehen.')
        : (labels.rollbackBlockedReason || 'Rollback braucht ein Rollback-Recht für diese App.'),
    },
    {
      key: 'data',
      label: labels.data || 'Datenbereiche',
      state: dataDecisionState(access, dataPermissions),
      value: dataSummary,
      reason: cleanText(access.reviewNote)
        || (labels.dataSeparateReason || 'App-Sichtbarkeit gibt keine Datenrechte frei; Lesen und Schreiben werden pro Datenbereich geprüft.'),
    },
  ];

  return {
    rows,
    actor: {
      id: actorId,
      label: actorLabel,
      role: actorRole,
      is_admin: actor.is_admin === true,
    },
    app: {
      module_id: moduleId,
      module_title: moduleTitle,
      version,
      visibility,
      lifecycle_state: cleanText(lifecycle.state),
      public: lifecycle.public === true,
      runtime_installed: lifecycle.runtimeInstalled === true || lifecycle.runtime_installed === true,
      can_see: canSee,
      can_open: canOpen,
      can_modify: canModify,
      can_open_source: canOpenSource,
      can_release: canRelease,
      can_rollback: canRollback,
    },
    release: {
      line: releaseLine,
      rollback_line: rollbackLine,
      status: cleanText(release.status),
      status_label: cleanText(release.statusLabel),
      has_release_state: release.hasReleaseState === true,
    },
    data: {
      summary: dataSummary,
      status: cleanText(access.status),
      status_label: cleanText(access.statusLabel),
      declared_collections: cleanList(access.declared),
      granted_collections: cleanList(access.granted),
      locked_collections: cleanList(access.locked),
      review_note: cleanText(access.reviewNote),
      grants_implied: access.grantsImplied === true || access.grants_implied === true,
      decisions: buildDataDecisionRows(access, dataPermissions, labels),
    },
  };
}

export function renderModuleWhyDiagnosticsHtml(options = {}) {
  const view = options.view && typeof options.view === 'object'
    ? options.view
    : buildModuleWhyDiagnosticsView(options);
  const title = options.labels?.whyTitle || 'Warum?';
  const dataRows = Array.isArray(view.data?.decisions) ? view.data.decisions : [];
  return `
    <section class="module-why-diagnostics" data-why-diagnostics="${escapeAttr(view.app?.module_id || '')}" aria-label="${escapeAttr(title)}">
      <div class="module-why-title">${escapeHtml(title)}</div>
      <dl>
        ${view.rows.map((row) => `
          <div data-why-row="${escapeAttr(row.key)}" data-decision-state="${escapeAttr(row.state || 'info')}">
            <dt>${escapeHtml(row.label)}</dt>
            <dd>
              <strong>${escapeHtml(row.value)}</strong>
              <span>${escapeHtml(row.reason)}</span>
            </dd>
          </div>
        `).join('')}
      </dl>
      ${dataRows.length ? `
        <div class="module-why-data" data-why-data-decisions>
          ${dataRows.map((row) => `
            <div data-why-data-row="${escapeAttr(row.collection)}">
              <strong>${escapeHtml(row.label)}</strong>
              <span data-why-data-permission="read">${escapeHtml(row.read.label)} · ${escapeHtml(row.read.reason)}</span>
              <span data-why-data-permission="write">${escapeHtml(row.write.label)} · ${escapeHtml(row.write.reason)}</span>
            </div>
          `).join('')}
        </div>
      ` : ''}
    </section>
  `;
}

export function buildGlobalCtoxContextModes({
  canModify = false,
  canSelfExecute = true,
  labels = {},
} = {}) {
  const dataNeedsApproval = !canSelfExecute;
  const appNeedsApproval = !canModify;
  return [
    {
      value: 'data',
      label: labels.workData || 'Daten ändern',
      impact: dataNeedsApproval ? 'approval_required' : 'data_mutation',
      approvalRequired: dataNeedsApproval,
      description: dataNeedsApproval
        ? (labels.dataApprovalDescription || 'Braucht Freigabe: Daten werden erst nach Review geändert.')
        : (labels.impactDataDescription || 'Ändert den ausgewählten Datensatz oder das Feld.'),
      selected: true,
    },
    {
      value: 'ask',
      label: labels.answer || 'Frage stellen',
      impact: 'read_only',
      approvalRequired: false,
      description: labels.impactAskDescription || 'Liest den Kontext und antwortet, ohne etwas zu ändern.',
      selected: false,
    },
    {
      value: 'app',
      label: labels.modifyApp || 'App ändern',
      impact: appNeedsApproval ? 'approval_required' : 'privileged_app_change',
      approvalRequired: appNeedsApproval,
      description: appNeedsApproval
        ? (labels.appApprovalDescription || 'Braucht Freigabe: Die App wird erst nach Review geändert.')
        : (labels.impactAppDescription || 'Ändert Layout, Logik oder Verhalten dieser App.'),
      selected: false,
    },
  ];
}

export function renderGlobalCtoxContextModeHtml(options = {}) {
  return buildGlobalCtoxContextModes(options)
    .map((mode) => (
      `<label${mode.selected ? ' class="is-selected"' : ''} data-impact="${escapeAttr(mode.impact || '')}" data-approval-required="${mode.approvalRequired ? 'true' : 'false'}" data-description="${escapeAttr(mode.description || '')}" title="${escapeAttr(mode.description || '')}">`
        + `<input type="radio" name="contextMode" value="${escapeAttr(mode.value)}"${mode.selected ? ' checked' : ''} style="display:none;" />`
        + '<span class="ctox-context-mode-copy">'
          + `<span>${escapeHtml(mode.label)}</span>`
        + '</span>'
      + '</label>'
    ))
    .join('');
}

export function buildBusinessUserPickerOptions(users = [], { session = {} } = {}) {
  const byId = new Map();
  const addUser = (user = {}) => {
    const id = cleanText(user.id || user.user_id);
    if (!id || byId.has(id)) return;
    if (user.active === false || user.is_deleted === true || user._deleted === true) return;
    byId.set(id, {
      id,
      display_name: cleanText(user.display_name || user.name || id) || id,
      role: cleanText(user.role || 'user') || 'user',
    });
  };
  (Array.isArray(users) ? users : []).forEach(addUser);
  addUser(session?.user || {});
  return [...byId.values()].sort((a, b) => {
    const byName = a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' });
    return byName || a.id.localeCompare(b.id);
  });
}

export function renderBusinessUserDatalistOptions(users = [], options = {}) {
  return buildBusinessUserPickerOptions(users, options)
    .map((user) => (
      `<option value="${escapeAttr(user.id)}" label="${escapeAttr(`${user.display_name} · ${user.role}`)}"></option>`
    ))
    .join('');
}

export function buildGlobalCtoxAgentScopeView({
  actor = {},
  module = {},
  lifecycle = {},
  dataAccess = {},
  context = {},
  canModify = false,
  externalActions = 'none',
  labels = {},
} = {}) {
  const actorId = cleanText(actor.id || actor.user_id);
  const actorRole = cleanText(actor.role || (actor.is_admin ? 'admin' : 'user')) || 'user';
  const actorLabel = cleanText(actor.display_name || actor.name || actorId) || labels.unknownActor || 'Unbekannter Nutzer';
  const moduleId = cleanText(module.id || module.module_id || context.module) || 'ctox';
  const moduleTitle = cleanText(module.title || module.name || context.label || moduleId) || moduleId;
  const appVersion = cleanText(lifecycle.versionLabel || lifecycle.version || module.version);
  const appVisibility = cleanText(lifecycle.state || (lifecycle.public ? 'team' : 'private')) || 'unknown';
  const appVisibilityLabel = cleanText(lifecycle.label || lifecycle.text) || visibilityLabel(appVisibility, labels);
  const dataSummary = cleanText(dataAccess.summary)
    || (Array.isArray(dataAccess.declared) && dataAccess.declared.length
      ? `${labels.declaredData || 'Deklariert'}: ${dataAccess.declared.join(', ')}`
      : labels.noDataAreas || 'Keine Datenbereiche deklariert');
  const externalLabel = externalActions === 'allowed'
    ? (labels.externalAllowed || 'Freigabe möglich')
    : externalActions === 'approval_required'
      ? (labels.externalApproval || 'Nur mit Freigabe')
      : (labels.externalBlocked || 'In diesem Schritt aus');
  const recordId = cleanText(context.record_id || context.recordId);
  const recordLabel = cleanText(context.label || context.record_label || context.clicked_text || moduleTitle);
  const recordType = cleanText(context.record_type || context.recordType || 'module');
  const selectedText = cleanText(context.selected_text || context.selectedText);
  const clickedText = cleanText(context.clicked_text || context.clickedText);

  const rows = [
    {
      key: 'actor',
      label: labels.actor || 'Nutzer',
      value: actorRole ? `${actorLabel} · ${actorRole}` : actorLabel,
    },
    {
      key: 'app',
      label: labels.app || 'App',
      value: [moduleTitle, appVersion, appVisibilityLabel].filter(Boolean).join(' · '),
    },
    {
      key: 'data',
      label: labels.data || 'Daten',
      value: dataSummary,
    },
    {
      key: 'external',
      label: labels.external || 'Externe Aktionen',
      value: externalLabel,
    },
  ];

  if (recordId || recordLabel) {
    rows.splice(2, 0, {
      key: 'selection',
      label: labels.selection || 'Auswahl',
      value: [recordLabel, recordId].filter(Boolean).join(' · '),
    });
  }

  return {
    rows,
    actor: {
      id: actorId,
      label: actorLabel,
      role: actorRole,
      is_admin: Boolean(actor.is_admin),
    },
    app: {
      module_id: moduleId,
      module_title: moduleTitle,
      version: appVersion,
      visibility: appVisibility,
      visibility_label: appVisibilityLabel,
      public: lifecycle.public === true,
      runtime_installed: lifecycle.runtimeInstalled === true || lifecycle.runtime_installed === true,
      can_modify: Boolean(canModify),
      can_manage: Boolean(lifecycle.canManage || canModify),
    },
    data: {
      summary: dataSummary,
      status: cleanText(dataAccess.status),
      status_label: cleanText(dataAccess.statusLabel),
      declared_collections: cleanList(dataAccess.declared),
      granted_collections: cleanList(dataAccess.granted),
      locked_collections: cleanList(dataAccess.locked),
      grants_implied: dataAccess.grantsImplied === true || dataAccess.grants_implied === true,
    },
    external_actions: {
      mode: externalActions,
      label: externalLabel,
    },
    selection: {
      module_id: moduleId,
      column: cleanText(context.column || 'center'),
      record_type: recordType,
      record_id: recordId,
      label: recordLabel,
      has_selected_text: Boolean(selectedText),
      has_clicked_text: Boolean(clickedText),
    },
  };
}

export function renderGlobalCtoxAgentScopeHtml(options = {}) {
  const view = options.view && typeof options.view === 'object'
    ? options.view
    : buildGlobalCtoxAgentScopeView(options);
  const title = options.labels?.scopeTitle || 'CTOX Zugriff';
  return `
    <section class="ctox-agent-scope" aria-label="${escapeAttr(title)}">
      <div class="ctox-agent-scope-title">${escapeHtml(title)}</div>
      <dl>
        ${view.rows.map((row) => `
          <div data-agent-scope-row="${escapeAttr(row.key)}">
            <dt>${escapeHtml(row.label)}</dt>
            <dd>${escapeHtml(row.value)}</dd>
          </div>
        `).join('')}
      </dl>
    </section>
  `;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (ch) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[ch]));
}

function escapeAttr(value) {
  return escapeHtml(value);
}

function cleanText(value) {
  return String(value ?? '').trim();
}

function cleanList(value) {
  if (!Array.isArray(value)) return [];
  return value.map(cleanText).filter(Boolean);
}

function decisionLabel(allowed, labels = {}) {
  return allowed
    ? (labels.allowed || 'Erlaubt')
    : (labels.blocked || 'Nicht erlaubt');
}

function dataDecisionState(dataAccess = {}, dataPermissions = []) {
  const decisions = buildDataDecisionRows(dataAccess, dataPermissions);
  if (!decisions.length) return 'info';
  const allBlocked = decisions.every((row) => !row.read.allowed && !row.write.allowed);
  if (allBlocked) return 'blocked';
  const allAllowed = decisions.every((row) => row.read.allowed && row.write.allowed);
  return allAllowed ? 'allowed' : 'partial';
}

function buildDataDecisionRows(dataAccess = {}, dataPermissions = [], labels = {}) {
  const areas = Array.isArray(dataAccess?.areas) ? dataAccess.areas : [];
  const declared = cleanList(dataAccess?.declared);
  const granted = cleanList(dataAccess?.granted);
  const locked = cleanList(dataAccess?.locked);
  const permissionByCollection = new Map(
    (Array.isArray(dataPermissions) ? dataPermissions : [])
      .map((item) => [cleanText(item?.collection || item?.collection_id), item])
      .filter(([collection]) => Boolean(collection)),
  );
  const collections = new Set([
    ...declared,
    ...granted,
    ...locked,
    ...areas.map((area) => cleanText(area?.collection)).filter(Boolean),
    ...permissionByCollection.keys(),
  ]);
  return [...collections].map((collection) => {
    const area = areas.find((item) => cleanText(item?.collection) === collection) || {};
    const permission = permissionByCollection.get(collection) || {};
    return {
      collection,
      label: dataAreaLabel(collection),
      read: buildSingleDataDecision({
        allowed: permission.readAllowed ?? permission.read?.allowed,
        reviewState: permission.readReviewState || permission.read_review_state || area.read,
        collection,
        kind: 'read',
        granted,
        locked,
        labels,
      }),
      write: buildSingleDataDecision({
        allowed: permission.writeAllowed ?? permission.write?.allowed,
        reviewState: permission.writeReviewState || permission.write_review_state || area.write,
        collection,
        kind: 'write',
        granted,
        locked,
        labels,
      }),
    };
  });
}

function buildSingleDataDecision({
  allowed,
  reviewState,
  collection,
  kind,
  granted = [],
  locked = [],
  labels = {},
} = {}) {
  const normalizedAllowed = allowed === true;
  const state = cleanText(reviewState)
    || (granted.includes(collection) ? 'granted' : '')
    || (locked.includes(collection) ? 'locked' : '')
    || 'not_reviewed';
  const action = kind === 'write'
    ? (labels.write || 'Schreiben')
    : (labels.read || 'Lesen');
  const reason = dataReviewReason(state, normalizedAllowed, labels);
  return {
    allowed: normalizedAllowed,
    state,
    label: `${action}: ${decisionLabel(normalizedAllowed, labels)}`,
    reason,
  };
}

function dataReviewReason(state, allowed, labels = {}) {
  if (allowed) return labels.dataAllowedReason || 'Wirksames Datenrecht ist vorhanden.';
  switch (state) {
    case 'granted':
      return labels.dataGrantNeedsPermission || 'Review ist freigegeben; es fehlt noch das wirksame Datenrecht.';
    case 'locked':
      return labels.dataLockedReason || 'Review markiert diesen Datenbereich als gesperrt.';
    case 'not_requested':
      return labels.dataNotRequestedReason || 'Dieser Zugriff wurde im Review nicht angefordert.';
    case 'not_reviewed':
      return labels.dataNotReviewedReason || 'Datenzugriff ist noch nicht geprüft.';
    default:
      return labels.dataBlockedReason || 'Kein wirksames Datenrecht fuer diesen Datenbereich.';
  }
}

function dataAreaLabel(collectionId) {
  const id = cleanText(collectionId);
  if (!id) return '';
  const words = id
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((word) => {
      const lower = word.toLowerCase();
      if (lower === 'ctox') return 'CTOX';
      if (lower === 'id') return 'ID';
      return lower.charAt(0).toUpperCase() + lower.slice(1);
    })
    .join(' ');
  return words ? `${words} (${id})` : id;
}

function visibilityLabel(value, labels = {}) {
  switch (cleanText(value)) {
    case 'team':
      return labels.visibilityTeam || 'Team';
    case 'preview':
      return labels.visibilityPreview || 'Vorschau';
    case 'restricted':
      return labels.visibilityRestricted || 'Eingeschränkt';
    case 'packaged':
      return labels.visibilityPackaged || 'System';
    case 'private':
      return labels.visibilityPrivate || 'Privat';
    default:
      return labels.visibilityUnknown || 'Unklar';
  }
}
