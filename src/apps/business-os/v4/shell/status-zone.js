// Shell v4 – Zustandszone (Entwurf §7): der einzige Ort für Zustandsinfos eines App-Fensters.
//
// Regeln (fest):
//   - Ohne Einträge ist die Zone unsichtbar (`hidden`, 0 px): kein Platzhalter, kein Zähler.
//   - Mit Einträgen ein Badge am App-Icon (FR6a/M-14): Zahl der Meldungen, Farbe = schwerster Eintrag (error > warn >
//     info; bei Gleichstand der jüngste). Klick auf Badge oder Icon öffnet die Liste aller Einträge INNERHALB des
//     Fensters unter dem Icon (das Fenster begrenzt sie; nichts hängt außerhalb des Fensters im Dokument). Esc schließt.
//   - Aktion: ein Eintrag mit action {label, run} zeigt genau einen knappen Knopf am Eintrag (Zeile und Liste).
//     Klick ruft run(); endet run ohne Fehler (und liefert nicht false), verschwindet der Eintrag (entry.dismiss()
//     des Absenders, sonst clear(code)). Scheitert run, bleibt der Eintrag stehen. Keine weiteren Texte.
//   - Lebensdauer nach Stufe (Strang FR4, Owner-Regel „Erfolg ist still“): 'info' verschwindet nach 6 s von selbst
//     (INFO_TTL_MS); 'warn'/'error' bleiben, bis die Quelle sie räumt (clear) oder eine Aktion am Eintrag erfolgreich
//     war. Andere Stufen (auch 'success') zeigt die Zone nicht an. Ein abgelaufener info-Eintrag kommt mit identischem
//     Inhalt (Stufe, Text, ref) nicht wieder (der Kernel-Strom liefert ihn bei jeder Änderung erneut); neuer Text oder
//     clear(code) heben das auf. Beim Ablauf ruft die Zone entry.dismiss() des Absenders (räumt z. B. den Kernel-Eintrag).
//   - Quittieren (Strang FR4): jeder 'warn'/'error'-Eintrag ohne eigene Aktion bekommt die Zonen-Aktion „Ausblenden“
//     (generisch, keine App-Sonderfälle; z. B. Befehlsfehler command:<id> aus commandLifecycle). Ausblenden ist je Tab
//     (sessionStorage je App, überlebt Shell-Tausch und Remount): derselbe Eintrag (code + Stufe + Text + ref) bleibt
//     ausgeblendet, auch wenn der Kernel-Strom ihn erneut liefert. Räumt die Quelle den Code (clear), verfällt die
//     Quittung; ein späteres Wiederauftreten wird wieder gezeigt. Der Kernel-Eintrag selbst bleibt unberührt.
//   - Quellen: api.status.report/clear der App (source 'app'); Aktivierungszustände; Messwerte; Shell-weite Zustände.
//     Die Filter (2-s-Regel, Messwert-Schwellen) liegen in diesem Modul, damit jede Quelle dieselben Regeln hat.
//
// Exporte:
//   createStatusZone(slotEl, listHost) → { el, list, report(item), clear(code), clearSource(source), entries(), destroy() }
//     slotEl = vom Rahmen bereitgestellter Zonen-Platz im Fenster-Chrome, listHost = Fensterelement (begrenzt die Liste)
//   createActivationFilter(sink)    → { onActivation({state, ver, error}), destroy() }   (2-s-Regel, failed sofort)
//   createMetricsWatcher(metrics, sink) → { destroy() }        (nur auffällige Messwerte; Verbindung: attachSyncState)
//   attachSyncState(zone, syncState, {appId, collections, warnDelayMs}) → detach()   (ZONE45, §16.45, siehe unten)
//   sink = { report(item), clear(code) }

import { SYNC_CODE, SYNC_TEXT } from './sync-state.js';

const LEVEL_WEIGHT = { error: 3, warn: 2, info: 1 };
// App-Menü am Icon-Header (FR6a, Owner 15.10.): der Rahmen hinterlegt je Fensterelement eine Funktion, die die
// Menüpunkte liefert ([{id, label, hint?, run}] – z. B. „Code ansehen“ / „Zurück zur App“). Sie stehen über den
// Meldungen in derselben Liste unter dem Icon; ohne Menüpunkte und ohne Meldungen öffnet ein Klick aufs Icon nichts.
export const APP_MENU = new WeakMap();
export const INFO_TTL_MS = 6000;
export const HIDE_LABEL = 'Ausblenden';
const ACK_PREFIX = 'ctox-v4-zone-ack:';
const ACK_MAX = 200;
// §16.19 Nr. 7: metrics.link.state ∈ online | offline (kernel-adapter.js bildet ältere Werte darauf ab).

