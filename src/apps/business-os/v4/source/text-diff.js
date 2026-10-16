// Shell v4 – Zeilen-Diff und Null-Kontext-Patch für die Code-Ansicht (Strang AGENT, Bedienkonzept §3).
//
//   lineDiff(a, b) → [{ op: ' ' | '-' | '+', text, a?: Zeilennr. alt, b?: Zeilennr. neu }]
//   renderDiff(host, a, b) → Element .v4-src-diff (nur lesend; zeigt Änderungen gegen die Basis)
//   parsePatch(text) → [{ path, before: 'a'|null, after: 'b'|null, hunks: [{ start, removed[], added[] }] }]
//   applyPatch(files, text) → { path: neuerText | null } (null = Datei gelöscht); wirft bei abweichendem Stand
//
// Format des Kern-Patches (§16.44, draft_turn.rs unified_patch): je Datei EIN Hunk ohne Kontext
// (`git apply --unidiff-zero`), „--- /dev/null“ = neue Datei, „+++ /dev/null“ = gelöschte Datei.

const lines = (t) => (t === null || t === undefined || t === '' ? [] : String(t).replace(/\n$/, '').split('\n'));

export function lineDiff(a, b) {
  const A = lines(a);
  const B = lines(b);
  let pre = 0;
  while (pre < A.length && pre < B.length && A[pre] === B[pre]) pre++;
  let suf = 0;
  while (suf < A.length - pre && suf < B.length - pre && A[A.length - 1 - suf] === B[B.length - 1 - suf]) suf++;
  const a2 = A.slice(pre, A.length - suf);
  const b2 = B.slice(pre, B.length - suf);
  const out = [];
  for (let i = 0; i < pre; i++) out.push({ op: ' ', text: A[i], a: i + 1, b: i + 1 });
  if (a2.length * b2.length > 4e6) {
    // Sehr große Änderung: als Block ersetzen statt LCS.
    a2.forEach((t, i) => out.push({ op: '-', text: t, a: pre + i + 1 }));
    b2.forEach((t, i) => out.push({ op: '+', text: t, b: pre + i + 1 }));
  } else {
    const n = a2.length;
    const m = b2.length;
    const L = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
    for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i][j] = a2[i] === b2[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
    let i = 0;
    let j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && a2[i] === b2[j]) { out.push({ op: ' ', text: a2[i], a: pre + i + 1, b: pre + j + 1 }); i++; j++; }
      // Bei Gleichstand zuerst die entfernte Zeile (wie git diff: „-“ vor „+“).
      else if (i < n && (j >= m || L[i + 1][j] >= L[i][j + 1])) { out.push({ op: '-', text: a2[i], a: pre + i + 1 }); i++; }
      else { out.push({ op: '+', text: b2[j], b: pre + j + 1 }); j++; }
    }
  }
  for (let k = 0; k < suf; k++) out.push({ op: ' ', text: A[A.length - suf + k], a: A.length - suf + k + 1, b: B.length - suf + k + 1 });
  return out;
}

/** Nur lesende Diff-Fläche: unveränderte Abschnitte > 6 Zeilen klappen auf 3 Kontextzeilen zusammen. */
export function renderDiff(host, a, b, { context = 3 } = {}) {
  const rows = lineDiff(a, b);
  const el = document.createElement('div');
  el.className = 'v4-src-diff';
  el.dataset.v4TextDiff = '';
  el.setAttribute('role', 'region');
  el.setAttribute('aria-label', 'Änderungen gegen die Basis');
  const keep = rows.map((r, i) => r.op !== ' ' || rows.slice(Math.max(0, i - context), i + context + 1).some((x) => x.op !== ' '));
  let added = 0;
  let removed = 0;
  let skipped = 0;
  const flushSkip = () => {
    if (!skipped) return;
    const s = document.createElement('div');
    s.className = 'v4-src-diff__skip';
    s.textContent = `⋯ ${skipped} unveränderte Zeilen`;
    el.append(s);
    skipped = 0;
  };
  rows.forEach((r, i) => {
    if (r.op === '+') added++;
    if (r.op === '-') removed++;
    if (!keep[i]) { skipped++; return; }
    flushSkip();
    const line = document.createElement('div');
    line.className = `v4-src-diff__line${r.op === '+' ? ' is-add' : r.op === '-' ? ' is-del' : ''}`;
    line.dataset.op = r.op;
    const na = document.createElement('span');
    na.className = 'v4-src-diff__no';
    na.textContent = r.a ?? '';
    const nb = document.createElement('span');
    nb.className = 'v4-src-diff__no';
    nb.textContent = r.b ?? '';
    const tx = document.createElement('span');
    tx.className = 'v4-src-diff__text';
    tx.textContent = `${r.op === ' ' ? ' ' : r.op} ${r.text}`;
    line.append(na, nb, tx);
    el.append(line);
  });
  flushSkip();
  el.dataset.added = String(added);
  el.dataset.removed = String(removed);
  if (!added && !removed) {
    const e = document.createElement('div');
    e.className = 'v4-src-diff__skip';
    e.textContent = 'Keine Änderungen gegen die Basis.';
    el.append(e);
  }
  host.append(el);
  return el;
}

