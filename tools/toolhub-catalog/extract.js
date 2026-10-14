// Mechanical Activepieces piece extractor (MIT source) -> CTOX toolhub catalog JSON.
const ts = require('typescript'), fs = require('fs'), path = require('path');
const ROOT = process.argv[2] || '/workspace/activepieces/packages/pieces/community';
const OUT = process.argv[3] || 'catalog.json';
function walk(d, acc = []) { for (const f of fs.readdirSync(d, { withFileTypes: true })) {
  if (f.name === 'node_modules' || f.name === 'i18n' || f.name === 'test') continue;
  const p = path.join(d, f.name); if (f.isDirectory()) walk(p, acc); else if (/\.ts$/.test(f.name) && !/\.spec\.ts$/.test(f.name)) acc.push(p); } return acc; }
function lit(n, consts) { // static evaluation of literals
  if (!n) return undefined;
  if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) return n.text;
  if (ts.isNumericLiteral(n)) return Number(n.text);
  if (n.kind === ts.SyntaxKind.TrueKeyword) return true; if (n.kind === ts.SyntaxKind.FalseKeyword) return false;
  if (ts.isTemplateExpression(n)) return n.head.text + n.templateSpans.map(s => '${…}' + s.literal.text).join('');
  if (ts.isArrayLiteralExpression(n)) return n.elements.map(e => lit(e, consts));
  if (ts.isIdentifier(n) && consts && consts[n.text] !== undefined) return consts[n.text];
  if (ts.isAsExpression(n) || ts.isParenthesizedExpression(n)) return lit(n.expression, consts);
  if (ts.isBinaryExpression(n) && n.operatorToken.kind === ts.SyntaxKind.PlusToken) { const a = lit(n.left, consts), b = lit(n.right, consts); if (typeof a === 'string' && typeof b === 'string') return a + b; }
  return undefined;
}
function objProps(o) { const m = {}; if (!o || !ts.isObjectLiteralExpression(o)) return m;
  for (const p of o.properties) { if ((ts.isPropertyAssignment(p) || ts.isShorthandPropertyAssignment(p) || ts.isMethodDeclaration(p)) && p.name) { const k = p.name.text ?? p.name.getText(); m[k] = ts.isPropertyAssignment(p) ? p.initializer : (ts.isShorthandPropertyAssignment(p) ? p.name : p); } } return m; }
function callName(c) { return c.expression.getText(); }
const TYPEMAP = { ShortText: ['string'], LongText: ['string'], Number: ['number'], Checkbox: ['boolean'], DateTime: ['string', 'date-time'], Json: ['object'], Object: ['object'], Array: ['array'], File: ['string', 'file'], StaticDropdown: ['string'], StaticMultiSelectDropdown: ['array'], Dropdown: ['string'], MultiSelectDropdown: ['array'], DynamicProperties: ['object'], MarkDown: [null], Custom: ['object'], Color: ['string'] };
function propSchema(call, consts) { // Property.X({...})
  const kind = call.expression.name ? call.expression.name.text : '?'; const a = objProps(call.arguments[0]);
  const [type, format] = TYPEMAP[kind] || ['string']; if (type === null) return null;
  const s = { type, 'x-ap-kind': kind }; if (format) s.format = format;
  const t = lit(a.displayName, consts), d = lit(a.description, consts); if (t) s.title = t; if (d) s.description = d;
  if (a.defaultValue) { const v = lit(a.defaultValue, consts); if (v !== undefined) s.default = v; }
  if (a.options && /^Static/.test(kind)) { const op = objProps(a.options); if (op.options && ts.isArrayLiteralExpression(op.options)) { const vals = op.options.elements.map(e => lit(objProps(e).value, consts)).filter(v => v !== undefined); if (vals.length) (type === 'array' ? (s.items = { enum: vals }) : (s.enum = vals)); } }
  if (/Dropdown|DynamicProperties/.test(kind) && !/^Static/.test(kind)) s['x-dynamic'] = true;
  return { schema: s, required: lit(a.required, consts) === true };
}
function schemaFromProps(o, consts, propVars) { const props = {}, required = []; if (!o || !ts.isObjectLiteralExpression(o)) return { type: 'object', properties: props, 'x-unresolved': !!o };
  for (const p of o.properties) { if (ts.isSpreadAssignment(p)) { props['x-spread'] = true; continue; }
    if (!ts.isPropertyAssignment(p) && !ts.isShorthandPropertyAssignment(p)) continue; const k = p.name.text ?? p.name.getText(); let init = ts.isPropertyAssignment(p) ? p.initializer : p.name;
    while (init && (ts.isIdentifier(init) || ts.isPropertyAccessExpression(init)) && propVars[init.getText()]) init = propVars[init.getText()];
    if (init && ts.isCallExpression(init) && init.expression.getText().endsWith('(') === false) {
      // unwrap factories like commonProps.x(...) -> can't; only Property.X
      if (/^Property\.\w+$/.test(init.expression.getText())) { const r = propSchema(init, consts); if (r) { props[k] = r.schema; if (r.required) required.push(k); } continue; } }
    props[k] = { 'x-unresolved': true } }
  return { type: 'object', properties: props, required };
}
function authOf(call, consts) { const kind = call.expression.name.text; const a = objProps(call.arguments[0]);
  const base = { type: { OAuth2: 'oauth2', SecretText: 'api_key', BasicAuth: 'basic', CustomAuth: 'custom', None: 'none' }[kind] || kind };
  if (kind === 'OAuth2') { base.authUrl = lit(a.authUrl, consts); base.tokenUrl = lit(a.tokenUrl, consts); base.scope = lit(a.scope, consts); base.pkce = lit(a.pkce, consts) === true; }
  if (kind === 'CustomAuth' || kind === 'OAuth2') { const pr = a.props; base.props = pr ? Object.keys(objProps(pr)) : undefined; }
  base.displayName = lit(a.displayName, consts); return base; }
