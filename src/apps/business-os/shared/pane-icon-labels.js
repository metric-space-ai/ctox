// Icon + Wort (UI-Regel „Icons tragen ein Wort“, workjet#801): Kopf- und Werkzeugknöpfe
// mit `data-short-de` / `data-short-en` bekommen ein sichtbares Kurzwort neben dem Icon.
// Der ausführliche Text bleibt in aria-label/title.
//
// Wörter nur, wo Platz ist: Je Kopf-/Werkzeugzeile mit Wortknöpfen misst ein Beobachter, ob die Wörter passen. Läuft
// die Zeile über, ragt ein Wort aus seinem Knopf, überdecken sich Knöpfe (auch Titel und andere Bedienelemente), wird ein Titel abgeschnitten oder umgebrochen oder fällt ein
// Suchfeld unter SEARCH_MIN, bekommt die Zeile `data-icons-only` (base.css: nur Icons). Neu entschieden bei jeder Größen- und Textänderung, höchstens einmal je Bild; der
// Beobachter reagiert nicht auf das eigene Attribut (keine Rückkopplung). Der v4-Rahmen prüft zusätzlich die
// reservierten Zonen (v4/frame/pane-heads.js, data-v4-icons-only).
//
// In der Shell v4 ruft der Host (v4/shell/column-resizers.js) das für JEDE App-Wurzel nach mount auf; Apps tragen nur
// data-short-de/-en an ihren Knöpfen. Knöpfe, die die App später rendert, bekommen ihr Wort beim nächsten Durchlauf.
// Ein Beobachter je Wurzel, auch über Modulinstanzen hinweg (ctox:shared/… und der relative Import der Shell sind zwei
// Instanzen): Ablage in einer Registry unter Symbol.for.
//
//   const labels = applyPaneIconLabels(root, lang);   // mehrfach aufrufbar (Sprachwechsel), ein Beobachter je root
//   labels.dispose();                                  // beim Abbau: Beobachter trennen, Markierungen entfernen

export const ICONS_ONLY_ATTR = 'data-icons-only';
const ROWS = '.ctox-pane-header, .ctox-filterbar';
const LABELLED = '.ctox-pane-icon.has-label';
const SEARCH_MIN = 120;
const REGISTRY = Symbol.for('ctox.paneIconLabels.fitters');
const fitters = (globalThis[REGISTRY] ||= new WeakMap());
const langKey = (lang) => (String(lang || '').toLowerCase().startsWith('en') ? 'shortEn' : 'shortDe');
// Englische Kurzwörter zum gemeinsamen Wortschatz (Apps tragen nur data-short-de; data-short-en überstimmt).
// v4-allow: en
const EN = Object.freeze({
  Neu: 'New', Import: 'Import', Export: 'Export', Filter: 'Filter', Zurücksetzen: 'Reset', Bearbeiten: 'Edit', // v4-allow: en
  Löschen: 'Delete', Entfernen: 'Remove', Archivieren: 'Archive', Download: 'Download', Hochladen: 'Upload', // v4-allow: en
  Speichern: 'Save', Kopieren: 'Copy', Ausführen: 'Run', Berechnen: 'Compute', Generieren: 'Generate', // v4-allow: en
  CTOX: 'CTOX', Agent: 'Agent', Heute: 'Today', Abbrechen: 'Cancel', Einstellungen: 'Settings', Senden: 'Send', // v4-allow: en
  Antwort: 'Answer', Research: 'Research', Anonymisieren: 'Anonymize', Erzeugen: 'Generate', Konfigurieren: 'Configure', // v4-allow: en
  Auftrag: 'Task', Prüfen: 'Check', Aktualisieren: 'Refresh', Aufbauen: 'Build', Fortsetzen: 'Resume', // v4-allow: en
  Zeile: 'Row', Spalte: 'Column', ZIP: 'ZIP', GitHub: 'GitHub', Webhook: 'Webhook', Reparsen: 'Reparse', // v4-allow: en
  Datenschutz: 'Privacy', // v4-allow: en
});
const wordFor = (button, key) => (key === 'shortEn'
  ? (button.dataset.shortEn || EN[button.dataset.shortDe] || button.dataset.shortDe)
  : button.dataset.shortDe);

