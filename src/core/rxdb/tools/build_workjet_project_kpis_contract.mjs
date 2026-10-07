#!/usr/bin/env node
// One wire fixture generates native types and the browser contract together.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
const fixture = new URL('../tests/fixtures/workjet-project-kpis-v1.json', import.meta.url);
const spec = JSON.parse(readFileSync(fixture, 'utf8'));
const rustPath = new URL('../../business_os/workjet_project_kpis_contract.generated.rs', import.meta.url);
const jsPath = new URL('../../../apps/business-os/shared/workjet-project-kpis-contract.generated.mjs', import.meta.url);
const marker = '// Generated from src/core/rxdb/tests/fixtures/workjet-project-kpis-v1.json. Do not edit.\n';
let rust = marker + `#![allow(dead_code)]\nuse serde::{Deserialize, Serialize};\n\npub(crate) const CONTRACT_VERSION: u64 = ${spec.contract_version};\npub(crate) const CONTRACT_SCHEMA: &str = ${JSON.stringify(spec.schema)};\n\npub(crate) trait WireValidate { fn validate(&self) -> Result<(), String>; }\n`;
for (const t of ['String', 'bool', 'u64', 'i64', 'f64']) {
  rust += `impl WireValidate for ${t} { fn validate(&self) -> Result<(), String> { `;
  if (t === 'f64') rust += 'if !self.is_finite() { return Err("non-finite number".into()); } ';
  if (t === 'u64') rust += 'if *self > 9_007_199_254_740_991 { return Err("unsafe JSON integer".into()); } ';
  if (t === 'i64') rust += 'if self.unsigned_abs() > 9_007_199_254_740_991 { return Err("unsafe JSON integer".into()); } ';
  rust += 'Ok(()) } }\n';
}
rust += 'impl<T: WireValidate> WireValidate for Vec<T> { fn validate(&self) -> Result<(), String> { for item in self { item.validate()?; } Ok(()) } }\n';
for (const [name, type] of Object.entries(spec.types)) {
  rust += type.enum ? '\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]\n' : '\n#[derive(Debug, Clone, Deserialize, Serialize)]\n';
  if (type.enum) {
    rust += `pub(crate) enum ${name} {\n` + type.enum.map(v => `#[serde(rename = ${JSON.stringify(v)})]\n${v.split(/[^A-Za-z0-9]+/).map(part => part[0].toUpperCase() + part.slice(1)).join('')},`).join('\n') + '\n}\n';
    rust += `impl WireValidate for ${name} { fn validate(&self) -> Result<(), String> { Ok(()) } }\n`;
    continue;
  }
  rust += '#[serde(deny_unknown_fields)]\n' + `pub(crate) struct ${name} {\n`;
  for (const [field, f] of Object.entries(type.fields)) {
    if (f.optional) rust += '#[serde(default, skip_serializing_if = "Option::is_none")]\n';
    rust += `pub(crate) ${field}: ${f.optional ? `Option<${f.type}>` : f.type},\n`;
  }
  rust += `}\nimpl WireValidate for ${name} { fn validate(&self) -> Result<(), String> {\n`;
  for (const [field, f] of Object.entries(type.fields)) {
    rust += f.optional ? `if let Some(value) = &self.${field} {\n` : `{ let value = &self.${field};\n`;
    rust += 'value.validate()?;\n';
    const expr = { min_chars:'value.chars().count()', max_chars:'value.chars().count()', min_items:'value.len()', max_items:'value.len()', minimum:'*value', maximum:'*value' };
    for (const [key, op] of Object.entries({min_chars:'<',max_chars:'>',min_items:'<',max_items:'>',minimum:'<',maximum:'>'})) {
      if (f[key] === undefined) continue;
      const bound = f.type === 'f64' && ['minimum','maximum'].includes(key) ? Number(f[key]).toFixed(1) : String(f[key]);
      rust += `if ${expr[key]} ${op} ${bound} { return Err(${JSON.stringify(name+'.'+field+' violates '+key)}.into()); }\n`;
    }
    rust += '}\n';
  }
  rust += `validate_rules(${JSON.stringify(name)}, &serde_json::to_value(self).map_err(|e| e.to_string())?)?;\n`;
  rust += 'Ok(()) } }\n';
}
rust += '\n#[cfg(test)]\npub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {\nmatch kind {\n';
for (const name of Object.keys(spec.types)) rust += `${JSON.stringify(name)} => serde_json::from_value::<${name}>(value).map_err(|e| e.to_string())?.validate(),\n`;
rust += '_ => Err("unknown contract type".into()),\n} }\n';
const rulesJson = JSON.stringify(spec.rules);
rust += `\nfn rules() -> &'static serde_json::Value { static RULES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new(); RULES.get_or_init(|| serde_json::from_str(r#"${rulesJson}"#).expect("generated KPI rules")) }\n`;
rust += String.raw`
fn at<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cursor = value;
    for key in path.split('.') { cursor = cursor.get(key)?; }
    if cursor.is_null() { None } else { Some(cursor) }
}
fn text(value: &serde_json::Value) -> &str { value.as_str().expect("generated rule string") }
fn validate_rules(kind: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(rules) = rules().get(kind) else { return Ok(()); };
    let fail = |rule: &str| format!("{kind}: {rule}");
    if let Some(fields) = rules["nonblank"].as_array() { for field in fields {
        if at(value,text(field)).and_then(|v| v.as_str()).is_some_and(|v| v.chars().all(|c| matches!(c,' '|'\t'|'\r'|'\n'))) { return Err(fail("blank prompt or identity")); }
    } }
    for operation in ["eq","lt","lte"] { if let Some(pairs) = rules[operation].as_array() { for pair in pairs {
        let (Some(left),Some(right)) = (at(value,text(&pair[0])),at(value,text(&pair[1]))) else { continue; };
        let ok = match operation { "eq" => left==right, "lt" => left.as_i64()<right.as_i64(), _ => left.as_i64()<=right.as_i64() };
        if !ok { return Err(fail(operation)); }
    } } }
    if let Some(items) = rules["unique"].as_array() { for rule in items {
        let Some(values) = at(value,text(&rule["field"])).and_then(|v| v.as_array()) else { continue; };
        let keys: Vec<_> = values.iter().filter_map(|v| if let Some(key) = rule["key"].as_str() { at(v,key) } else { Some(v) }).collect();
        for i in 0..keys.len() { if keys[i+1..].contains(&keys[i]) { return Err(fail("duplicate identity or input")); } }
    } }
    for operation in ["every_eq","every_lte"] { if let Some(items) = rules[operation].as_array() { for rule in items {
        let target = at(value,text(&rule["target"]));
        if let Some(values) = at(value,text(&rule["field"])).and_then(|v| v.as_array()) { for item in values {
            if let (Some(left),Some(right)) = (at(item,text(&rule["key"])),target) {
                let ok = if operation=="every_eq" { left==right } else { left.as_i64()<=right.as_i64() };
                if !ok { return Err(fail(operation)); }
            }
        } }
    } } }
    if let Some(state_rule) = rules.get("state_fields") {
        let state = at(value,text(&state_rule["field"])).and_then(|v| v.as_str()).ok_or_else(|| fail("missing state"))?;
        let state_fields = &state_rule["states"][state];
        for (mode,required) in [("required",true),("forbidden",false)] { if let Some(fields) = state_fields[mode].as_array() { for field in fields {
            if at(value,text(field)).is_some()!=required { return Err(fail(mode)); }
        } } }
    }
    if rules["calculate"]==true {
        let sources = value["sources"].as_array().ok_or_else(|| fail("sources"))?;
        let keys = value["computation"]["input_keys"].as_array().ok_or_else(|| fail("input keys"))?;
        if sources.len()!=keys.len() { return Err(fail("all evidence must be consumed")); }
        let mut inputs = Vec::new();
        for key in keys {
            let source = sources.iter().find(|s| s["source_key"]==*key).ok_or_else(|| fail("unknown input"))?;
            inputs.push(source["value"].as_f64().ok_or_else(|| fail("numeric input"))?);
        }
        let expected = match value["computation"]["operation"].as_str() {
            Some("identity") if inputs.len()==1 => inputs[0],
            Some("sum") => inputs.iter().sum(),
            Some("average") => inputs.iter().sum::<f64>()/(inputs.len() as f64),
            Some("percentage") if inputs.len()==2 && inputs[1]!=0.0 => 100.0*inputs[0]/inputs[1],
            _ => return Err(fail("invalid calculation arity or denominator")),
        };
        if !expected.is_finite() || value["value"].as_f64()!=Some(expected) { return Err(fail("value differs from evidence")); }
    }
    Ok(())
}
`;
rust = execFileSync('rustfmt', ['--edition','2021','--emit','stdout'], {input:rust,encoding:'utf8'});
const js = marker + `export const PROJECT_KPIS_SCHEMA = ${JSON.stringify(spec.schema)};\nexport const PROJECT_KPIS_VERSION = ${spec.contract_version};\nexport const PROJECT_KPIS_TYPES = deepFreeze(${JSON.stringify(spec.types,null,2)});\nexport const PROJECT_KPIS_COMMANDS = deepFreeze(${JSON.stringify(spec.commands,null,2)});\nexport const PROJECT_KPIS_RULES = deepFreeze(${JSON.stringify(spec.rules,null,2)});\n` + String.raw`
function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateProjectKpiValue(typeName, value) {
  function validate(type, value, field) {
    if (type.startsWith('Vec<')) {
      if (!Array.isArray(value)) throw new Error(field + ': array required');
      value.forEach(v => validate(type.slice(4,-1), v, field)); return;
    }
    if (type === 'String') { if (typeof value !== 'string') throw new Error(field + ': string required'); return; }
    if (type === 'bool') { if (typeof value !== 'boolean') throw new Error(field + ': boolean required'); return; }

    if (['u64','i64','f64'].includes(type)) {
      if (typeof value !== 'number' || !Number.isFinite(value)
          || (type !== 'f64' && (!Number.isSafeInteger(value) || (type === 'u64' && value < 0)))) throw new Error(field + ': invalid number');
      return;
    }
    const shape = PROJECT_KPIS_TYPES[type];
    if (!shape) throw new Error(field + ': unknown type');
    if (shape.enum) { if (!shape.enum.includes(value)) throw new Error(field + ': invalid enum'); return; }
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(field + ': object required');
    for (const key of Object.keys(value)) if (!Object.hasOwn(shape.fields, key)) throw new Error(field + ': unexpected ' + key);
    for (const [key, definition] of Object.entries(shape.fields)) {
      const v = value[key], at = field + '.' + key;
      if (v === undefined || v === null) { if (definition.optional) continue; throw new Error(at + ': required'); }
      validate(definition.type, v, at);
      for (const [constraint,bound] of Object.entries(definition)) {
        const metric = constraint.endsWith('_chars') ? [...v].length : constraint.endsWith('_items') ? v.length : v;
        if ((['minimum','min_chars','min_items'].includes(constraint) && metric < bound)
            || (['maximum','max_chars','max_items'].includes(constraint) && metric > bound)) throw new Error(at + ': ' + constraint);
      }
    }
    validateRules(type, value);
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
function at(value, path) {
  for (const key of path.split('.')) value = value?.[key];
  return value ?? undefined;
}
function validateRules(kind, value) {
  const rules = PROJECT_KPIS_RULES[kind];
  if (!rules) return;
  const fail = rule => { throw new Error(kind + ': ' + rule); };
  for (const field of rules.nonblank || []) if (/^[ \t\r\n]*$/.test(at(value,field))) fail('blank prompt or identity');
  for (const operation of ['eq','lt','lte']) for (const [a,b] of rules[operation] || []) {
    const left=at(value,a), right=at(value,b);
    if (left===undefined || right===undefined) continue;
    if (!(operation==='eq' ? left===right : operation==='lt' ? left<right : left<=right)) fail(operation);
  }
  for (const rule of rules.unique || []) {
    const keys=(at(value,rule.field) || []).map(v => rule.key ? at(v,rule.key) : v);
    if (new Set(keys).size!==keys.length) fail('duplicate identity or input');
  }
  for (const operation of ['every_eq','every_lte']) for (const rule of rules[operation] || []) {
    const right=at(value,rule.target);
    for (const item of at(value,rule.field) || []) {
      const left=at(item,rule.key);
      if (left===undefined || right===undefined) continue;
      if (!(operation==='every_eq' ? left===right : left<=right)) fail(operation);
    }
  }
  if (rules.state_fields) {
    const fields=rules.state_fields.states[at(value,rules.state_fields.field)];
    if (!fields) fail('missing state');
    for (const field of fields.required || []) if (at(value,field)===undefined) fail('required');
    for (const field of fields.forbidden || []) if (at(value,field)!==undefined) fail('forbidden');
  }
  if (rules.calculate) {
    const keys=value.computation.input_keys;
    if (keys.length!==value.sources.length) fail('all evidence must be consumed');
    const inputs=keys.map(key => {
      const source=value.sources.find(s => s.source_key===key);
      if (!source) fail('unknown input');
      return source.value;
    });
    const total=inputs.reduce((a,b) => a+b,0);
    let expected;
    switch (value.computation.operation) {
      case 'identity': if (inputs.length!==1) fail('arity'); expected=inputs[0]; break;
      case 'sum': expected=total; break;
      case 'average': expected=total/inputs.length; break;
      case 'percentage': if (inputs.length!==2 || inputs[1]===0) fail('arity or denominator'); expected=100*inputs[0]/inputs[1]; break;
      default: fail('calculation');
    }
    if (!Number.isFinite(expected) || value.value!==expected) fail('value differs from evidence');
  }
}

`;
for (const [path, content] of [[rustPath,rust],[jsPath,js]]) {
  if (process.argv.includes('--check')) {
    if (readFileSync(path,'utf8') !== content) throw new Error('Generated Project KPI contract is stale: ' + path.pathname);
  } else writeFileSync(path,content);
}
console.log(`Project KPI v${spec.contract_version}: native/browser contract ${process.argv.includes('--check') ? 'current' : 'generated'}`);