export function createStatusZone(slotEl, listHost = slotEl?.closest?.('.shell-window') || slotEl?.parentElement, { infoTtlMs = INFO_TTL_MS, ackScope = listHost?.dataset?.windowApp || null } = {}) {
  // FR6a / M-14 (Owner 15.10.): Meldungen hängen am App-Icon. Badge = Zahl offener Meldungen, Farbe nach der höchsten
  // Schwere; ohne Meldungen kein Badge. Klick auf Icon oder Badge klappt die Liste unter dem Icon auf (im Fenster
  // verankert, Esc schließt). Neue Meldung: kurzer Puls, die Liste öffnet sich nicht von selbst. Je Meldung Text, die
  // Aktion des Absenders (falls vorhanden) und „Ausblenden“.
  const el = document.createElement('div');
  el.className = 'v4-status-zone';
  el.hidden = true;
  const badge = document.createElement('button');
  badge.type = 'button';
  badge.className = 'v4-status-zone__badge';
  badge.setAttribute('aria-haspopup', 'true');
  badge.setAttribute('aria-expanded', 'false');
  // Text der schwersten Meldung (für Vorleser und Tooltip); sichtbar ist nur die Zahl.
  const line = document.createElement('span');
  line.className = 'v4-status-zone__top v4-status-zone__line';
  el.append(badge, line);

  // Liste: Kind des Fensters (nicht der Zone), damit sie über dem Inhalt liegt und vom Fenster begrenzt wird.
  const list = document.createElement('div');
  list.className = 'v4-status-zone__list';
  list.hidden = true;
  list.setAttribute('role', 'log');
  // Den Platz für Zone und Liste stellt der Rahmen bereit (Fenster-Chrome gehört v4/frame).
  slotEl.append(el);
  listHost.append(list);

  const entries = new Map(); // code → {level, code, text, ref, source, at, action?, dismiss?}
  const timers = new Map(); // code → Ablauf-Timer eines info-Eintrags
  const expired = new Map(); // code → Signatur des abgelaufenen info-Eintrags (kein Wiederaufleben bei gleichem Inhalt)
  const sig = (e) => `${e.level}\u0000${e.text}\u0000${e.ref ?? ''}`;
  // Quittungen je Tab: Schlüssel `${code}\u0001${sig}`; ohne Bereich (oder ohne sessionStorage) nur im Speicher.
  const ackKey = ackScope ? ACK_PREFIX + ackScope : null;
  const acked = new Set((() => { try { const v = ackKey && JSON.parse(sessionStorage.getItem(ackKey) || '[]'); return Array.isArray(v) ? v : []; } catch { return []; } })());
  const saveAcks = () => { if (!ackKey) return; try { if (acked.size) sessionStorage.setItem(ackKey, JSON.stringify([...acked].slice(-ACK_MAX))); else sessionStorage.removeItem(ackKey); } catch { /* kein Speicher */ } };
  const ackId = (e) => `${e.code}\u0001${sig(e)}`;
  function dropAcks(code) {
    let changed = false;
    for (const k of acked) if (k.startsWith(`${code}\u0001`)) { acked.delete(k); changed = true; }
    if (changed) saveAcks();
  }
  function stopTimer(code) { const t = timers.get(code); if (t) { clearTimeout(t); timers.delete(code); } }
  function expire(code) {
    timers.delete(code);
    const e = entries.get(code);
    if (!e) return;
    expired.set(code, sig(e));
    entries.delete(code);
    render();
    try { e.dismiss?.(); } catch { /* Absender weg */ }
  }

  // Aktion des Absenders: run(); endet sie ohne Fehler (und nicht false), verschwindet der Eintrag (dismiss des
  // Absenders, sonst lokal). Scheitert sie, bleibt er stehen.
  function actionButton(e) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'v4-status-zone__action';
    b.dataset.code = e.code;
    b.textContent = String(e.action.label);
    b.addEventListener('click', async (ev) => {
      ev.stopPropagation();
      const cur = entries.get(e.code);
      if (!cur?.action || b.disabled) return;
      b.disabled = true;
      let ok = false;
      try { ok = (await cur.action.run()) !== false; } catch (err) { console.warn('[status-zone] action', e.code, err); }
      b.disabled = false;
      const now = entries.get(e.code);
      if (!ok || !now) return;
      try { if (typeof now.dismiss === 'function') now.dismiss(); } catch { /* Absender weg */ }
      if (entries.has(e.code)) { stopTimer(e.code); entries.delete(e.code); render(); }
    });
    return b;
  }
  // „Ausblenden“ an jeder Meldung: info läuft sofort ab; warn/error werden je Tab quittiert (Strom-Wiederholung und
  // Shell-Tausch bleiben still), der Kernel-Eintrag selbst bleibt unberührt.
  function hideButton(e) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'v4-status-zone__action v4-status-zone__action--hide';
    b.dataset.code = e.code;
    b.dataset.hide = '';
    b.textContent = HIDE_LABEL;
    b.addEventListener('click', (ev) => {
      ev.stopPropagation();
      const now = entries.get(e.code);
      if (!now) return;
      if (now.level === 'info') { stopTimer(e.code); expire(e.code); return; }
      acked.add(ackId(now)); saveAcks();
      stopTimer(e.code); entries.delete(e.code); render();
    });
    return b;
  }

  function heaviest() {
    let best = null;
    for (const e of entries.values()) {
      if (!best || LEVEL_WEIGHT[e.level] > LEVEL_WEIGHT[best.level] || (LEVEL_WEIGHT[e.level] === LEVEL_WEIGHT[best.level] && e.at >= best.at)) best = e;
    }
    return best;
  }
  let pulseTimer = null;
  function pulse() {
    el.classList.remove('is-pulse');
    void el.offsetWidth;
    el.classList.add('is-pulse');
    clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => el.classList.remove('is-pulse'), 1300);
  }
  function render() {
    const top = heaviest();
    if (!top) {
      el.hidden = true;
      if (!menuItems().length) close(); else if (!list.hidden) renderList();
      line.textContent = '';
      badge.textContent = '';
      delete el.dataset.level;
      return;
    }
    el.hidden = false;
    el.dataset.level = top.level;
    const n = entries.size;
    badge.textContent = String(n);
    badge.title = top.text;
    badge.setAttribute('aria-label', `${n} ${n === 1 ? 'Meldung' : 'Meldungen'}: ${top.text}`);
    line.textContent = top.text;
    if (!list.hidden) renderList();
  }
  const menuItems = () => { try { return APP_MENU.get(listHost)?.() || []; } catch { return []; } };
  function renderList() {
    const rows = [...entries.values()].sort((a, b) => LEVEL_WEIGHT[b.level] - LEVEL_WEIGHT[a.level] || b.at - a.at);
    const items = menuItems();
    let menu = null;
    if (items.length) {
      menu = document.createElement('div');
      menu.className = 'v4f-appmenu';
      menu.setAttribute('role', 'menu');
      for (const it of items) {
        const b = document.createElement('button');
        b.type = 'button';
        b.setAttribute('role', 'menuitem');
        b.dataset.v4Appmenu = it.id;
        b.textContent = it.label;
        if (it.hint) { const k = document.createElement('kbd'); k.textContent = it.hint; b.append(k); }
        b.addEventListener('click', (ev) => { ev.stopPropagation(); close(); try { it.run(); } catch (err) { console.warn('[status-zone] menu', it.id, err); } });
        menu.append(b);
      }
    }
    list.replaceChildren(...(menu ? [menu] : []), ...rows.map((e) => {
      const row = document.createElement('div');
      row.className = 'v4-status-zone__item';
      row.dataset.level = e.level;
      row.dataset.code = e.code;
      const text = document.createElement('span');
      text.textContent = e.text;
      row.append(text);
      if (e.action) row.append(actionButton(e));
      row.append(hideButton(e));
      return row;
    }));
  }
  function open() {
    if (!entries.size && !menuItems().length) return false;
    renderList();
    list.hidden = false;
    badge.setAttribute('aria-expanded', 'true');
    return true;
  }
  function close() {
    list.hidden = true;
    badge.setAttribute('aria-expanded', 'false');
  }
  const toggle = () => (list.hidden ? open() : (close(), false));
  badge.addEventListener('click', (ev) => { ev.stopPropagation(); toggle(); });
  // Klick auf das App-Icon klappt die Liste ebenfalls auf (nur mit Meldungen; ein Ziehen am Icon ist kein Klick).
  const icon = listHost?.querySelector?.('.shell-window-v2-icon') || null;
  let downAt = null;
  const onIconDown = (ev) => { downAt = { x: ev.clientX, y: ev.clientY }; };
  const onIconClick = (ev) => {
    const moved = downAt && Math.hypot(ev.clientX - downAt.x, ev.clientY - downAt.y) > 4;
    downAt = null;
    if (!moved && (entries.size || menuItems().length)) toggle();
  };
  icon?.addEventListener('pointerdown', onIconDown);
  icon?.addEventListener('click', onIconClick);
  // Esc schließt die offene Liste (auch wenn der Fokus nach dem Klick auf das Icon nicht im Fenster liegt).
  const onKey = (ev) => { if (ev.key === 'Escape' && !list.hidden) { ev.stopPropagation(); close(); badge.focus({ preventScroll: true }); } };
  document.addEventListener('keydown', onKey, true);
  const onOutside = (ev) => { if (!list.hidden && !list.contains(ev.target) && !el.contains(ev.target) && !icon?.contains(ev.target)) close(); };
  document.addEventListener('pointerdown', onOutside, true);

  return {
    el,
    list,
    badge,
    open,
    close,
    toggle,
    report({ level = 'info', code, text, ref, source = 'app', action = null, dismiss = null }) {
      if (!code || !LEVEL_WEIGHT[level]) return;
      const own = action && typeof action.run === 'function' && String(action.label ?? '').trim() ? action : null;
      const next = { level, code, text: String(text ?? code), ref, source, at: performance.now(), action: own, dismiss: typeof dismiss === 'function' ? dismiss : null };
      const s = sig(next);
      if (level === 'info' && expired.get(code) === s) return;
      if (level !== 'info' && acked.has(ackId(next))) { stopTimer(code); if (entries.delete(code)) render(); return; }
      expired.delete(code);
      const prev = entries.get(code);
      entries.set(code, next);
      // Gleicher info-Inhalt erneut gemeldet (Strom): Frist läuft weiter, sonst neu ab jetzt; andere Stufen ohne Frist.
      if (level !== 'info') stopTimer(code);
      else if (!(prev && timers.has(code) && sig(prev) === s)) { stopTimer(code); timers.set(code, setTimeout(() => expire(code), infoTtlMs)); }
      render();
      if (!prev || sig(prev) !== s) pulse();
    },
    clear(code) { stopTimer(code); expired.delete(code); dropAcks(code); if (entries.delete(code)) render(); },
    clearSource(source) {
      let changed = false;
      for (const [code, e] of entries) if (e.source === source) { stopTimer(code); dropAcks(code); entries.delete(code); changed = true; }
      if (changed) render();
    },
    // action: eigene Aktion des Absenders, sonst „Ausblenden“ (warn/error) bzw. null (info) – wie bisher.
    entries: () => [...entries.values()].map(({ dismiss, ...e }) => ({ ...e, action: e.action ? { label: e.action.label } : (e.level === 'info' ? null : { label: HIDE_LABEL }) })),
    destroy() {
      for (const t of timers.values()) clearTimeout(t);
      timers.clear(); expired.clear(); clearTimeout(pulseTimer);
      icon?.removeEventListener('pointerdown', onIconDown);
      icon?.removeEventListener('click', onIconClick);
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('pointerdown', onOutside, true);
      el.remove(); list.remove(); entries.clear();
    },
  };
}

