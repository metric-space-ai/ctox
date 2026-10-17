// ctox:shared/prompt-template.js – Prompt-Templates aus dem App-Paket laden und streng füllen (Kern §16.30 Nr. 4, §16.34).
//
// Rein: keine App-Bezüge, keine Node- oder DOM-Abhängigkeit. Ein Handler lädt `prompts/<name>.md` aus dem aktiven
// Artefakt der EIGENEN App über `ctx.asset(path) → string` (§16.34: Pfad normalisiert, kein `..`/absolut, ≤ 1 MiB,
// UTF-8). Eine Leser-Funktion `read(path)` ist nur für Tests injizierbar.
//
// Platzhalter: `{{feld}}` bzw. `{{a.b}}` (Punkt = verschachteltes Feld). Streng: ein fehlender Wert (undefined/null)
// ist ein Fehler `invalid` mit allen fehlenden Namen, nie leerer Text. Erlaubt sind string, number, boolean; Objekte
// und Arrays muss der Handler selbst serialisieren. `\{{` steht für ein wörtliches `{{`. Jedes andere `{{`, das kein
// gültiger Platzhalter ist, ist ebenfalls ein Fehler (Tippfehler im Template fallen sofort auf).
//
//   const text = await renderPrompt(ctx, 'issue', { topic: topic.title, sources: JSON.stringify(list) });
//   await ctx.cap.agent.task.enqueue({ title, instruction: text, skill: 'news/news-aggregation', on_complete, on_failed });

const PLACEHOLDER = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\s*\}\}/y;
const NAME = /^[a-z0-9][a-z0-9-]*$/;

function templateError(code, message, detail) {
  const err = new Error(message);
  err.code = code;
  if (detail) err.detail = detail;
  return err;
}

/** Pfad eines Templates: Name `issue` → `prompts/issue.md`; ein Pfad unter `prompts/` bleibt (normalisiert). */
export function promptPath(name) {
  if (typeof name !== 'string' || !name) throw templateError('invalid', 'prompt-template: Name fehlt');
  if (NAME.test(name)) return `prompts/${name}.md`;
  const rel = name.replace(/^\.\//, '');
  const parts = rel.split('/');
  if (rel.startsWith('/') || rel.includes('\\') || parts.some((p) => p === '..' || p === '.' || p === '')
    || parts[0] !== 'prompts' || !rel.endsWith('.md')) {
    throw templateError('invalid', `prompt-template: ungültiger Template-Pfad ${JSON.stringify(name)} (erlaubt: Name oder prompts/<name>.md)`);
  }
  return rel;
}

/** Template-Text laden. `read` nur für Tests; sonst ctx.asset (§16.34). */
export async function loadPromptTemplate(ctx, name, { read } = {}) {
  const file = promptPath(name);
  const reader = read ?? (ctx && typeof ctx.asset === 'function' ? (p) => ctx.asset(p) : null);
  if (!reader) throw templateError('unavailable', 'prompt-template: ctx.asset fehlt (Kern §16.34)');
  const text = await reader(file);
  if (typeof text !== 'string') throw templateError('invalid', `prompt-template: ${file} lieferte keinen Text`);
  return text;
}

function lookup(values, dotted) {
  let cur = values;
  for (const key of dotted.split('.')) {
    if (cur == null || typeof cur !== 'object' || !Object.prototype.hasOwnProperty.call(cur, key)) return undefined;
    cur = cur[key];
  }
  return cur;
}

/** Platzhalternamen eines Templates (in Reihenfolge, ohne Dubletten). Wirft bei kaputten `{{`. */
export function templateFields(template) {
  return [...new Set(parse(template).filter((t) => t.field).map((t) => t.field))];
}

function parse(template) {
  if (typeof template !== 'string') throw templateError('invalid', 'prompt-template: Template ist kein Text');
  const tokens = [];
  let text = '';
  let i = 0;
  while (i < template.length) {
    if (template.startsWith('\\{{', i)) { text += '{{'; i += 3; continue; }
    if (template.startsWith('{{', i)) {
      PLACEHOLDER.lastIndex = i;
      const m = PLACEHOLDER.exec(template);
      if (!m) {
        const line = template.slice(0, i).split('\n').length;
        throw templateError('invalid', `prompt-template: ungültiger Platzhalter in Zeile ${line}: ${JSON.stringify(template.slice(i, i + 24))}`);
      }
      if (text) tokens.push({ text });
      text = '';
      tokens.push({ field: m[1] });
      i = PLACEHOLDER.lastIndex;
      continue;
    }
    text += template[i];
    i += 1;
  }
  if (text) tokens.push({ text });
  return tokens;
}

/** Platzhalter streng füllen. Fehlende Werte → Fehler `invalid` (detail.missing), unzulässige Typen → `invalid`. */
export function fillTemplate(template, values = {}) {
  const tokens = parse(template);
  const missing = [];
  const badType = [];
  let out = '';
  for (const t of tokens) {
    if (!t.field) { out += t.text; continue; }
    const v = lookup(values, t.field);
    if (v === undefined || v === null) { if (!missing.includes(t.field)) missing.push(t.field); continue; }
    if (typeof v === 'string') out += v;
    else if (typeof v === 'number' && Number.isFinite(v)) out += String(v);
    else if (typeof v === 'boolean') out += String(v);
    else if (!badType.includes(t.field)) badType.push(t.field);
  }
  if (missing.length) throw templateError('invalid', `prompt-template: fehlende Platzhalter: ${missing.join(', ')}`, { missing });
  if (badType.length) throw templateError('invalid', `prompt-template: Wert ist kein Text/Zahl/Wahrheitswert: ${badType.join(', ')}`, { bad_type: badType });
  return out;
}

/** Laden + füllen in einem Schritt. */
export async function renderPrompt(ctx, name, values, options = {}) {
  return fillTemplate(await loadPromptTemplate(ctx, name, options), values);
}
