// Kopfleiste des v4-Rahmens = Kopfleiste der Shell-V2 (Strang TOPBAR, Owner 15.10.: „das bleibt natürlich alles
// erhalten!!!!“, M-20 „der button um zu den settings zu kommen ist weg“).
//
// DOM und Klassen wie src/apps/business-os/index.html (header.topbar); Aussehen kommt wörtlich aus app.css +
// shared/shell-topbar.css über shared/shell-v2-topbar.css (erzeugt von lab/extract-topbar-css.mjs, G0 im Rahmen-Labor
// vergleicht beide Pfade pixelweise).
//
//   links   Launcher-Raster (.start-button) → onLauncher (v4-Launcher/Desktop), Workspace-Umschalter mit Punkt
//   Mitte   App-Reiter (.module-tabs) = die EINZIGE Leiste (FR6c, M-07): ein Reiter je App bzw. Gruppe (taskbar.js
//           buildEntries: Crew-Chats unter „Crew“, angeheftete Apps verschmolzen mit ihren Fenstern), ab zwei Fenstern
//           die Zahl am Reiter, Klick reihum, Rechtsklick Fensterliste. Überlauf „+N“ wie shared/topbar-apps.js, der
//           aktive Reiter bleibt immer sichtbar; die Leiste wächst nie über ihre Breite. Pins per Ziehen (DRAG_APP/PIN).
//   rechts  Zustandspunkt (.shell-sync-status), Zahnrad „Einstellungen“, Avatar (.account-button) mit Konto-Blatt
//
// Zustandspunkt: Quelle ist v4/shell/sync-state.js (Strang ZONE45, currentSyncState()). Grün nur bei level 'ok'.
// Ohne Quelle oder bei 'unknown' bleibt der Punkt grau und ohne Text (kein data-sync-state).
// Einstellungen (Sprache, Schema) liegen in shell_preferences.settings (Schema: settings = Objekt, frei).

import { chooseVisibleTopbarApps } from '../../shared/topbar-apps.js';
import { currentSyncState, SYNC_TEXT } from '../shell/sync-state.js';
import { DRAG_APP, DRAG_PIN } from './launcher.js';
import { buildEntries, nextInGroup } from './taskbar.js';

const GRID_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></svg>';
const PERSON_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 12.5a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z"></path><path d="M4.75 20.25a7.25 7.25 0 0 1 14.5 0"></path></svg>';
// Zahnrad im Strich der Shell-V2-Symbole (24er Raster, currentColor).
const GEAR_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"></path></svg>';

// Ergänzungen der Reiter (FR6c). Zahl wörtlich wie shared/shell-topbar.css .module-tab-count (Shell-V2), Fensterliste
// im Stil der Leisten-Menüs, Zahnrad unter 560 px aus (Einstellungen bleiben über das Konto-Blatt erreichbar).
const TABS_CSS = `
:where([data-v4-topbar]) .topbar .module-tab-count {
  min-width: 16px; padding: 1px 4px; border-radius: 4px;
  background: var(--accent-soft); color: var(--accent);
  font-size: 10px; font-weight: 650; line-height: 14px; text-align: center;
}
:where([data-v4-topbar]) .topbar .module-tab-count[hidden] { display: none; }
:where([data-v4-topbar]) .module-tabs.is-drop-target { outline: 2px dashed var(--accent); outline-offset: -2px; border-radius: 6px; }
:where([data-v4-topbar]) .module-tab.is-drop-before { box-shadow: inset 2px 0 0 var(--accent); }
@media (max-width: 559.98px) { :where([data-v4-topbar]) [data-v4-settings-button] { display: none; } }
.v4f-task-menu {
  position: fixed; z-index: 9600; min-width: 200px; max-width: min(320px, calc(100vw - 16px)); max-height: 60vh;
  overflow: auto; padding: 4px; box-sizing: border-box; display: flex; flex-direction: column; gap: 1px;
  background: var(--v4f-surface); color: var(--v4f-fg); border: 1px solid var(--v4f-line); border-radius: 8px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, .18); font: 13px/1.35 var(--v4f-font);
}
.v4f-task-menu[hidden] { display: none; }
.v4f-task-menu-item {
  display: flex; align-items: center; gap: 8px; min-height: 32px; padding: 0 8px; border: 0; border-radius: 6px;
  background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer;
}
.v4f-task-menu-item .v4f-icon { --v4f-icon: 20px; border-radius: 5px; flex: 0 0 auto; }
.v4f-task-menu-item [data-v4-menu-label] { min-width: 0; flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.v4f-task-menu-item:hover, .v4f-task-menu-item:focus-visible { background: var(--v4f-surface-2); outline: none; }
.v4f-task-menu-item.is-active { background: var(--v4f-accent-soft); }
.v4f-task-menu-item.is-minimized { opacity: .7; }
`;