const pieces = []; let stats = { pieces: 0, actions: 0, triggers: 0, schemaResolved: 0, propsTotal: 0, propsResolved: 0, auth: {} };
for (const dir of fs.readdirSync(ROOT).sort()) { const pdir = path.join(ROOT, dir, 'src'); if (!fs.existsSync(pdir)) continue;
  const files = walk(pdir); const actions = {}, triggers = {}, auths = []; let piece = null; const consts = {}, propVars = {};
  const parsed = files.map(f => ts.createSourceFile(f, fs.readFileSync(f, 'utf8'), ts.ScriptTarget.Latest, true));
  for (const sf of parsed) ts.forEachChild(sf, function v(n) { if (ts.isVariableDeclaration(n) && n.initializer && ts.isIdentifier(n.name)) { const l = lit(n.initializer); if (typeof l === 'string' || Array.isArray(l)) consts[n.name.text] = l; if (ts.isCallExpression(n.initializer) && /^Property\./.test(n.initializer.expression.getText())) propVars[n.name.text] = n.initializer; } ts.forEachChild(n, v); });
  for (const sf of parsed) { const rel = path.relative(path.join(ROOT, dir), sf.fileName);
    ts.forEachChild(sf, function v(n) {
      if (ts.isCallExpression(n)) { const cn = callName(n);
        if (cn === 'createAction' || cn === 'createTrigger') { const a = objProps(n.arguments[0]); const sch = schemaFromProps(a.props, consts, propVars);
          const keys = Object.keys(sch.properties).filter(k => !k.startsWith('x-')); const res = keys.filter(k => !sch.properties[k]['x-unresolved']);
          stats.propsTotal += keys.length; stats.propsResolved += res.length; if (res.length === keys.length && !sch['x-unresolved'] && !sch.properties['x-spread']) stats.schemaResolved++;
          let runLines = 0; if (a.run) runLines = a.run.getText().split('\n').length;
          const src = sf.text; const rec = { name: lit(a.name, consts), displayName: lit(a.displayName, consts), description: lit(a.description, consts), file: rel, inputSchema: sch, runLines, usesSdk: /from '(googleapis|@slack\/web-api|@notionhq|@linear\/sdk|@hubspot|@microsoft\/microsoft-graph|jira)/.test(src), httpCalls: (src.match(/httpClient\.sendRequest|fetch\(|axios/g) || []).length };
          if (cn === 'createTrigger') { rec.triggerType = a.type ? a.type.getText() : undefined; triggers[rec.name] = rec; } else actions[rec.name] = rec; }
        if (/^PieceAuth\.\w+$/.test(cn)) auths.push(authOf(n, consts));
        if (cn === 'createPiece') { const a = objProps(n.arguments[0]); piece = { displayName: lit(a.displayName, consts), description: lit(a.description, consts), logoUrl: lit(a.logoUrl, consts), categories: a.categories && ts.isArrayLiteralExpression(a.categories) ? a.categories.elements.map(e => e.getText().split('.').pop()) : [], authors: lit(a.authors, consts) }; }
      } ts.forEachChild(n, v); }); }
  if (!piece) continue; const acts = Object.values(actions).filter(a => a.name), trs = Object.values(triggers).filter(a => a.name);
  const auth = auths.filter(a => a.type !== 'none'); const primary = auth[0] || { type: 'none' }; stats.auth[primary.type] = (stats.auth[primary.type] || 0) + 1;
  stats.pieces++; stats.actions += acts.length; stats.triggers += trs.length;
  pieces.push({ slug: dir, ...piece, auth: primary, authAlternatives: auth.slice(1), actions: acts, triggers: trs, ctoxImplemented: false, source: 'activepieces/community (MIT)' });
}
fs.writeFileSync(OUT, JSON.stringify({ generatedAt: new Date().toISOString(), source: 'github.com/activepieces/activepieces (MIT)', stats, pieces }, null, 1));
const oauthLit = pieces.filter(p => p.auth.type === 'oauth2' && p.auth.authUrl && p.auth.tokenUrl).length;
console.log(JSON.stringify({ ...stats, oauth2WithLiteralUrls: oauthLit, schemaResolvedPct: (100 * stats.schemaResolved / (stats.actions + stats.triggers)).toFixed(1), propsResolvedPct: (100 * stats.propsResolved / stats.propsTotal).toFixed(1) }));
