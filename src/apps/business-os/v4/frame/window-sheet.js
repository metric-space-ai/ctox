// Shell v4 – Blatt und Menü INNERHALB eines App-Fensters (Rahmen-Standardschnittstelle für api.shell.dialog/menu).
//
// Owner-Regel „nichts außerhalb der App“: kein Shell-Overlay, kein Dokument-Popup. Das Blatt hängt im Fensterelement,
// deckt nur dessen Inhaltsbereich ab und ist nur für dieses Fenster modal (Inhalt inert, andere Fenster bleiben
// bedienbar). Tastatur: Enter bestätigt, Esc bricht ab, Tab bleibt im Blatt (Fokusfalle), danach Fokus zurück.
//
//   createWindowSheet({ host, inertTargets?, onOpen?, document? })   onOpen: Rahmen holt das Fenster nach vorn
//     → { dialog(spec): Promise, menu(spec): Promise<string|null>, cancelAll(), isOpen(), destroy() }
//
// dialog(spec) – Form offen im Stub (OFFEN-A), angenommen werden alle Formen der portierten Apps:
//   kind        'confirm' (Standard) | 'prompt' | 'alert' | 'select' (Aliase 'choice', 'choose')
//   title, text Überschrift und Text (Zeilenumbrüche bleiben)
//   confirm_label / cancel_label   Standard „OK“ bzw. „Löschen“ (destruktiv) und „Abbrechen“
//   danger | tone:'danger' | destructive   Bestätigen-Knopf rot
//   require_text   Bestätigen erst, wenn genau dieser Text eingegeben ist
//   prompt:  value | default (Startwert, Zeichenkette), placeholder, secret (Passwortfeld), multiline, required
//   select:  options | choices | items: [string | {value|id, label, disabled}], value | default (vorgewählt)
// Rückgabe: confirm → true | false, prompt → Zeichenkette | null, select → Wert | null, alert → true.
// Mehrere Aufrufe laufen nacheinander (Warteschlange je Fenster).
//
// menu(spec): {items: [{id, label, icon?, disabled?, danger?} | {type:'separator'}], x?, y? | at?: {x, y}}
//   (Bildschirmkoordinaten wie event.clientX/Y) → id des gewählten Eintrags oder null (Esc, Klick daneben).

const DIALOG_KINDS = new Map([['confirm', 'confirm'], ['prompt', 'prompt'], ['alert', 'alert'], ['select', 'select'], ['choice', 'select'], ['choose', 'select']]);
const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export const SHEET_LABELS = Object.freeze({ ok: 'OK', cancel: 'Abbrechen', delete: 'Löschen', confirmText: 'Zur Bestätigung „{text}“ eingeben' });

const str = (v) => (v == null ? '' : String(v));

export function dialogKind(spec) {
  return DIALOG_KINDS.get(str(spec?.kind).toLowerCase()) || 'confirm';
}

/** Antwort beim Abbrechen (Esc, Fenster schließt, App wird abgebaut). */
export function cancelValueOf(spec) {
  const kind = dialogKind(spec);
  if (kind === 'confirm') return false;
  if (kind === 'alert') return true;
  return null;
}

function isDanger(spec) {
  return spec?.danger === true || spec?.destructive === true || str(spec?.tone).toLowerCase() === 'danger';
}

function optionsOf(spec) {
  const raw = Array.isArray(spec?.options) ? spec.options : Array.isArray(spec?.choices) ? spec.choices : Array.isArray(spec?.items) ? spec.items : [];
  return raw.map((o) => (o && typeof o === 'object'
    ? { value: o.value ?? o.id ?? o.label, label: str(o.label ?? o.value ?? o.id), disabled: !!o.disabled }
    : { value: o, label: str(o), disabled: false })).filter((o) => o.label !== '' || o.value != null);
}