const TEXT = {
  de: {
    start: 'Startmenü öffnen', openApps: 'Geöffnete Apps', settings: 'Einstellungen', account: 'Konto', close: 'Schließen',
    // v4-allow: en – „Workspace“ ist der Produktbegriff der Shell-V2 (welsch zeigt wörtlich „Lokaler Workspace“).
    workspace: 'Workspace', localWorkspace: 'Lokaler Workspace', instance: 'Instanz', role: 'Rolle', logout: 'Abmelden',
    language: 'Sprache', theme: 'Schema', system: 'System', light: 'Hell', dark: 'Dunkel', more: (n) => `${n} weitere Apps`, moreMenu: 'Weitere Apps',
    sync: 'Verbindung', syncOk: 'Alles live',
  },
  // v4-allow: en – englisches Wörterbuch
  en: {
    start: 'Open start menu', openApps: 'Open apps', settings: 'Settings', account: 'Account', close: 'Close',
    workspace: 'Workspace', localWorkspace: 'Local workspace', instance: 'Instance', role: 'Role', logout: 'Sign out', // v4-allow: en
    language: 'Language', theme: 'Scheme', system: 'System', light: 'Light', dark: 'Dark', more: (n) => `${n} more apps`, moreMenu: 'More apps',
    sync: 'Connection', syncOk: 'All live',
  },
};
// Deutsche Rollennamen wie Shell-V2 (shared/roles.js roleDisplayName): chef/owner = „Owner“, founder =
// „App-Verantwortliche:r“, user/member = „Teammitglied“.
// v4-allow: en – „Owner“ und „Admin“ sind die Rollennamen der Shell-V2 auch im Deutschen.
const ROLE = { admin: 'Admin', owner: 'Owner', chef: 'Owner', founder: 'App-Verantwortliche:r', member: 'Teammitglied', user: 'Teammitglied', viewer: 'Leser' };