export function parsePatch(text) {
  const out = [];
  let cur = null;
  let hunk = null;
  for (const raw of String(text ?? '').split('\n')) {
    if (raw.startsWith('--- ') && (!hunk || hunk.done())) {
      const p = raw.slice(4).trim();
      cur = { path: null, before: p === '/dev/null' ? null : p.replace(/^a\//, ''), after: null, hunks: [] };
      out.push(cur);
      hunk = null;
      continue;
    }
    if (raw.startsWith('+++ ') && cur && !cur.hunks.length && cur.path === null) {
      const p = raw.slice(4).trim();
      cur.after = p === '/dev/null' ? null : p.replace(/^b\//, '');
      cur.path = cur.after ?? cur.before;
      continue;
    }
    const m = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(raw);
    if (m && cur) {
      const oldStart = Number(m[1]);
      const oldLen = m[2] === undefined ? 1 : Number(m[2]);
      const newLen = m[4] === undefined ? 1 : Number(m[4]);
      // Länge 0: die Zahl nennt die Zeile VOR der Einfügestelle (start = Anzahl vorangehender Zeilen).
      const h = { start: oldLen === 0 ? oldStart : oldStart - 1, oldLen, newLen, old: [], new: [], context: 0 };
      h.done = () => h.old.length >= h.oldLen && h.new.length >= h.newLen;
      hunk = h;
      cur.hunks.push(h);
      continue;
    }
    if (!hunk || hunk.done()) continue;
    // Kontextzeile (Leerzeichen; eine ganz leere Zeile gilt als leere Kontextzeile), entfernt, hinzugefügt.
    if (raw.startsWith(' ') || raw === '') { hunk.old.push(raw.slice(1)); hunk.new.push(raw.slice(1)); hunk.context++; }
    else if (raw.startsWith('-')) hunk.old.push(raw.slice(1));
    else if (raw.startsWith('+')) hunk.new.push(raw.slice(1));
  }
  return out.filter((f) => f.path);
}

export class PatchConflict extends Error {
  constructor(message, detail = {}) { super(message); this.name = 'PatchConflict'; this.code = 'conflict'; Object.assign(this, detail); }
}

/** files: { pfad: aktueller Text | null }. Wendet den Patch auf den AKTUELLEN Entwurf an. Jeder Hunk muss mit seinen
 *  Kontext- und entfernten Zeilen exakt passen: an der erwarteten Stelle (plus Verschiebung durch vorige Hunks) oder an
 *  genau EINER anderen Stelle (z. B. eigene Zeilen davor eingefügt). Sonst PatchConflict – nie stumm daneben. */
export function applyPatch(files, text) {
  const result = {};
  for (const f of parsePatch(text)) {
    const curText = files[f.path];
    if (f.after === null) {
      if (curText === null || curText === undefined) continue; // schon gelöscht
      result[f.path] = null;
      continue;
    }
    if (f.before !== null && (curText === null || curText === undefined)) throw new PatchConflict(`${f.path} fehlt im Entwurf`, { path: f.path });
    let L = lines(f.before === null ? (curText ?? '') : curText);
    let shift = 0;
    for (const h of f.hunks) {
      const matches = (pos) => pos >= 0 && pos + h.old.length <= L.length && h.old.every((r, k) => L[pos + k] === r);
      let at = h.start + shift;
      if (!matches(at)) {
        // Ohne Kontext und ohne entfernte Zeilen (reine Einfügung, alter Kern) gibt es nichts zu prüfen: nur an der Stelle.
        if (!h.old.length) at = Math.min(at, L.length);
        else {
          const hits = [];
          for (let q = 0; q + h.old.length <= L.length; q++) if (matches(q)) hits.push(q);
          if (hits.length !== 1) {
            throw new PatchConflict(`${f.path}: ${hits.length ? 'Kontext mehrdeutig' : 'Kontext passt nicht mehr'} (Zeile ${h.start + 1})`, { path: f.path, line: h.start + 1 });
          }
          at = hits[0];
        }
      }
      L = [...L.slice(0, at), ...h.new, ...L.slice(at + h.old.length)];
      shift = at - h.start + (h.new.length - h.old.length);
    }
    result[f.path] = L.length ? `${L.join('\n')}\n` : '';
  }
  return result;
}
