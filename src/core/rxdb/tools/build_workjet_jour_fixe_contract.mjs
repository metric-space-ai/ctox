#!/usr/bin/env node
// One wire fixture generates native types and the browser contract together.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
const fixture = new URL('../tests/fixtures/workjet-jour-fixe-v1.json', import.meta.url);
const spec = JSON.parse(readFileSync(fixture, 'utf8'));
const rustPath = new URL('../../business_os/workjet_jour_fixe_contract.generated.rs', import.meta.url);
const jsPath = new URL('../../../apps/business-os/shared/workjet-jour-fixe-contract.generated.mjs', import.meta.url);
const marker = '// Generated from src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json. Do not edit.\n';
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
  rust += 'Ok(()) } }\n';
}
rust += '\n#[cfg(test)]\npub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {\nmatch kind {\n';
for (const name of Object.keys(spec.types)) rust += `${JSON.stringify(name)} => serde_json::from_value::<${name}>(value).map_err(|e| e.to_string())?.validate(),\n`;
rust += '_ => Err("unknown contract type".into()),\n} }\n';
rust = execFileSync('rustfmt', ['--edition','2021','--emit','stdout'], {input:rust,encoding:'utf8'});
const js = marker + `export const JOUR_FIXE_SCHEMA = ${JSON.stringify(spec.schema)};\nexport const JOUR_FIXE_VERSION = ${spec.contract_version};\nexport const JOUR_FIXE_TYPES = deepFreeze(${JSON.stringify(spec.types,null,2)});\nexport const JOUR_FIXE_COMMANDS = deepFreeze(${JSON.stringify(spec.commands,null,2)});\n` + String.raw`
function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateJourFixeValue(typeName, value) {
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
    const shape = JOUR_FIXE_TYPES[type];
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
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
`;
for (const [path, content] of [[rustPath,rust],[jsPath,js]]) {
  if (process.argv.includes('--check')) {
    if (readFileSync(path,'utf8') !== content) throw new Error('Generated Jour fixe contract is stale: ' + path.pathname);
  } else writeFileSync(path,content);
}
console.log(`Jour fixe v${spec.contract_version}: native/browser contract ${process.argv.includes('--check') ? 'current' : 'generated'}`);