// Zeilenaktionen in Listen bleiben Icons (wie in Mail- und Dateilisten üblich): dort kein Wort.
const LIST_ROW = '.ctox-list-item, li, tr, [role="option"], [role="row"], [role="listitem"], [role="treeitem"]';

function labelButtons(root, key) {
  for (const button of root.querySelectorAll('[data-short-de]')) {
    if (button.closest(LIST_ROW)) {
      button.querySelector(':scope > .ctox-pane-icon-label')?.remove();
      if (button.classList.contains('has-label')) button.classList.remove('has-label');
      continue;
    }
    const word = wordFor(button, key);
    let label = button.querySelector(':scope > .ctox-pane-icon-label');
    if (!label) {
      label = button.ownerDocument.createElement('span');
      label.className = 'ctox-pane-icon-label';
      button.append(label);
    }
    if (label.textContent !== word) label.textContent = word;
    if (!button.classList.contains('has-label')) button.classList.add('has-label');
  }
}

export function applyPaneIconLabels(root, lang = 'de') {
  if (!root?.querySelectorAll) return { dispose() {}, fit() {} };
  let fitter = fitters.get(root);
  if (!fitter) { fitter = createFitter(root); fitters.set(root, fitter); }
  fitter.setLang(lang);
  fitter.fit();
  return fitter;
}

function rowIsTight(row) {
  const r = row.getBoundingClientRect();
  if (!(r.width > 0 && r.height > 0)) return false;
  const cut = (el) => el.scrollWidth > el.clientWidth + 1;
  if (cut(row) || [...row.children].some(cut)) return true;
  const boxes = [];
  for (const b of row.querySelectorAll(LABELLED)) {
    const br = b.getBoundingClientRect();
    if (!(br.width > 0)) continue;
    if (br.right > r.right + 0.5 || br.left < r.left - 0.5) return true;
    // Wort ragt aus dem eigenen Knopf (App-CSS hält die Knopfbreite fest).
    const lr = b.querySelector(':scope > .ctox-pane-icon-label')?.getBoundingClientRect();
    if (lr && lr.width > 0 && (lr.right > br.right + 0.5 || lr.left < br.left - 0.5)) return true;
    boxes.push(br);
  }
  // Wortknöpfe überdecken Titel, Kicker oder andere Bedienelemente der Zeile.
  const others = [...row.querySelectorAll('.ctox-pane-title, .ctox-pane-kicker, button, input, select, a[href]')]
    .filter((el) => !el.matches(LABELLED) && !el.closest(LABELLED)).map((el) => el.getBoundingClientRect()).filter((o) => o.width > 0);
  const overlap = (a, c) => Math.min(a.right, c.right) - Math.max(a.left, c.left) > 0.5 && Math.min(a.bottom, c.bottom) - Math.max(a.top, c.top) > 0.5;
  if (boxes.some((b) => others.some((o) => overlap(b, o)))) return true;
  // Wortknöpfe überdecken sich gegenseitig.
  for (let i = 0; i < boxes.length; i += 1) {
    for (let j = i + 1; j < boxes.length; j += 1) {
      const a = boxes[i]; const c = boxes[j];
      if (Math.min(a.right, c.right) - Math.max(a.left, c.left) > 0.5 && Math.min(a.bottom, c.bottom) - Math.max(a.top, c.top) > 0.5) return true;
    }
  }
  for (const t of row.querySelectorAll('.ctox-pane-title, .ctox-pane-kicker')) {
    if (cut(t)) return true;
    // Der Pane-Kopf ist einzeilig: bricht der Titel wegen der Wörter um, fehlen sie hier.
    const cs = t.ownerDocument.defaultView.getComputedStyle(t);
    const lh = parseFloat(cs.lineHeight) || (parseFloat(cs.fontSize) || 14) * 1.2;
    if (t.getBoundingClientRect().height > lh * 1.5) return true;
  }
  for (const i of row.querySelectorAll('input[type="search"], input[type="text"], input:not([type])')) {
    const w = i.getBoundingClientRect().width;
    if (w > 0 && w < SEARCH_MIN) return true;
  }
  return false;
}