// Aktivierungszustände: received/fetched/data-ready erst sichtbar, wenn der Zustand > 2 s anhält; failed sofort.
export function createActivationFilter(sink, { delayMs = 2000 } = {}) {
  const CODE = 'shell.activation';
  let timer = null;
  const LABEL = { received: 'Neue Version empfangen', fetched: 'Neue Version geladen, Daten werden vorbereitet', 'data-ready': 'Neue Version wird aktiviert' };
  return {
    onActivation({ state, ver, error }) {
      clearTimeout(timer);
      timer = null;
      if (state === 'failed') {
        sink.report({ level: 'error', code: CODE, text: `Version ${ver} konnte nicht aktiviert werden${error ? `: ${error}` : ''}`, source: 'activation' });
        return;
      }
      sink.clear(CODE);
      if (LABEL[state]) {
        timer = setTimeout(() => sink.report({ level: 'info', code: CODE, text: `${LABEL[state]} (${ver})`, source: 'activation' }), delayMs);
      }
    },
    destroy() { clearTimeout(timer); },
  };
}

// Messwerte nur, wenn auffällig: älteste Outbox-Änderung > 10 s, Rückstand wächst > 10 s.
// „Verbindung weg“ meldet allein der Sync-Zustand (sync-state.js + attachSyncState, ZONE45): eine Quelle, ein Eintrag.
export function createMetricsWatcher(metrics, sink, { outboxMs = 10000, backlogMs = 10000 } = {}) {
  let backlogTimer = null;
  let lastBacklog = null;
  let growingSince = null;
  const unsub = metrics.subscribe((m) => {
    if (!m) return;
    // Outbox
    if ((m.outbox?.count || 0) > 0 && (m.outbox?.oldest_ms || 0) > outboxMs) {
      sink.report({ level: 'warn', code: 'metrics.outbox', text: `${m.outbox.count} Änderungen noch nicht übertragen`, source: 'metrics' });
    } else sink.clear('metrics.outbox');
    // Rückstand wächst
    const b = m.backlog || 0;
    if (lastBacklog !== null && b > lastBacklog) {
      if (growingSince === null) {
        growingSince = performance.now();
        backlogTimer = setTimeout(() => sink.report({ level: 'warn', code: 'metrics.backlog', text: 'Rückstand beim Abgleich wächst', source: 'metrics' }), backlogMs);
      }
    } else if (lastBacklog !== null && b < lastBacklog) {
      growingSince = null;
      clearTimeout(backlogTimer);
      sink.clear('metrics.backlog');
    }
    lastBacklog = b;
  });
  return { destroy() { unsub(); clearTimeout(backlogTimer); } };
}

