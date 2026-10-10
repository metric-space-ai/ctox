#!/usr/bin/env node
// One wire fixture generates native types and the browser contract together.
// Each entry in CONTRACTS is one fixture; `prefix` names its browser exports and `validator`
// names its browser validation function. Jour fixe keeps its original names for existing consumers.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const CONTRACTS = [
  {
    fixture: 'workjet-worker-outcome-v1.json',
    rust: '../../business_os/workjet_worker_outcome_contract.generated.rs',
    js: '../../../apps/business-os/shared/workjet-worker-outcome-contract.generated.mjs',
    prefix: 'WORKER_OUTCOME',
    validator: 'validateWorkerOutcomeValue',
  },
  {
    fixture: 'workjet-supervisor-luma-v1.json',
    rust: '../../business_os/workjet_supervisor_luma_contract.generated.rs',
    js: '../../../apps/business-os/shared/workjet-supervisor-luma-contract.generated.mjs',
    prefix: 'SUPERVISOR_LUMA',
    validator: 'validateSupervisorLumaValue',
  },
  {
    fixture: 'workjet-jour-fixe-v1.json',
    rust: '../../business_os/workjet_jour_fixe_contract.generated.rs',
    js: '../../../apps/business-os/shared/workjet-jour-fixe-contract.generated.mjs',
    prefix: 'JOUR_FIXE',
    validator: 'validateJourFixeValue',
  },
  {
    fixture: 'workjet-calendar-v1.json',
    rust: '../../business_os/workjet_calendar_contract.generated.rs',
    js: '../../../apps/business-os/shared/workjet-calendar-contract.generated.mjs',
    prefix: 'CALENDAR',
    validator: 'validateCalendarValue',
  },
];

function buildContract({ fixture: fixtureName, rust: rustRel, js: jsRel, prefix, validator }) {
  const spec = JSON.parse(readFileSync(new URL('../tests/fixtures/' + fixtureName, import.meta.url), 'utf8'));
  const rustPath = new URL(rustRel, import.meta.url);
  const jsPath = new URL(jsRel, import.meta.url);
  const marker = `// Generated from src/core/rxdb/tests/fixtures/${fixtureName}. Do not edit.\n`;
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
        const condition = key === 'min_items' && f[key] === 1
          ? 'value.is_empty()'
          : `${expr[key]} ${op} ${bound}`;
        rust += `if ${condition} { return Err(${JSON.stringify(name+'.'+field+' violates '+key)}.into()); }\n`;
      }
      rust += '}\n';
    }
    for (const order of type.ordered_fields ?? []) {
      if (!type.fields[order.before] || !type.fields[order.after]) throw new Error(name + ': unknown ordered field');
      rust += `if self.${order.after} <= self.${order.before} { return Err(${JSON.stringify(name + '.' + order.after + ' must follow ' + order.before)}.into()); }\n`;
    }
    rust += 'Ok(()) } }\n';
  }
  rust += '\n#[cfg(test)]\npub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {\nmatch kind {\n';
  for (const name of Object.keys(spec.types)) rust += `${JSON.stringify(name)} => serde_json::from_value::<${name}>(value).map_err(|e| e.to_string())?.validate(),\n`;
  rust += '_ => Err("unknown contract type".into()),\n} }\n';
  rust = execFileSync('rustfmt', ['--edition','2021','--emit','stdout'], {input:rust,encoding:'utf8'});
  const js = marker + `export const ${prefix}_SCHEMA = ${JSON.stringify(spec.schema)};\nexport const ${prefix}_VERSION = ${spec.contract_version};\nexport const ${prefix}_TYPES = deepFreeze(${JSON.stringify(spec.types,null,2)});\nexport const ${prefix}_COMMANDS = deepFreeze(${JSON.stringify(spec.commands ?? {},null,2)});\n` + String.raw`
function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function __VALIDATOR__(typeName, value) {
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
    const shape = __PREFIX___TYPES[type];
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
    for (const order of shape.ordered_fields ?? []) {
      if (value[order.after] <= value[order.before]) throw new Error(field + '.' + order.after + ': must follow ' + order.before);
    }
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
`.replaceAll('__PREFIX__', prefix).replaceAll('__VALIDATOR__', validator);
  return [[rustPath, rust], [jsPath, js], fixtureName];
}

const checking = process.argv.includes('--check');
for (const contract of CONTRACTS) {
  const [[rustPath, rust], [jsPath, js], fixtureName] = buildContract(contract);
  for (const [path, content] of [[rustPath, rust], [jsPath, js]]) {
    if (checking) {
      if (readFileSync(path, 'utf8') !== content) throw new Error('Generated contract is stale: ' + path.pathname);
    } else writeFileSync(path, content);
  }
  console.log(`${fixtureName}: native/browser contract ${checking ? 'current' : 'generated'}`);
}