function createFitter(root) {
  const win = root.ownerDocument?.defaultView;
  const raf = win?.requestAnimationFrame?.bind(win) || ((fn) => setTimeout(fn, 16));
  const caf = win?.cancelAnimationFrame?.bind(win) || clearTimeout;
  let frame = 0;
  let disposed = false;
  const observed = new Set();
  const ro = typeof win?.ResizeObserver === 'function' ? new win.ResizeObserver(() => schedule()) : null;
  // Titel und Knöpfe ändern sich ohne Größenänderung der Zeile (Thema gewählt, neu gerenderte Knöpfe): Text- und
  // Kindänderungen unter root lösen ebenfalls einen Durchlauf aus (Wörter setzen, messen). Attribute nicht beobachten
  // (das eigene data-icons-only); das eingefügte Wort selbst löst höchstens einen weiteren, wirkungslosen Durchlauf aus.
  // Tippen in Editoren (contenteditable) betrifft keine Kopfzeile: diese Änderungen lösen keinen Durchlauf aus.
  const inEditor = (node) => (node.nodeType === 1 ? node : node.parentElement)?.closest?.('[contenteditable=""], [contenteditable="true"]');
  const mo = typeof win?.MutationObserver === 'function'
    ? new win.MutationObserver((records) => { if (records.some((r) => !inEditor(r.target))) schedule(); })
    : null;
  let key = 'shortDe';
  ro?.observe(root);
  mo?.observe(root, { childList: true, characterData: true, subtree: true });

  function fit() {
    frame = 0;
    if (disposed) return;
    labelButtons(root, key);
    // Zeile = Kopf-/Werkzeugzeile des Wortknopfs, sonst sein Elternelement (Detail-Werkzeugleisten, Kartenaktionen).
    const rows = [...new Set([...root.querySelectorAll(LABELLED)].map((b) => b.closest(ROWS) || b.parentElement).filter(Boolean))];
    // Beobachtet werden die Zeilen und darin Titel und Wortknöpfe: ein späterer Umbruch (Schrift nachgeladen, Text
    // geändert) ändert deren Größe, nicht die der Zeile.
    const watch = new Set(rows.flatMap((el) => [el, ...el.querySelectorAll(`.ctox-pane-title, .ctox-pane-kicker, ${LABELLED}`)]));
    for (const el of observed) if (!watch.has(el)) { ro?.unobserve(el); observed.delete(el); }
    for (const el of watch) if (!observed.has(el)) { ro?.observe(el); observed.add(el); }
    // Mit Wörtern messen (alle Markierungen entfernen, ein Layout lesen), dann gezielt setzen.
    for (const el of rows) el.removeAttribute(ICONS_ONLY_ATTR);
    const tight = rows.filter(rowIsTight);
    for (const el of tight) el.setAttribute(ICONS_ONLY_ATTR, '');
  }
  function schedule() {
    if (!frame && !disposed) frame = raf(fit);
  }
  return {
    fit,
    setLang(lang) { key = langKey(lang); },
    dispose() {
      disposed = true;
      if (frame) caf(frame);
      frame = 0;
      ro?.disconnect();
      mo?.disconnect();
      for (const el of root.querySelectorAll(`[${ICONS_ONLY_ATTR}]`)) el.removeAttribute(ICONS_ONLY_ATTR);
      observed.clear();
      fitters.delete(root);
    },
  };
}