// ZONE45 (§16.45, Owner-Mangel M-08): ehrlicher Verbindungszustand der App-Collections am Meldungs-Badge.
//   warn  (catching_up/offline)  → „Verbindung wird wiederhergestellt“, erst wenn der Zustand > warnDelayMs anhält
//                                  (dieselbe 2-s-Regel wie Aktivierungszustände: ein normaler Abgleich nach einer
//                                  Änderung dauert Millisekunden und ist kein Verbindungsproblem)
//   error                        → „Verbindung unterbrochen“, sofort
//   Aktion „Neu verbinden“ → syncState.reconnect() (nur wenn der Kernel reconnect hat; V3 ist nur lesend).
//   ok/unknown → Eintrag verschwindet still (unknown ist kein Alarm; Grün zeigt der Zustandspunkt trotzdem nicht).
// Räumt sich selbst, wenn die Zone zerstört wird (Fenster zu, Shell-Tausch) oder der Sync-Zustand zerstört wird.
//   Ist der Grund nur „Verbindung weg“ (alle offenen Collections offline bzw. nur metrics.link), gilt die bisherige
//   5-s-Regel der Messwerte (offlineDelayMs): kurze Netzaussetzer bleiben still. Das ist die EINZIGE Offline-Meldung;
//   createMetricsWatcher meldet die Verbindung nicht mehr (ZONE45 Runde 2).
export function attachSyncState(zone, syncState, { appId, collections, warnDelayMs = 2000, offlineDelayMs = 5000 } = {}) {
  if (!zone || !syncState?.subscribe) return () => {};
  let shown = null; // Stufe des gezeigten Eintrags
  let timer = null;
  const action = syncState.canReconnect ? { label: SYNC_TEXT.action, run: () => syncState.reconnect() } : null;
  const show = (level) => {
    if (shown === level) return;
    shown = level;
    zone.report({ level, code: SYNC_CODE, text: SYNC_TEXT[level], source: 'shell', action, dismiss: () => { shown = null; } });
  };
  const hide = () => {
    clearTimeout(timer); timer = null;
    if (shown) { shown = null; zone.clear(SYNC_CODE); }
  };
  const apply = () => {
    let st;
    try { st = syncState.getAppState(appId, collections); } catch { st = { level: 'unknown' }; }
    if (st.level === 'error') { clearTimeout(timer); timer = null; show('error'); return; }
    if (st.level === 'warn') {
      if (shown === 'warn' || timer) return;
      if (shown === 'error') { show('warn'); return; }
      const offlineOnly = st.source === 'link' || (st.pending.length > 0 && st.pending.every((n) => st.collections?.[n] === 'offline')) || (!st.pending.length && !st.errors.length);
      const delay = offlineOnly ? offlineDelayMs : warnDelayMs;
      if (delay <= 0) { show('warn'); return; }
      timer = setTimeout(() => {
        timer = null;
        let now;
        try { now = syncState.getAppState(appId, collections); } catch { now = { level: 'unknown' }; }
        if (now.level === 'warn') show('warn'); else apply();
      }, delay);
      return;
    }
    hide();
  };
  const off = syncState.subscribe(apply, { app: true });
  let detached = false;
  const destroyZone = zone.destroy;
  const detach = () => {
    if (detached) return;
    detached = true;
    off();
    clearTimeout(timer); timer = null;
    if (zone.destroy === wrapped) zone.destroy = destroyZone;
  };
  function wrapped(...a) { detach(); return destroyZone?.apply(zone, a); }
  if (typeof destroyZone === 'function') zone.destroy = wrapped;
  return () => { const had = shown; detach(); if (had) { shown = null; zone.clear(SYNC_CODE); } };
}