const esc = (v) => String(v ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export function initialsOf(name) {
  const words = String(name || '').trim().split(/\s+/).filter(Boolean);
  return words.slice(0, 2).map((w) => [...w][0]).join('').toLocaleUpperCase();
}
// Shell-V2: Instanz-ID „local“ heißt „Lokaler Workspace“; sonst der Name, den der Kernel liefert, sonst die ID.
export function workspaceName(identity, tx = TEXT.de) {
  const id = identity || {};
  const named = id.instance_name || id.instance_title || id.workspace_name || id.display_name || '';
  if (named) return String(named);
  const iid = String(id.instance_id || '');
  return !iid || iid === 'local' || iid === 'lab' ? tx.localWorkspace : iid;
}
export const SYNC_WARN_DELAY_MS = 2000;
// sync-state level → data-sync-state der Shell-V2 (shell-topbar.css). 'unknown' und keine Quelle → kein Attribut (grau).
export function syncPresentation(overall, tx = TEXT.de) {
  const level = overall?.level;
  if (level === 'ok') return { state: 'healthy', label: '', title: tx.syncOk };
  if (level === 'warn') return { state: 'unconfirmed', label: SYNC_TEXT.warn, title: SYNC_TEXT.warn };
  if (level === 'error') return { state: 'error', label: SYNC_TEXT.error, title: SYNC_TEXT.error };
  return { state: null, label: '', title: tx.sync };
}

// Markup der Kopfleiste samt Blättern. Die Hülle [data-v4-topbar] trägt data-shell-style="ctox" (Shell-V2-Standard),
// .app-shell darin ist nur der Bezugsrahmen der wörtlich übernommenen Regeln „.app-shell > .topbar“.
export function topbarMarkup(tx = TEXT.de) {
  return `
<div class="app-shell">
  <header class="topbar">
    <div class="topbar-left-group">
      <div class="topbar-brand-triad">
        <button class="topbar-triad-button start-button" type="button" data-shell-start aria-label="${esc(tx.start)}" title="${esc(tx.start)}">${GRID_SVG}</button>
      </div>
      <div class="topbar-status-bar">
        <div class="ctox-version-line shell-release-line" data-shell-release data-shell-state="current">
          <button class="shell-instance-button" type="button" data-shell-instance-toggle aria-haspopup="dialog" aria-expanded="false">
            <span class="shell-instance-dot" data-shell-instance-health data-health="unknown" aria-hidden="true"></span>
            <span data-shell-instance-name></span>
            <span class="shell-instance-chevron" aria-hidden="true">⌄</span>
          </button>
          <section class="shell-release-panel" data-shell-release-panel role="dialog" aria-label="${esc(tx.workspace)}" hidden>
            <strong data-v4-workspace-title></strong>
            <dl><div><dt>${esc(tx.instance)}</dt><dd data-v4-instance-id></dd></div></dl>
            <div class="shell-release-panel-actions">
              <button class="text-button" type="button" data-open-settings>${esc(tx.settings)}</button>
            </div>
          </section>
        </div>
      </div>
    </div>
    <div class="module-nav">
      <nav class="module-tabs" data-module-tabs aria-label="${esc(tx.openApps)}"></nav>
    </div>
    <div class="topbar-actions">
      <button class="shell-sync-status" type="button" data-v4-sync aria-label="${esc(tx.sync)}" title="${esc(tx.sync)}">
        <span class="shell-sync-dot" aria-hidden="true"></span>
        <span data-sync-state-label aria-live="polite"></span>
      </button>
      <button class="icon-button app-menu-button" type="button" data-open-settings data-v4-settings-button aria-label="${esc(tx.settings)}" title="${esc(tx.settings)}">${GEAR_SVG}</button>
      <button class="account-button" type="button" data-open-account aria-label="${esc(tx.account)}">
        ${PERSON_SVG}
        <span data-account-label></span>
      </button>
    </div>
  </header>
</div>
<div class="drawer-backdrop" data-v4-top-backdrop hidden></div>
<aside class="drawer drawer-right account-popover" data-v4-account-panel role="dialog" aria-label="${esc(tx.account)}" hidden>
  <div class="drawer-body account-drawer">
    <button class="text-button account-settings-entry" type="button" data-open-settings data-v4-account-settings>${esc(tx.settings)}</button>
    <header class="drawer-header-row">
      <div><h2>${esc(tx.account)}</h2><p data-v4-account-sub></p></div>
      <button class="icon-button" type="button" data-v4-top-close aria-label="${esc(tx.close)}">×</button>
    </header>
    <section class="account-role-card" data-v4-role-card hidden><span>${esc(tx.role)}</span><strong data-v4-role></strong></section>
    <div class="account-actions" data-v4-account-actions hidden>
      <button class="text-button" type="button" data-logout>${esc(tx.logout)}</button>
    </div>
  </div>
</aside>
<aside class="drawer drawer-right settings-drawer" data-v4-settings-panel role="dialog" aria-label="${esc(tx.settings)}" hidden>
  <div class="drawer-body">
    <header class="drawer-header-row">
      <div><h2>${esc(tx.settings)}</h2></div>
      <button class="icon-button" type="button" data-v4-top-close aria-label="${esc(tx.close)}">×</button>
    </header>
    <div class="settings-preferences" aria-label="${esc(tx.settings)}">
      <label class="settings-preference-control">
        <span>${esc(tx.language)}</span>
        <select class="header-select" data-language-select aria-label="${esc(tx.language)}">
          <option value="de">DE</option><option value="en">EN</option>
        </select>
      </label>
      <label class="settings-preference-control">
        <span>${esc(tx.theme)}</span>
        <select class="header-select" data-theme-select aria-label="${esc(tx.theme)}">
          <option value="system">${esc(tx.system)}</option><option value="light">${esc(tx.light)}</option><option value="dark">${esc(tx.dark)}</option>
        </select>
      </label>
    </div>
  </div>
</aside>`;
}

export function createTopbar({ host, kernel, lang = 'de', iconFor = null, prefs = null, actor = null, onLauncher = null, onTab = null,
  onPinned = null, onWindow = null, onFocusWindow = null, onPinDrop = null, syncState = currentSyncState } = {}) {
  const tx = TEXT[lang] || TEXT.de;
  const el = document.createElement('div');
  el.className = 'v4f-top';
  el.dataset.v4Topbar = '';
  el.dataset.shellStyle = 'ctox';
  el.innerHTML = topbarMarkup(tx);
  const tabsCss = document.createElement('style');
  tabsCss.dataset.v4TabsCss = '';
  tabsCss.textContent = TABS_CSS;
  const winMenu = document.createElement('div');
  winMenu.className = 'v4f-task-menu';
  winMenu.dataset.v4TaskMenu = '';
  winMenu.setAttribute('role', 'menu');
  winMenu.hidden = true;
  el.append(tabsCss, winMenu);
  host.prepend(el);
  host.dataset.v4HasTopbar = '';
  const $ = (sel) => el.querySelector(sel);
  const tabsEl = $('[data-module-tabs]');
  const instanceBtn = $('[data-shell-instance-toggle]');
  const releasePanel = $('[data-shell-release-panel]');
  const accountBtn = $('[data-open-account]');
  const accountPanel = $('[data-v4-account-panel]');
  const settingsPanel = $('[data-v4-settings-panel]');
  const backdrop = $('[data-v4-top-backdrop]');
  const syncBtn = $('[data-v4-sync]');
  const cleanups = [];
  const listen = (target, type, fn, opts) => { target.addEventListener(type, fn, opts); cleanups.push(() => target.removeEventListener(type, fn, opts)); };

  // ---- Workspace ----
  const identity = kernel?.identity || {};
  const wsName = workspaceName(identity, tx);
  $('[data-shell-instance-name]').textContent = wsName;
  $('[data-v4-workspace-title]').textContent = wsName;
  $('[data-v4-instance-id]').textContent = String(identity.instance_id || '');

  // ---- Konto ----
  const displayName = String(actor?.display_name || identity.display_name || identity.user_id || '');
  const role = String(actor?.role || '');
  $('[data-account-label]').textContent = displayName;
  if (displayName) {
    accountBtn.dataset.authenticated = 'true';
    accountBtn.dataset.avatar = initialsOf(displayName);
    accountBtn.setAttribute('aria-label', displayName);
    accountBtn.title = displayName;
  }
  $('[data-v4-account-sub]').textContent = [displayName, ROLE[role] || role].filter(Boolean).join(' · ');
  // Rolle und Abmelden nur, wenn der Kernel sie liefert (app.css setzt display an den Karten, daher entfernen statt hidden).
  if (role) { $('[data-v4-role-card]').hidden = false; $('[data-v4-role]').textContent = ROLE[role] || role; } else $('[data-v4-role-card]').remove();
  const logout = typeof identity.logout === 'function' ? identity.logout.bind(identity) : null;
  if (logout) $('[data-v4-account-actions]').hidden = false; else $('[data-v4-account-actions]').remove();

  // ---- Blätter (Konto, Einstellungen) und Workspace-Panel ----
  let lastFocus = null;
  function closeSheets({ restore = true } = {}) {
    const open = !accountPanel.hidden || !settingsPanel.hidden;
    accountPanel.hidden = true;
    settingsPanel.hidden = true;
    backdrop.hidden = true;
    accountBtn.setAttribute('aria-expanded', 'false');
    if (open && restore && lastFocus?.isConnected) lastFocus.focus({ preventScroll: true });
  }
  function closeRelease() { releasePanel.hidden = true; instanceBtn.setAttribute('aria-expanded', 'false'); }
  function openSheet(panel) {
    if (accountPanel.hidden && settingsPanel.hidden) lastFocus = document.activeElement;
    closeRelease();
    accountPanel.hidden = panel !== accountPanel;
    settingsPanel.hidden = panel !== settingsPanel;
    backdrop.hidden = false;
    accountBtn.setAttribute('aria-expanded', String(panel === accountPanel));
    if (panel === settingsPanel) syncSettings();
    (panel.querySelector('select, [data-v4-account-settings]') || panel.querySelector('button'))?.focus({ preventScroll: true });
  }
  const openSettings = () => openSheet(settingsPanel);
  const openAccount = () => openSheet(accountPanel);

  listen(el, 'click', (event) => {
    const t = event.target instanceof Element ? event.target : null;
    if (!t) return;
    if (t.closest('[data-shell-start]')) { closeSheets({ restore: false }); closeRelease(); onLauncher?.(); return; }
    if (t.closest('[data-open-settings]')) { openSettings(); return; }
    if (t.closest('[data-open-account]')) { if (accountPanel.hidden) openAccount(); else closeSheets(); return; }
    if (t.closest('[data-v4-top-close]') || t === backdrop) { closeSheets(); return; }
    if (t.closest('[data-shell-instance-toggle]')) {
      const open = releasePanel.hidden;
      releasePanel.hidden = !open;
      instanceBtn.setAttribute('aria-expanded', String(open));
      return;
    }
    if (t.closest('[data-logout]') && logout) { closeSheets({ restore: false }); logout(); return; }
    const tab = t.closest('.module-tab[data-v4-task-group]');
    if (tab) { const overflow = tab.closest('details'); if (overflow) overflow.open = false; closeWinMenu(); activate(tab); }
  });
  listen(document, 'pointerdown', (event) => {
    if (!releasePanel.hidden && !event.target?.closest?.('[data-shell-release]')) closeRelease();
    if (!winMenu.hidden && !winMenu.contains(event.target)) closeWinMenu();
    const ov = tabsEl.querySelector('details[open]');
    if (ov && !ov.contains(event.target)) ov.open = false;
  }, true);
  listen(document, 'keydown', (event) => {
    if (event.key !== 'Escape') return;
    if (!winMenu.hidden) { event.preventDefault(); event.stopPropagation(); closeWinMenu({ refocus: true }); return; }
    if (!accountPanel.hidden || !settingsPanel.hidden) { event.preventDefault(); event.stopPropagation(); closeSheets(); return; }
    if (!releasePanel.hidden) { event.preventDefault(); closeRelease(); instanceBtn.focus({ preventScroll: true }); return; }
    const ov = tabsEl.querySelector('details[open]');
    if (ov) { event.preventDefault(); ov.open = false; ov.querySelector('summary')?.focus({ preventScroll: true }); }
  }, true);

  // ---- Einstellungen: Sprache und Schema (shell_preferences.settings) ----
  const html = document.documentElement;
  const media = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
  const stored = () => { try { return prefs?.get?.()?.settings || {}; } catch { return {}; } };
  function applyTheme(choice) {
    if (choice === 'light' || choice === 'dark') { html.dataset.theme = choice; delete html.dataset.v4ThemeAuto; }
    else if (choice === 'system') { html.dataset.theme = media?.matches ? 'dark' : 'light'; html.dataset.v4ThemeAuto = '1'; }
  }
  function syncSettings() {
    const s = stored();
    settingsPanel.querySelector('[data-language-select]').value = s.locale === 'en' || (!s.locale && html.lang === 'en') ? 'en' : 'de';
    settingsPanel.querySelector('[data-theme-select]').value = s.theme || (html.dataset.v4ThemeAuto === '1' ? 'system' : (html.dataset.theme === 'dark' ? 'dark' : 'light'));
  }
  async function saveSetting(fields) {
    try { await prefs?.put?.({ settings: { ...stored(), ...fields } }); } catch { /* Kern lehnt ab: Auswahl bleibt für diese Sitzung */ }
  }
  listen(settingsPanel, 'change', (event) => {
    const sel = event.target;
    if (sel.matches('[data-theme-select]')) { applyTheme(sel.value); saveSetting({ theme: sel.value }); }
    if (sel.matches('[data-language-select]')) { html.lang = sel.value; saveSetting({ locale: sel.value }); }
  });
  if (stored().theme) applyTheme(stored().theme);
  const onMedia = () => { if (stored().theme === 'system') applyTheme('system'); };
  media?.addEventListener?.('change', onMedia);
  cleanups.push(() => media?.removeEventListener?.('change', onMedia));

  // ---- Zustandspunkt (ZONE45 sync-state.js) ----
  let syncOff = null;
  function paintSync(overall) {
    const p = syncPresentation(overall, tx);
    if (p.state) syncBtn.dataset.syncState = p.state; else delete syncBtn.dataset.syncState;
    syncBtn.title = p.title;
    syncBtn.setAttribute('aria-label', p.label || p.title);
    syncBtn.querySelector('[data-sync-state-label]').textContent = p.label;
  }
  function attachSync() {
    if (syncOff) return true;
    const src = typeof syncState === 'function' ? syncState() : syncState;
    if (!src?.subscribe) return false;
    // Gelb mit Text erst nach 2 s Dauer (dieselbe Regel wie am Meldungs-Badge, ZONE45): ein Abgleich nach jedem
    // Veröffentlichen dauert Millisekunden und soll keinen Zustandstext aufblitzen lassen. Grün und Rot sofort.
    let warnTimer = null;
    let shown = null;
    const show = (overall) => { shown = overall?.level ?? null; paintSync(overall); };
    const off = src.subscribe((overall) => {
      if (overall?.level === 'warn' && shown !== 'warn') {
        // Bis dahin neutral (grau, ohne Text): nie grün, solange die Quelle nicht „ok“ meldet.
        if (shown === 'ok') { shown = 'pending'; paintSync(null); }
        if (!warnTimer) warnTimer = setTimeout(() => { warnTimer = null; const now = src.getOverallState?.() ?? overall; if (now?.level === 'warn') show(now); }, SYNC_WARN_DELAY_MS);
        return;
      }
      if (warnTimer) { clearTimeout(warnTimer); warnTimer = null; }
      show(overall);
    });
    cleanups.push(() => { if (warnTimer) clearTimeout(warnTimer); });
    syncOff = typeof off === 'function' ? off : () => off?.unsubscribe?.();
    return true;
  }
  paintSync(null);
  let syncTimer = null;
  if (!attachSync()) {
    // Die Quelle entsteht ggf. nach der Kopfleiste; bis dahin grau. Kein Dauer-Polling, sobald sie gefunden ist.
    syncTimer = setInterval(() => { if (attachSync()) { clearInterval(syncTimer); syncTimer = null; } }, 1000);
  }

  // ---- App-Reiter (Shell-V2: ein Reiter je geöffneter App; Überlauf wie shared/topbar-apps.js) ----
  const overflow = document.createElement('details');
  overflow.className = 'module-overflow';
  const trigger = document.createElement('summary');
  trigger.className = 'module-overflow-trigger';
  const menu = document.createElement('div');
  menu.className = 'module-overflow-menu';
  menu.setAttribute('role', 'group');
  overflow.append(trigger, menu);
  const measure = document.createElement('div');
  measure.className = 'module-tabs-measure';
  measure.setAttribute('aria-hidden', 'true');
  let items = [];
  const tabCache = new Map(); // Eintragsschlüssel (App bzw. Gruppe) -> button
  let frame = null;
  let lastWidth = -1;
  function layout() {
    frame = null;
    lastWidth = tabsEl.clientWidth;
    if (!items.length) { tabsEl.replaceChildren(); return; }
    const focused = document.activeElement;
    const wasOpen = overflow.open;
    // Natürliche Breiten messen (ein früher geschrumpfter Reiter zählt wieder voll).
    for (const i of items) i.removeAttribute('data-v4-tab-shrink');
    measure.replaceChildren(...items, overflow);
    tabsEl.replaceChildren(measure);
    overflow.open = false;
    trigger.textContent = `+${items.length}`;
    const triggerWidth = trigger.getBoundingClientRect().width;
    const widths = items.map((i) => i.getBoundingClientRect().width);
    const gap = Number.parseFloat(getComputedStyle(tabsEl).columnGap) || 0;
    // FR6c: das aktive Fenster hat Vorrang vor dem Tastaturfokus und liegt nie im Überlauf.
    let priority = items.findIndex((i) => i.dataset.running === 'focused');
    if (priority < 0) priority = items.indexOf(focused);
    let visible = new Set(chooseVisibleTopbarApps(widths, tabsEl.clientWidth, { gap, overflowWidth: triggerWidth, priority }));
    // Passt der vordere Reiter nicht einmal allein neben „+N“ (schmale Breite, langer Name), bleibt nur er sichtbar und
    // schrumpft mit „…“ auf die freie Breite (frame.css, Kopfleiste) – die Leiste läuft nie über Punkt und Avatar.
    if (priority >= 0 && !visible.has(priority)) {
      visible = new Set([priority]);
      items[priority].setAttribute('data-v4-tab-shrink', '');
    }
    const hidden = items.filter((_, i) => !visible.has(i));
    menu.replaceChildren(...hidden);
    const nodes = items.filter((_, i) => visible.has(i));
    if (hidden.length) {
      trigger.textContent = `+${hidden.length}`;
      trigger.setAttribute('aria-label', tx.more(hidden.length));
      menu.setAttribute('aria-label', tx.moreMenu);
      nodes.push(overflow);
    }
    tabsEl.replaceChildren(...nodes);
    overflow.open = hidden.length > 0 && wasOpen;
    if (items.includes(focused) && focused.isConnected) focused.focus({ preventScroll: true });
  }
  const refresh = () => { if (frame === null) frame = requestAnimationFrame(layout); };
  const ro = typeof ResizeObserver === 'function' ? new ResizeObserver(() => { if (tabsEl.clientWidth !== lastWidth) refresh(); }) : null;
  ro?.observe(tabsEl);
  cleanups.push(() => { ro?.disconnect(); if (frame !== null) cancelAnimationFrame(frame); });

  // ---- Reiter = Einträge (FR6c): Klick, Fensterliste, Ziehen ----
  let entriesByKey = new Map();
  let lastAppById = new Map();
  const focusWin = (id) => (onFocusWindow ? onFocusWindow(id) : (onTab ? onTab(id) : onWindow?.(id)));
  function activate(tab) {
    const entry = entriesByKey.get(tab.dataset.v4TaskGroup);
    if (!entry) return;
    if (!entry.windows.length) { onPinned?.(entry.pins[0] || entry.appId, tab.querySelector('.module-tab-icon')); return; }
    const next = nextInGroup(entry.windows);
    if (next.toggle && onWindow) onWindow(next.id); else focusWin(next.id);
  }
  let menuFor = null;
  function closeWinMenu({ refocus = false } = {}) {
    if (winMenu.hidden) return;
    const key = menuFor;
    winMenu.hidden = true;
    winMenu.replaceChildren();
    menuFor = null;
    if (refocus) tabCache.get(key)?.focus({ preventScroll: true });
  }
  function openWinMenu(key, anchor) {
    closeWinMenu();
    const entry = entriesByKey.get(key);
    if (!entry?.windows.length) return;
    menuFor = key;
    for (const w of entry.windows) {
      const item = document.createElement('button');
      item.type = 'button';
      item.className = 'v4f-task-menu-item';
      item.setAttribute('role', 'menuitem');
      item.dataset.v4TaskMenuWin = w.id;
      item.classList.toggle('is-active', !!w.focused && w.state !== 'minimized');
      item.classList.toggle('is-minimized', w.state === 'minimized');
      item.innerHTML = '<span class="v4f-icon" data-v4-icon></span><span data-v4-menu-label></span>';
      const label = String(w.title || entry.title);
      item.querySelector('[data-v4-menu-label]').textContent = label;
      item.title = label;
      iconFor?.(lastAppById.get(w.appId) || { id: w.appId, title: w.title }, item.querySelector('[data-v4-icon]'));
      item.addEventListener('click', () => { closeWinMenu(); focusWin(w.id); });
      winMenu.append(item);
    }
    winMenu.hidden = false;
    // Unter dem Reiter (Leiste liegt oben), innerhalb des Fensters.
    const r = anchor.getBoundingClientRect();
    const m = winMenu.getBoundingClientRect();
    winMenu.style.left = `${Math.max(8, Math.min(r.left, innerWidth - m.width - 8))}px`;
    winMenu.style.top = `${Math.min(r.bottom + 4, Math.max(8, innerHeight - m.height - 8))}px`;
    (winMenu.querySelector('.is-active') || winMenu.firstElementChild)?.focus({ preventScroll: true });
  }
  listen(el, 'contextmenu', (event) => {
    const tab = event.target instanceof Element ? event.target.closest('.module-tab[data-v4-task-group]') : null;
    if (!tab || !entriesByKey.get(tab.dataset.v4TaskGroup)?.windows.length) return;
    event.preventDefault();
    const overflowed = tab.closest('details');
    if (overflowed) overflowed.open = false;
    const key = tab.dataset.v4TaskGroup;
    if (menuFor === key) closeWinMenu(); else openWinMenu(key, overflowed ? overflowed : tab);
  });
  listen(winMenu, 'keydown', (event) => {
    const list = [...winMenu.children];
    const i = list.indexOf(document.activeElement);
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const n = list.length;
      list[((i < 0 ? 0 : i) + (event.key === 'ArrowDown' ? 1 : n - 1)) % n]?.focus();
    } else if (event.key === 'Tab') closeWinMenu();
  });
  // Anheften per Ziehen (FR2-5): Launcher-Kachel (DRAG_APP) auf die Reiter heftet an (vor dem angehefteten Reiter
  // unter dem Zeiger, sonst ans Ende); ein angehefteter Reiter (DRAG_PIN) lässt sich umordnen oder auf die Fläche ziehen.
  const accepts = (event) => onPinDrop && (event.dataTransfer.types.includes(DRAG_APP) || event.dataTransfer.types.includes(DRAG_PIN));
  const pinUnder = (event) => event.target?.closest?.('.module-tab[data-v4-pin]') || null;
  const unmark = () => { tabsEl.classList.remove('is-drop-target'); for (const n of el.querySelectorAll('.is-drop-before')) n.classList.remove('is-drop-before'); };
  listen(tabsEl, 'dragover', (event) => {
    if (!accepts(event)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = event.dataTransfer.types.includes(DRAG_PIN) ? 'move' : 'copy';
    unmark();
    tabsEl.classList.add('is-drop-target');
    pinUnder(event)?.classList.add('is-drop-before');
  });
  listen(tabsEl, 'dragleave', (event) => { if (!tabsEl.contains(event.relatedTarget)) unmark(); });
  listen(tabsEl, 'drop', (event) => {
    if (!accepts(event)) return;
    event.preventDefault();
    const id = event.dataTransfer.getData(DRAG_PIN) || event.dataTransfer.getData(DRAG_APP);
    const before = pinUnder(event)?.dataset.v4Pin || null;
    unmark();
    if (id && id !== before) onPinDrop(id, before);
  });

  // model: {pins, apps, windows}. Fenster ohne App (Shell-Ansicht „Neue App“) bekommen ebenfalls einen Reiter, weil
  // die Reiterleiste der einzige Weg zu einem minimierten Fenster ist.
  function render({ pins = [], apps = [], windows = [] } = {}) {
    const appById = new Map(apps.map((a) => [a.id, a]));
    lastAppById = appById;
    const entries = buildEntries({ pins, windows, appById });
    entriesByKey = new Map(entries.map((e) => [e.key, e]));
    const next = entries.map((e) => {
      const n = e.windows.length;
      const lead = n ? (e.windows.find((w) => w.focused && w.state !== 'minimized') || e.windows.reduce((a, b) => ((b.z || 0) > (a.z || 0) ? b : a))) : null;
      const iconId = lead?.appId || e.pins[0] || e.appId;
      const app = appById.get(iconId) || { id: iconId, title: e.title };
      const focused = e.windows.some((w) => w.focused && w.state !== 'minimized');
      let tab = tabCache.get(e.key);
      if (!tab) {
        tab = document.createElement('button');
        tab.type = 'button';
        tab.className = 'module-tab';
        tab.dataset.module = '';
        tab.dataset.targetKind = 'app';
        tab.dataset.v4TaskGroup = e.key;
        tab.innerHTML = '<span class="module-tab-icon" aria-hidden="true"></span><span class="module-tab-label"></span><span class="module-tab-count" data-v4-count hidden></span>';
        tab.addEventListener('dragstart', (event) => {
          if (!tab.dataset.v4Pin) { event.preventDefault(); return; }
          event.dataTransfer.effectAllowed = 'move';
          event.dataTransfer.setData(DRAG_PIN, tab.dataset.v4Pin);
          event.dataTransfer.setData('text/plain', tab.dataset.v4Pin);
        });
        tab.addEventListener('dragend', unmark);
        tabCache.set(e.key, tab);
      }
      // Gruppe: Name der Gruppe („Crew“); sonst Name der App (Shell-V2).
      const label = e.grouped ? e.title : String(appById.get(e.appId)?.title || lead?.title || e.title);
      tab.dataset.target = e.appId;
      tab.dataset.v4TabApp = iconId;
      tab.dataset.v4TabWin = lead?.id || '';
      tab.dataset.v4WinIds = e.windows.map((w) => w.id).join(' ');
      tab.dataset.v4AppIds = [...new Set([...e.pins, ...e.windows.map((w) => w.appId)])].join(' ');
      tab.dataset.v4Count = String(n);
      if (n === 1) tab.dataset.v4Win = e.windows[0].id; else delete tab.dataset.v4Win;
      if (e.pins.length) { tab.dataset.pinned = 'true'; tab.dataset.v4Pin = e.pins[0]; tab.draggable = true; }
      else { delete tab.dataset.pinned; delete tab.dataset.v4Pin; tab.draggable = false; }
      if (n) { tab.dataset.running = focused ? 'focused' : 'true'; tab.setAttribute('aria-haspopup', 'menu'); }
      else { delete tab.dataset.running; tab.removeAttribute('aria-haspopup'); }
      tab.setAttribute('aria-current', focused ? 'page' : 'false');
      tab.title = n > 1 ? `${label} (${n})` : label;
      tab.setAttribute('aria-label', tab.title);
      const labelEl = tab.querySelector('.module-tab-label');
      if (labelEl.textContent !== label) labelEl.textContent = label;
      const count = tab.querySelector('[data-v4-count]');
      count.hidden = n < 2;
      if (count.textContent !== String(n)) count.textContent = String(n);
      const slot = tab.querySelector('.module-tab-icon');
      iconFor?.(app, slot);
      slot.firstElementChild?.classList.add('workjet-app-raster-icon');
      return tab;
    });
    for (const key of [...tabCache.keys()]) if (!entriesByKey.has(key)) tabCache.delete(key);
    if (menuFor && !entriesByKey.get(menuFor)?.windows.length) closeWinMenu();
    const same = next.length === items.length && next.every((t, i) => t === items[i]);
    const prio = items.findIndex((i) => i.dataset.running === 'focused');
    items = next;
    // Neu verteilen, wenn sich Reihe oder aktiver Reiter geändert haben (der aktive darf nie im Überlauf liegen),
    // sonst nur, wenn ein Reiter nicht mehr im Dokument hängt.
    const focusMoved = items.findIndex((i) => i.dataset.running === 'focused') !== prio;
    if (!same || focusMoved || !items.every((t) => t.isConnected) || items.some((t) => t.closest('details') && t.dataset.running === 'focused')) refresh();
  }

  const tabOfWin = (winId) => items.find((n) => (n.dataset.v4WinIds || '').split(' ').includes(String(winId))) || null;
  const tabOfApp = (appId) => items.find((n) => n.dataset.v4Pin === appId) || items.find((n) => (n.dataset.v4AppIds || '').split(' ').includes(String(appId))) || null;
  const anchorOf = (tab) => (tab ? (tab.closest('details') ? trigger : tab.querySelector('.module-tab-icon')) : null);

  return {
    element: el,
    render,
    pinAnchor(appId) { return anchorOf(tabOfApp(appId)); },
    winAnchor(winId) { return anchorOf(tabOfWin(winId)); },
    openSettings,
    openAccount,
    close: () => { closeSheets(); closeRelease(); },
    paintSync,
    destroy() {
      closeWinMenu();
      for (const fn of cleanups.splice(0)) { try { fn(); } catch { /* leer */ } }
      if (syncTimer) clearInterval(syncTimer);
      try { syncOff?.(); } catch { /* leer */ }
      syncOff = null;
      // data-v4-has-topbar bleibt: der abtretende Rahmen (Doppelpuffer) steht bis zum Umschalten unverändert sichtbar.
    },
  };
}