export function createWindowSheet({ host, inertTargets = null, onOpen = null, document: doc = globalThis.document } = {}) {
  if (!host) throw new Error('createWindowSheet: host fehlt');
  let queue = Promise.resolve();
  let active = null; // {overlay, finish(value)}
  let menuState = null; // {el, finish(value)}
  let destroyed = false;
  const pending = new Set(); // offene Dialog-Aufrufe (auch wartende): {spec, resolve}

  const ensurePositioned = () => {
    try { if (getComputedStyle(host).position === 'static') host.style.position = 'relative'; } catch { /* kein Layout */ }
  };

  const defaultInert = () => [...host.children].filter((el) => !el.matches?.('[data-v4-sheet-overlay], [data-v4-menu]'));
  function setInert(on, list) {
    for (const el of list) {
      if (on) { if (!el.inert) { el.inert = true; el.dataset.v4SheetInert = ''; } }
      else if ('v4SheetInert' in el.dataset) { el.inert = false; delete el.dataset.v4SheetInert; }
    }
  }

  function build(spec) {
    const kind = dialogKind(spec);
    const danger = isDanger(spec);
    const overlay = doc.createElement('div');
    overlay.className = 'v4-sheet-overlay';
    overlay.dataset.v4SheetOverlay = '';
    const sheet = doc.createElement('div');
    sheet.className = 'v4-sheet';
    sheet.dataset.v4Sheet = kind;
    sheet.setAttribute('role', kind === 'alert' ? 'alertdialog' : 'dialog');
    sheet.setAttribute('aria-modal', 'true');
    const uid = `v4-sheet-${Math.random().toString(36).slice(2, 9)}`;
    const title = str(spec?.title).trim();
    const text = str(spec?.text ?? spec?.message).trim();
    if (title) {
      const h = doc.createElement('h2');
      h.className = 'v4-sheet-title';
      h.id = `${uid}-t`;
      h.textContent = title;
      sheet.append(h);
      sheet.setAttribute('aria-labelledby', h.id);
    }
    if (text && text !== title) {
      const p = doc.createElement('p');
      p.className = 'v4-sheet-text';
      p.id = `${uid}-d`;
      p.textContent = text;
      sheet.append(p);
      sheet.setAttribute('aria-describedby', p.id);
      if (!title) sheet.setAttribute('aria-labelledby', p.id);
    } else if (!title && text) {
      sheet.setAttribute('aria-label', text);
    }
    let input = null;
    if (kind === 'prompt') {
      input = doc.createElement(spec?.multiline ? 'textarea' : 'input');
      input.className = 'v4-sheet-input';
      input.dataset.v4SheetInput = '';
      if (!spec?.multiline) input.type = spec?.secret ? 'password' : 'text';
      else input.rows = 4;
      input.autocomplete = 'off';
      input.spellcheck = !spec?.secret;
      const start = spec?.value ?? (typeof spec?.default === 'string' || typeof spec?.default === 'number' ? spec.default : '');
      input.value = str(start);
      if (spec?.placeholder) input.placeholder = str(spec.placeholder);
      input.setAttribute('aria-label', title || text || 'Eingabe');
      sheet.append(input);
    }
    let group = null;
    const options = kind === 'select' ? optionsOf(spec) : [];
    if (kind === 'select') {
      group = doc.createElement('div');
      group.className = 'v4-sheet-options';
      group.dataset.v4SheetOptions = '';
      group.setAttribute('role', 'radiogroup');
      if (title) group.setAttribute('aria-labelledby', `${uid}-t`);
      const want = spec?.value ?? spec?.default;
      let checked = options.findIndex((o) => !o.disabled && o.value === want);
      if (checked < 0) checked = options.findIndex((o) => !o.disabled);
      options.forEach((o, i) => {
        const label = doc.createElement('label');
        label.className = 'v4-sheet-option';
        const radio = doc.createElement('input');
        radio.type = 'radio';
        radio.name = `${uid}-o`;
        radio.value = String(i);
        radio.disabled = o.disabled;
        radio.checked = i === checked;
        radio.dataset.v4SheetOption = str(o.value);
        const span = doc.createElement('span');
        span.textContent = o.label;
        label.append(radio, span);
        group.append(label);
      });
      sheet.append(group);
    }
    let confirmInput = null;
    const required = str(spec?.require_text);
    if (required) {
      const label = doc.createElement('label');
      label.className = 'v4-sheet-require';
      const span = doc.createElement('span');
      span.textContent = SHEET_LABELS.confirmText.replace('{text}', required);
      confirmInput = doc.createElement('input');
      confirmInput.className = 'v4-sheet-input';
      confirmInput.type = 'text';
      confirmInput.autocomplete = 'off';
      confirmInput.dataset.v4SheetRequire = '';
      label.append(span, confirmInput);
      sheet.append(label);
    }
    const actions = doc.createElement('div');
    actions.className = 'v4-sheet-actions';
    let cancel = null;
    if (kind !== 'alert') {
      cancel = doc.createElement('button');
      cancel.type = 'button';
      cancel.className = 'v4-sheet-button';
      cancel.dataset.v4SheetCancel = '';
      cancel.textContent = str(spec?.cancel_label).trim() || SHEET_LABELS.cancel;
      actions.append(cancel);
    }
    const ok = doc.createElement('button');
    ok.type = 'button';
    ok.className = `v4-sheet-button ${danger ? 'is-danger' : 'is-primary'}`;
    ok.dataset.v4SheetOk = '';
    ok.textContent = str(spec?.confirm_label ?? spec?.ok_label).trim() || (danger ? SHEET_LABELS.delete : SHEET_LABELS.ok);
    actions.append(ok);
    sheet.append(actions);
    overlay.append(sheet);
    return { kind, overlay, sheet, input, group, options, confirmInput, required, cancel, ok };
  }

  function show(spec, resolve) {
    const ui = build(spec);
    const { kind, overlay, sheet, input, group, options, confirmInput, required, cancel, ok } = ui;
    const prevFocus = doc.activeElement;
    const inertList = (typeof inertTargets === 'function' ? inertTargets() : inertTargets) || defaultInert();
    const valid = () => {
      if (required && str(confirmInput?.value).trim() !== required) return false;
      if (kind === 'prompt' && spec?.required && !str(input?.value).trim()) return false;
      if (kind === 'select' && !group?.querySelector('input:checked')) return false;
      return true;
    };
    const sync = () => { ok.disabled = !valid(); };
    let done = false;
    const finish = (value) => {
      if (done) return;
      done = true;
      active = null;
      overlay.removeEventListener('keydown', onKey);
      setInert(false, inertList);
      overlay.remove();
      if (prevFocus?.isConnected && host.contains(prevFocus) && typeof prevFocus.focus === 'function') {
        try { prevFocus.focus({ preventScroll: true }); } catch { /* egal */ }
      }
      resolve(value);
    };
    const accept = () => {
      if (!valid()) return;
      if (kind === 'confirm') finish(true);
      else if (kind === 'alert') finish(true);
      else if (kind === 'prompt') finish(str(input.value));
      else {
        const picked = group.querySelector('input:checked');
        finish(picked ? options[Number(picked.value)].value : null);
      }
    };
    const onKey = (ev) => {
      if (ev.key === 'Escape') {
        ev.preventDefault();
        ev.stopPropagation();
        finish(cancelValueOf(spec));
      } else if (ev.key === 'Enter' && !ev.isComposing) {
        const t = ev.target;
        if (t?.tagName === 'BUTTON') return; // nativ: der fokussierte Knopf
        if (t?.tagName === 'TEXTAREA' && !(ev.metaKey || ev.ctrlKey)) return;
        ev.preventDefault();
        ev.stopPropagation();
        accept();
      } else if (ev.key === 'Tab') {
        const list = [...sheet.querySelectorAll(FOCUSABLE)].filter((el) => !el.disabled && el.offsetParent !== null);
        if (!list.length) { ev.preventDefault(); return; }
        const i = list.indexOf(doc.activeElement);
        const next = ev.shiftKey ? (i <= 0 ? list[list.length - 1] : list[i - 1]) : (i < 0 || i === list.length - 1 ? list[0] : list[i + 1]);
        ev.preventDefault();
        next.focus();
      }
    };
    overlay.addEventListener('keydown', onKey);
    // Klick auf die Abdeckung: Fokus zurück ins Blatt (kein Schließen, wie macOS-Blätter).
    overlay.addEventListener('mousedown', (ev) => { if (ev.target === overlay) { ev.preventDefault(); (input || ok).focus(); } });
    ok.addEventListener('click', accept);
    cancel?.addEventListener('click', () => finish(cancelValueOf(spec)));
    input?.addEventListener('input', sync);
    confirmInput?.addEventListener('input', sync);
    group?.addEventListener('change', sync);
    group?.addEventListener('dblclick', (ev) => { if (ev.target.closest('label')) accept(); });
    closeMenu(null);
    ensurePositioned();
    host.append(overlay);
    setInert(true, inertList);
    sync();
    active = { overlay, finish, spec };
    try { onOpen?.(); } catch { /* Fenster nach vorn ist optional */ }
    const first = confirmInput || input || group?.querySelector('input:checked') || (ok.disabled ? cancel : ok);
    first?.focus({ preventScroll: true });
    if (input && !input.disabled && input.select) input.select();
  }

  function dialog(spec = {}) {
    if (destroyed) return Promise.resolve(cancelValueOf(spec));
    return new Promise((resolve) => {
      const entry = { spec, resolve, started: false };
      pending.add(entry);
      const settle = (value) => { pending.delete(entry); resolve(value); };
      entry.cancel = () => { if (!entry.started) settle(cancelValueOf(spec)); else active?.finish(cancelValueOf(spec)); };
      queue = queue.then(() => new Promise((next) => {
        if (destroyed || !pending.has(entry)) { next(); return; }
        entry.started = true;
        show(spec, (value) => { settle(value); next(); });
      }));
    });
  }

  // ---------- Menü ----------
  function closeMenu(value) {
    menuState?.finish(value);
  }

  function menu(spec = {}) {
    if (destroyed) return Promise.resolve(null);
    if (active) return Promise.resolve(null); // ein offenes Blatt ist modal
    closeMenu(null);
    const items = Array.isArray(spec?.items) ? spec.items : [];
    if (!items.some((it) => it && it.type !== 'separator')) return Promise.resolve(null);
    return new Promise((resolve) => {
      const el = doc.createElement('div');
      el.className = 'v4-menu';
      el.dataset.v4Menu = '';
      el.setAttribute('role', 'menu');
      el.tabIndex = -1;
      if (spec.label) el.setAttribute('aria-label', str(spec.label));
      const entries = [];
      items.forEach((it) => {
        if (!it) return;
        if (it.type === 'separator') {
          if (!el.lastElementChild || el.lastElementChild.dataset.v4MenuSeparator !== undefined) return;
          const sep = doc.createElement('div');
          sep.className = 'v4-menu-separator';
          sep.dataset.v4MenuSeparator = '';
          sep.setAttribute('role', 'separator');
          el.append(sep);
          return;
        }
        const b = doc.createElement('button');
        b.type = 'button';
        b.className = `v4-menu-item${it.danger ? ' is-danger' : ''}`;
        b.setAttribute('role', 'menuitem');
        b.tabIndex = -1;
        b.dataset.v4MenuItem = str(it.id);
        if (it.disabled) { b.setAttribute('aria-disabled', 'true'); b.classList.add('is-disabled'); }
        const icon = doc.createElement('span');
        icon.className = 'v4-menu-icon';
        icon.setAttribute('aria-hidden', 'true');
        icon.textContent = str(it.icon);
        const label = doc.createElement('span');
        label.className = 'v4-menu-label';
        label.textContent = str(it.label ?? it.id);
        b.append(icon, label);
        b.addEventListener('click', (ev) => { ev.stopPropagation(); if (!it.disabled) finish(str(it.id)); });
        b.addEventListener('mouseenter', () => { if (!it.disabled) b.focus({ preventScroll: true }); });
        el.append(b);
        entries.push({ b, it });
      });
      if (el.lastElementChild?.dataset.v4MenuSeparator !== undefined) el.lastElementChild.remove();
      const prevFocus = doc.activeElement;
      ensurePositioned();
      host.append(el);
      // Lage: an der Zeigerposition, im Fenster gehalten.
      const at = spec.at && typeof spec.at === 'object' ? spec.at : spec;
      const hr = host.getBoundingClientRect();
      const mw = el.offsetWidth;
      const mh = el.offsetHeight;
      const px = Number.isFinite(Number(at?.x)) && Number(at.x) > 0 ? Number(at.x) - hr.left : 12;
      const py = Number.isFinite(Number(at?.y)) && Number(at.y) > 0 ? Number(at.y) - hr.top : 12;
      el.style.left = `${Math.max(4, Math.min(px, hr.width - mw - 4))}px`;
      el.style.top = `${Math.max(4, Math.min(py, hr.height - mh - 4))}px`;
      const enabled = () => entries.filter((e) => !e.it.disabled).map((e) => e.b);
      const move = (dir) => {
        const list = enabled();
        if (!list.length) return;
        const i = list.indexOf(doc.activeElement);
        const next = dir === 'first' ? list[0] : dir === 'last' ? list[list.length - 1] : list[(i + dir + list.length) % list.length] || list[0];
        next.focus({ preventScroll: true });
      };
      const onKey = (ev) => {
        if (ev.key === 'ArrowDown') { ev.preventDefault(); move(1); }
        else if (ev.key === 'ArrowUp') { ev.preventDefault(); move(-1); }
        else if (ev.key === 'Home') { ev.preventDefault(); move('first'); }
        else if (ev.key === 'End') { ev.preventDefault(); move('last'); }
        else if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); finish(null, true); }
        else if (ev.key === 'Tab') { ev.preventDefault(); finish(null, true); }
        else if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          ev.stopPropagation();
          const hit = entries.find((e) => e.b === doc.activeElement);
          if (hit && !hit.it.disabled) finish(str(hit.it.id));
        }
      };
      const onOutside = (ev) => { if (!el.contains(ev.target)) finish(null); };
      el.addEventListener('keydown', onKey);
      doc.addEventListener('pointerdown', onOutside, true);
      doc.addEventListener('contextmenu', onOutside, true);
      let closed = false;
      function finish(value, restore = false) {
        if (closed) return;
        closed = true;
        if (menuState?.el === el) menuState = null;
        doc.removeEventListener('pointerdown', onOutside, true);
        doc.removeEventListener('contextmenu', onOutside, true);
        el.remove();
        if ((restore || value != null) && prevFocus?.isConnected && host.contains(prevFocus)) {
          try { prevFocus.focus({ preventScroll: true }); } catch { /* egal */ }
        }
        resolve(value);
      }
      menuState = { el, finish };
      move('first');
      if (!enabled().length) el.focus({ preventScroll: true });
    });
  }

  function cancelAll() {
    closeMenu(null);
    for (const entry of [...pending]) entry.cancel();
  }

  return {
    dialog,
    menu,
    cancelAll,
    isOpen: () => !!active,
    get element() { return active?.overlay || null; },
    destroy() {
      if (destroyed) return;
      cancelAll();
      destroyed = true;
    },
  };
}
