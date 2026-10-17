// Shell v4 – Geometrie-Labor: Messung G1–G5 und Layout-Signatur (läuft in der Seite über page.evaluate).
// Ausgelagert aus geometry-v4.mjs, damit geometry-v4.test.mjs die Kriterien an Positiv-/Negativ-Fixtures und an
// Mutanten prüfen kann. Keine Importe: measureInPage wird serialisiert und im Browser ausgeführt.
export function measureInPage({ app, vocab, allow, owner, withG5 }) {
  const rec = window.__geo.frame.shellHandle().win(owner);
  if (!rec) {
    const k = window.__geo.kernel;
    return { G7: [], G1: [`Fenster nach dem Öffnen wieder weg; Shell ${JSON.stringify(window.__geo.frame.describeShell()?.windows)}; kernel.apps ${JSON.stringify(k.apps.get().map((x) => ({ id: x.id, state: x.state, active_ver: x.active_ver, error: x.error })))}; Zone ${JSON.stringify(k.status.get?.() || [])}`.slice(0, 900)], G2: [], G3: [], G4: [], G5: [], notes: [], info: {}, failedToRun: true };
  }
  const win = rec.handle.element;
  const content = win.querySelector('.shell-window-content');
  const root = win.querySelector('.v4-app-root');
  const out = { G1: [], G2: [], G3: [], G4: [], G5: [], G7: [], G8: [], G9: [], notes: [], info: {} };
  if (!root) { out.G1.push('kein .v4-app-root (App nicht montiert)'); return out; }
  const R = root.getBoundingClientRect();
  const W = win.getBoundingClientRect();
  const C = content.getBoundingClientRect();
  const desc = (el) => {
    const cls = typeof el.className === 'string' ? el.className.trim().split(/\s+/).slice(0, 2).join('.') : '';
    const da = [...el.attributes].find((x) => x.name.startsWith('data-') && x.name !== 'data-app');
    return `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ''}${cls ? `.${cls}` : ''}${!el.id && !cls && da ? `[${da.name}]` : ''}`;
  };
  const visible = (el) => {
    if (!el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
    const r = el.getBoundingClientRect();
    return r.width * r.height > 4;
  };
  const all = [...root.querySelectorAll('*')];
  const vis = all.filter(visible);
  out.info.elements = all.length;
  out.info.visibleElements = vis.length;
  out.info.root = { w: Math.round(R.width), h: Math.round(R.height) };
  out.info.window = { w: Math.round(W.width), h: Math.round(W.height), mode: win.classList.contains('is-mobile-sheet') ? 'mobile-sheet' : (win.dataset.windowState || win.dataset.state || '') };
  out.info.textChars = (root.innerText || '').trim().length;
  out.info.scrollTop = Math.round(root.scrollTop);

  // ---- G1 ----
  if (root.scrollWidth > root.clientWidth + 1) out.G1.push(`root scrollt horizontal (${root.scrollWidth} > ${root.clientWidth})`);
  const clipOf = new Map();
  const clipRect = (el) => {
    if (el === root || !el || !root.contains(el)) return { l: -1e9, r: 1e9, t: -1e9, b: 1e9 };
    if (clipOf.has(el)) return clipOf.get(el);
    const parent = clipRect(el.parentElement);
    const cs = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    const c = { ...parent };
    if (cs.overflowX !== 'visible') { c.l = Math.max(c.l, r.left); c.r = Math.min(c.r, r.right); }
    if (cs.overflowY !== 'visible') { c.t = Math.max(c.t, r.top); c.b = Math.min(c.b, r.bottom); }
    clipOf.set(el, c);
    return c;
  };
  // Seitlicher Überstand IN einem Container mit [data-scroll-x] (unterhalb von root) ist erlaubt: Datentabellen dürfen
  // in ihrem eigenen Container seitlich scrollen (Orchestrator-Entscheidung GEO2). Das Attribut an root selbst oder an
  // einem Vorfahren von root (Fenster) zählt nicht: die App-Fläche bleibt streng. Ragt der Container selbst aus root,
  // bleibt das ein G1-Fehler (Prüfung „ragen aus root“ unten ist unverändert).
  const inScrollX = (el) => { const c = el.closest('[data-scroll-x]'); return !!c && c !== root && root.contains(c); };
  const outside = [];
  const hscroll = [];
  const hscrollAllowed = [];
  for (const el of vis) {
    const r = el.getBoundingClientRect();
    const c = clipRect(el.parentElement);
    const l = Math.max(r.left, c.l), rr = Math.min(r.right, c.r), t = Math.max(r.top, c.t), b = Math.min(r.bottom, c.b);
    if (rr - l < 1 || b - t < 1) continue;
    const cs = getComputedStyle(el);
    if (cs.position === 'fixed' || cs.position === 'sticky') { /* bleibt im contain-Rahmen, unten mitgeprüft */ }
    const dx = Math.max(R.left - l, rr - R.right);
    const dy = (R.top - root.scrollTop) - t; // oberhalb des Inhaltsanfangs (root scrollt vertikal)
    if (dx > 1.5 || dy > 1.5) outside.push(`${desc(el)} ${dx > 1.5 ? `${Math.round(dx)}px seitlich` : `${Math.round(dy)}px oberhalb`}`);
    if ((cs.overflowX === 'auto' || cs.overflowX === 'scroll') && el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0) {
      (inScrollX(el) ? hscrollAllowed : hscroll).push(`${desc(el)} ${el.scrollWidth}>${el.clientWidth}`);
    }
  }
  if (outside.length) out.G1.push(`${outside.length} Elemente ragen aus root: ${[...new Set(outside)].slice(0, 6).join('; ')}`);
  if (hscroll.length) out.G1.push(`${hscroll.length} innere Container scrollen horizontal: ${hscroll.slice(0, 4).join('; ')}`);
  if (hscrollAllowed.length) out.notes.push(`G1 erlaubt: ${hscrollAllowed.length} Container mit [data-scroll-x] scrollen seitlich: ${hscrollAllowed.slice(0, 4).join('; ')}`);
  // Die Chatleiste der Shell (CHAT4, v4/shell/chat: div.v4f-chat) hängt der Rahmen bewusst an document.body, sobald ctox
  // aktiv ist – Shell, nicht App. Jedes andere neue body-Kind bleibt ein G1-Befund.
  const bodyNew = [...document.body.children].filter((e) => !window.__geo.bodyBefore.includes(e) && e.tagName !== 'SCRIPT' && e.tagName !== 'LINK' && e.tagName !== 'STYLE' && !e.classList.contains('v4f-chat'));
  if (bodyNew.length) out.G1.push(`${bodyNew.length} Elemente an document.body: ${bodyNew.slice(0, 3).map(desc).join(', ')}`);
  // Punktabtastung: App-Pixel über Fensterkopf, Zone oder Taskleiste?
  const zoneSlot = win.querySelector('[data-v4-zone-slot]');
  const areas = [['Fensterkopf', { left: W.left, right: W.right, top: W.top, bottom: C.top }]];
  if (zoneSlot) { const z = zoneSlot.getBoundingClientRect(); if (z.height > 0) areas.push(['Zone', z]); }
  const bar = document.querySelector('.v4f-bar, [data-v4-taskbar]');
  if (bar) areas.push(['Taskleiste', bar.getBoundingClientRect()]);
  for (const [name, a] of areas) {
    let hits = 0;
    for (let i = 0; i < 9; i++) for (let j = 0; j < 3; j++) {
      const x = a.left + ((a.right - a.left) * (i + 0.5)) / 9;
      const y = a.top + ((a.bottom - a.top) * (j + 0.5)) / 3;
      if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) continue;
      const top = document.elementFromPoint(x, y);
      if (top && root.contains(top)) hits++;
    }
    if (hits) out.G1.push(`App überdeckt ${name} (${hits} Abtastpunkte)`);
  }

  // ---- G2 ----
  const titles = window.__geo.titles.slice();
  const appTitles = titles.slice(1);
  const finalTitle = (win.querySelector('.shell-window-title-text')?.textContent || '').trim();
  out.info.title = finalTitle;
  out.info.titleCalls = appTitles;
  if (!appTitles.length) out.G2.push('App ruft api.shell.setTitle nicht auf');
  if (!finalTitle) out.G2.push('Titel leer');
  else {
    if (finalTitle === app) out.G2.push(`Titel ist die App-ID „${finalTitle}“`);
    if (/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i.test(finalTitle)) out.G2.push(`Titel enthält UUID „${finalTitle}“`);
    if (/\b[a-z][a-z-]{1,14}-\d{3}\b/.test(finalTitle)) out.G2.push(`Titel enthält Datensatz-ID „${finalTitle}“`);
  }

  // ---- G3 ----
  const res = vocab.map((v) => { try { return [v.id, new RegExp(v.pattern, v.flags || '')]; } catch { return null; } }).filter(Boolean);
  // Ausnahme nur, wenn der sichtbare Text die gekennzeichnete Beschriftung vollständig enthält.
  const allowed = (el, text) => el.closest('[data-v4-allow],[data-v4-allow-label],[data-v4-allow-state]') || allow.some((a) => text.includes(a));
  const hitsG3 = [];
  const tw = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const texts = [];
  for (let n = tw.nextNode(); n; n = tw.nextNode()) {
    const t = n.nodeValue.replace(/\s+/g, ' ').trim();
    if (!t || !n.parentElement || !visible(n.parentElement)) continue;
    // Text im Zusammenhang des Elements („<b>1</b> wartet“ → „1 wartet“), bei langen Blöcken nur der Knoten.
    const whole = (n.parentElement.innerText || '').replace(/\s+/g, ' ').trim();
    texts.push([n.parentElement, whole.length <= 160 ? whole : t]);
  }
  for (const el of vis) {
    if (el.placeholder) texts.push([el, el.placeholder]);
  }
  for (const [el, t] of texts) {
    for (const [id, re] of res) {
      if (re.test(t) && !allowed(el, t)) { hitsG3.push(`„${t.slice(0, 70)}“ [${id}] in ${desc(el)}`); break; }
    }
  }
  if (hitsG3.length) out.G3.push(`${hitsG3.length} Zustandstexte: ${[...new Set(hitsG3)].slice(0, 6).join('; ')}`);
  const zone = win.querySelector('.v4-status-zone');
  if (zone && !zone.hidden && zone.getBoundingClientRect().height > 0) {
    out.G3.push(`Zustandszone sichtbar: „${(zone.innerText || '').trim().slice(0, 120)}“`);
  }
  out.info.zoneEntries = window.__geo.kernel.status.forApp ? window.__geo.kernel.status.forApp(app).get().map((e) => `${e.source}:${e.code}`) : [];

  // ---- G4 ----
  const cut = [];
  for (const el of vis) {
    const cs = getComputedStyle(el);
    const ell = cs.textOverflow === 'ellipsis' && cs.overflowX !== 'visible' && el.scrollWidth > el.clientWidth + 1;
    const clamp = cs.webkitLineClamp && cs.webkitLineClamp !== 'none' && el.scrollHeight > el.clientHeight + 1;
    if (!(ell || clamp) || !(el.textContent || '').trim()) continue;
    if (el.closest('[data-v4-allow-ellipsis]')) continue;
    let inList = !!el.closest('li,tr,[role="row"],[role="listitem"],[role="option"],[role="treeitem"]');
    for (let a = el, k = 0; !inList && a && a !== root && k < 5; a = a.parentElement, k++) {
      const c1 = typeof a.className === 'string' ? a.className.trim().split(/\s+/)[0] : '';
      if (c1 && a.parentElement && [...a.parentElement.children].filter((s) => s.classList?.contains(c1)).length >= 3) inList = true;
    }
    cut.push(`${inList ? 'Liste' : 'Text'} ${desc(el)} „${(el.textContent || '').trim().slice(0, 40)}“`);
  }
  if (cut.length) out.G4.push(`${cut.length} abgeschnittene Texte (${cut.filter((c) => c.startsWith('Liste')).length} in Listen): ${cut.slice(0, 5).join('; ')}`);
  out.info.ellipsisCut = cut.length;
  // G4 Icon + Wort (workjet#801): sichtbare Kurzwörter stehen ganz im eigenen Knopf und überdecken sich nicht.
  const words = [...root.querySelectorAll('.ctox-pane-icon-label')]
    .map((l) => ({ l, r: l.getBoundingClientRect(), b: l.parentElement.getBoundingClientRect() }))
    .filter((x) => x.r.width > 0 && x.r.height > 0 && getComputedStyle(x.l).visibility !== 'hidden');
  const wordHits = [];
  for (const x of words) {
    if (x.r.left < x.b.left - 0.5 || x.r.right > x.b.right + 0.5) wordHits.push(`Wort „${x.l.textContent.trim()}“ ragt aus seinem Knopf`);
  }
  for (let i = 0; i < words.length; i += 1) {
    for (let j = i + 1; j < words.length; j += 1) {
      const a = words[i].r; const c = words[j].r;
      if (Math.min(a.right, c.right) - Math.max(a.left, c.left) > 0.5 && Math.min(a.bottom, c.bottom) - Math.max(a.top, c.top) > 0.5) {
        wordHits.push(`Wörter „${words[i].l.textContent.trim()}“ und „${words[j].l.textContent.trim()}“ überdecken sich`);
      }
    }
  }
  for (const x of words) {
    const row = x.l.parentElement.closest('.ctox-pane-header, .ctox-filterbar') || x.l.parentElement.parentElement;
    for (const t of row?.querySelectorAll('.ctox-pane-title, .ctox-pane-kicker') || []) {
      const tr = t.getBoundingClientRect();
      if (tr.width > 0 && Math.min(x.r.right, tr.right) - Math.max(x.r.left, tr.left) > 0.5 && Math.min(x.r.bottom, tr.bottom) - Math.max(x.r.top, tr.top) > 0.5) {
        wordHits.push(`Wort „${x.l.textContent.trim()}“ überdeckt Titel „${t.textContent.trim().slice(0, 30)}“`);
      }
    }
  }
  if (wordHits.length) out.G4.push(`${wordHits.length} Wortknopf-Fehler: ${wordHits.slice(0, 4).join('; ')}`);

  // ---- G7: Geschwister-Pane-Köpfe eines Fensters gleich hoch (±1 px) ----
  // Geschwister = sichtbare .ctox-pane-header im App-Inhalt mit gleicher Oberkante (±2 px), also Köpfe
  // nebeneinanderstehender Spalten. Lange Titel dürfen einen Kopf wachsen lassen, dann wachsen alle Geschwister mit.
  const headBoxes = [...root.querySelectorAll('.ctox-pane-header')].filter(visible).map((el) => ({ el, r: el.getBoundingClientRect() }));
  const headRows = [];
  for (const b of headBoxes.sort((a, c) => a.r.top - c.r.top)) {
    const g = headRows.find((x) => Math.abs(x.top - b.r.top) <= 2);
    if (g) g.items.push(b); else headRows.push({ top: b.r.top, items: [b] });
  }
  for (const g of headRows) {
    if (g.items.length < 2) continue;
    const hs = g.items.map((x) => x.r.height);
    if (Math.max(...hs) - Math.min(...hs) > 1) out.G7.push(`Pane-Köpfe einer Reihe ungleich hoch: ${g.items.map((x) => `${desc(x.el.parentElement || x.el)}>${desc(x.el)} ${Math.round(x.r.height)}px`).join(', ')}`);
  }
  out.info.paneHeadRows = headRows.filter((g) => g.items.length > 1).map((g) => g.items.map((x) => Math.round(x.r.height * 10) / 10));

  // ---- G8: reservierte Zonen (FR6a, Owner 15.10.) ----
  // Unter dem schwebenden Icon-Header (oben links) und dem schwebenden Knopfcluster (oben rechts) liegt kein
  // interaktives Element der App (button, a, input, select, textarea, [role=button], [tabindex]). Maße aus den
  // Variablen des Rahmens am Inhaltsbereich: --shell-icon-inset × --shell-icon-zone-height (links),
  // --shell-controls-inset × --shell-controls-zone-height (rechts). Ohne diese Variablen (kein v4-Rahmen): nicht gemessen.
  out.G8 = [];
  {
    const cs = getComputedStyle(content);
    const num = (n) => parseFloat(cs.getPropertyValue(n));
    const iw = num('--shell-icon-inset'); const ih = num('--shell-icon-zone-height');
    const cw = num('--shell-controls-inset'); const ch = num('--shell-controls-zone-height');
    if (iw > 0 && ih > 0 && cw > 0 && ch > 0) {
      const zones = [['Icon-Header', C.left, C.top, C.left + iw, C.top + ih], ['Knopfcluster', C.right - cw, C.top, C.right, C.top + ch]];
      const hits = [];
      for (const el of root.querySelectorAll('button, a[href], input, select, textarea, [role="button"], [tabindex]:not([tabindex="-1"])')) {
        if (!visible(el)) continue;
        const r = el.getBoundingClientRect();
        for (const [name, x0, y0, x1, y1] of zones) {
          if (r.left < x1 - 0.5 && r.right > x0 + 0.5 && r.top < y1 - 0.5 && r.bottom > y0 + 0.5) hits.push(`${desc(el)} unter ${name}`);
        }
      }
      if (hits.length) out.G8.push(`${hits.length} Bedienelemente in reservierten Zonen: ${hits.slice(0, 5).join('; ')}`);
      out.info.reservedZones = { icon: [iw, ih], controls: [cw, ch] };
    } else out.notes.push('G8 nicht gemessen: keine Zonen-Variablen am Inhaltsbereich');
  }
  // ---- G9 (Teil Geometrie-Labor): Kantenpalette berechnet, nie leer ----
  // Hier: Quelle 'icon' oder 'generated'. Die Pixelmessung (ΔE deckendes Logopixel ↔ Kante) hängt geometry-v4.mjs
  // nach dem Screenshot an (g9.mjs).
  out.G9 = [];
  if (win.dataset.shellContract === 'v2' && content.style !== undefined && getComputedStyle(content).getPropertyValue('--shell-icon-inset')) {
    const src = win.dataset.shellV2PaletteSource || '';
    if (src !== 'icon' && src !== 'generated') out.G9.push(`Kantenpalette nicht aus dem Icon berechnet (shellV2PaletteSource „${src}“)`);
  }

  // ---- Layout-Signatur (G6) ----
  let columns = 0;
  for (const el of [root, ...vis]) {
    const kids = [...el.children].filter((k) => visible(k)).map((k) => k.getBoundingClientRect()).filter((r) => r.width >= 100 && r.height >= 100);
    if (kids.length < 2) continue;
    // Kinder, die sich vertikal überlappen und nebeneinander stehen.
    kids.sort((a, b) => a.left - b.left);
    let n = 1;
    for (let i = 1; i < kids.length; i++) if (kids[i].left >= kids[i - 1].right - 2 && kids[i].top < kids[i - 1].bottom && kids[i].bottom > kids[i - 1].top) n++;
    columns = Math.max(columns, n);
  }
  const keys = new Set();
  for (const el of vis) {
    if (typeof el.className !== 'string' || !el.className.trim()) continue;
    keys.add(`${el.tagName}.${el.className.trim().split(/\s+/)[0]}`);
  }
  out.signature = { columns, keys: [...keys] };

  // ---- Token-Abdeckung (Info): var(--x) ohne Rückfall, die im Rahmen nicht definiert sind ----
  // Shell-eigene Blätter (Chatleiste CHAT4 unter /v4/shell/, Rahmen unter /v4/frame/) kommen erst mit der App-Aktivierung
  // dazu, sind aber kein App-CSS: G5 schaltete sonst die Chat-Stile ab und meldete die Chatleiste als „App-Leck“.
  // Dazu die zwei Stilblöcke, die die V2-Chatleiste selbst einhängt (shared/business-chat.js, Crew-Figuren).
  const SHELL_STYLE_IDS = new Set(['ctox-business-chat-style', 'ctox-crew-creature-css']);
  const shellOwned = (sheet) => (sheet.href ? /^\/v4\/(shell|frame)\//.test(new URL(sheet.href, location.href).pathname) : SHELL_STYLE_IDS.has(sheet.ownerNode?.id));
  const appSheets = [...document.styleSheets].filter((s) => !window.__geo.sheetsBefore.has(s.ownerNode) && !(s.ownerNode?.dataset && 'v4FrameCss' in s.ownerNode.dataset) && !shellOwned(s));
  out.info.appSheets = appSheets.map((s) => (s.href || '(inline)').replace(location.origin, ''));
  const rulesOf = (list, acc = []) => { for (const r of list) { if (r.selectorText !== undefined) acc.push(r); if (r.cssRules) rulesOf(r.cssRules, acc); } return acc; };
  const rules = [];
  for (const s of appSheets) { try { rulesOf(s.cssRules, rules); } catch { /* fremde Herkunft */ } }
  const undef = new Map();
  const rcs = getComputedStyle(root);
  for (const r of rules) {
    for (const m of r.cssText.matchAll(/var\(\s*(--[\w-]+)\s*\)/g)) {
      if (!rcs.getPropertyValue(m[1]).trim()) undef.set(m[1], (undef.get(m[1]) || 0) + 1);
    }
  }
  out.info.undefinedTokens = [...undef.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([k, n]) => `${k}×${n}`);
  out.info.undefinedTokenCount = undef.size;

  // ---- G5 ----
  if (withG5) {
    const outsideEls = [...document.querySelectorAll('*')].filter((e) => !root.contains(e) && e !== root);
    const strictOutside = outsideEls.filter((e) => !e.closest(`[data-app="${app}"]`));
    const leaks = new Map();
    for (const r of rules) {
      let sel = r.selectorText;
      if (!sel || sel.includes('::')) sel = sel?.replace(/::?[\w-]+(\([^)]*\))?$/g, '');
      if (!sel) continue;
      for (const part of sel.split(',')) {
        const s = part.trim();
        if (!s) continue;
        let hit = null;
        try { hit = outsideEls.find((e) => e.matches(s)); } catch { continue; }
        if (hit) leaks.set(s, `${desc(hit)}${strictOutside.includes(hit) ? ' (außerhalb jedes [data-app])' : ' (Fenster-Chrome)'}`);
      }
    }
    const PROPS = ['color', 'background-color', 'font-size', 'font-family', 'font-weight', 'line-height', 'display', 'position', 'visibility', 'opacity', 'padding-top', 'padding-left', 'margin-top', 'margin-left', 'border-top-width', 'border-top-color', 'width', 'height', 'box-shadow', 'text-transform', 'letter-spacing'];
    const snap = () => outsideEls.map((e) => { const cs = getComputedStyle(e); return PROPS.map((p) => cs.getPropertyValue(p)).join('|'); });
    const withCss = snap();
    for (const s of appSheets) s.disabled = true;
    const without = snap();
    for (const s of appSheets) s.disabled = false;
    const changed = [];
    outsideEls.forEach((e, i) => {
      if (withCss[i] === without[i]) return;
      const a = withCss[i].split('|'), b = without[i].split('|');
      const props = PROPS.filter((_, k) => a[k] !== b[k]);
      changed.push(`${desc(e)}: ${props.slice(0, 3).join(',')}`);
    });
    if (changed.length) out.G5.push(`${changed.length} Elemente außerhalb root ändern ihren Stil durch App-CSS: ${changed.slice(0, 5).join('; ')}`);
    if (leaks.size) out.notes.push(`G5-Beleg: ${leaks.size} App-Selektoren treffen Elemente außerhalb root: ${[...leaks].slice(0, 4).map(([s, h]) => `${s} → ${h}`).join('; ')}`);
  }
  return out;
}

export function sameLayout(a, b) {
  if (!a || !b) return { same: false, why: 'Signatur fehlt' };
  const A = new Set(a.keys), B = new Set(b.keys);
  const inter = [...A].filter((k) => B.has(k)).length;
  const uni = new Set([...A, ...B]).size || 1;
  const j = inter / uni;
  const onlyA = [...A].filter((k) => !B.has(k)), onlyB = [...B].filter((k) => !A.has(k));
  const same = a.columns === b.columns && j >= 0.9;
  return { same, j: Math.round(j * 100) / 100, why: same ? '' : `Spalten ${a.columns} (Viewport) vs ${b.columns} (Fenster in 1280); Bausteine gleich ${Math.round(j * 100)} %${onlyA.length ? `; nur Viewport: ${onlyA.slice(0, 3).join(',')}` : ''}${onlyB.length ? `; nur Fenster: ${onlyB.slice(0, 3).join(',')}` : ''}` };
}
