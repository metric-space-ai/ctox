#!/usr/bin/env node
// One fixture generates the native types and browser observer validator.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
const fixturePath = new URL('../tests/fixtures/workjet-supervisor-execution-v1.json', import.meta.url);
const spec = JSON.parse(readFileSync(fixturePath, 'utf8'));
const marker = '// Generated from src/core/rxdb/tests/fixtures/workjet-supervisor-execution-v1.json. Do not edit.\n';
let rust = marker + `#![allow(dead_code)]\nuse serde::{Deserialize, Serialize};\npub(crate) const CONTRACT_SCHEMA: &str = ${JSON.stringify(spec.schema)};\npub(crate) const CONTRACT_VERSION: u64 = ${spec.contract_version};\npub(crate) trait WireValidate { fn validate(&self) -> Result<(), String>; }\n`;
for (const type of ['String', 'bool', 'u64', 'i64']) {
  rust += `impl WireValidate for ${type} { fn validate(&self)->Result<(),String> { `;
  if (type === 'u64') rust += 'if *self>9_007_199_254_740_991 { return Err("unsafe JSON integer".into()); } ';
  if (type === 'i64') rust += 'if self.unsigned_abs()>9_007_199_254_740_991 { return Err("unsafe JSON integer".into()); } ';
  rust += 'Ok(()) } }\n';
}
rust += 'impl<T:WireValidate> WireValidate for Vec<T> { fn validate(&self)->Result<(),String> { for item in self { item.validate()?; } Ok(()) } }\n';
for (const [name, { fields }] of Object.entries(spec.types)) {
  rust += `#[derive(Debug,Clone,Deserialize,Serialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct ${name} {\n`;
  for (const [field, rule] of Object.entries(fields)) {
    if (rule.optional) rust += '#[serde(default,skip_serializing_if="Option::is_none")]\n';
    rust += `pub(crate) ${field}: ${rule.optional ? `Option<${rule.type}>` : rule.type},\n`;
  }
  rust += `}\nimpl WireValidate for ${name} { fn validate(&self)->Result<(),String> {\n`;
  for (const [field, rule] of Object.entries(fields)) {
    rust += rule.optional ? `if let Some(value)=&self.${field} {\n` : `{ let value=&self.${field};\n`;
    rust += 'value.validate()?;\n';
    if (rule.type === 'String' && rule.min_chars) rust += `if value.trim().is_empty() { return Err(${JSON.stringify(name+'.'+field+' is blank')}.into()); }\n`;
    const expressions = { min_chars: 'value.chars().count()', max_chars: 'value.chars().count()', max_items: 'value.len()', minimum: '*value', maximum: '*value' };
    for (const [key, expression] of Object.entries(expressions)) {
      if (rule[key] === undefined || (rule.type === 'u64' && key === 'minimum' && rule[key] === 0)) continue;
      rust += `if ${expression} ${key.startsWith('max') ? '>' : '<'} ${rule[key]} { return Err(${JSON.stringify(name+'.'+field+' violates '+key)}.into()); }\n`;
    }
    rust += '}\n';
  }
  rust += 'Ok(()) } }\n';
}
rust += '#[cfg(test)]\npub(crate) fn validate_fixture(kind:&str,value:serde_json::Value)->Result<(),String> { match kind {\n';
for (const name of Object.keys(spec.types)) rust += `${JSON.stringify(name)}=>serde_json::from_value::<${name}>(value).map_err(|e|e.to_string())?.validate(),\n`;
rust += '_=>Err("unknown observer contract type".into()) } }\n';
rust = execFileSync('rustfmt', ['--edition','2021','--emit','stdout'], { input: rust, encoding: 'utf8' });
const js = marker + `export const SUPERVISOR_EXECUTION_SCHEMA = ${JSON.stringify(spec.schema)};\nexport const SUPERVISOR_EXECUTION_VERSION = ${spec.contract_version};\nexport const SUPERVISOR_EXECUTION_TYPES = deepFreeze(${JSON.stringify(spec.types,null,2)});\nexport const SUPERVISOR_EXECUTION_COMMANDS = deepFreeze(${JSON.stringify(spec.commands,null,2)});\n` + String.raw`
function deepFreeze(value) { for (const child of Object.values(value)) if (child && typeof child === 'object') deepFreeze(child); return Object.freeze(value); }
export function validateSupervisorExecutionValue(type, value) {
  if (type === 'String') { if (typeof value !== 'string') throw new Error('expected string'); return value; }
  if (type === 'bool') { if (typeof value !== 'boolean') throw new Error('expected boolean'); return value; }
  if (type === 'u64' || type === 'i64') { if (!Number.isSafeInteger(value) || (type === 'u64' && value < 0)) throw new Error('expected safe integer'); return value; }
  if (type.startsWith('Vec<')) { if (!Array.isArray(value)) throw new Error('expected array'); for (const item of value) validateSupervisorExecutionValue(type.slice(4,-1),item); return value; }
  const definition = SUPERVISOR_EXECUTION_TYPES[type];
  if (!definition || !value || typeof value !== 'object' || Array.isArray(value)) throw new Error('invalid observer object');
  for (const key of Object.keys(value)) if (!Object.hasOwn(definition.fields,key)) throw new Error('unknown field '+key);
  for (const [key, rule] of Object.entries(definition.fields)) {
    const field=value[key];
    if (field === undefined || field === null) { if (rule.optional) continue; throw new Error('missing '+key); }
    validateSupervisorExecutionValue(rule.type,field);
    if (rule.type === 'String' && rule.min_chars && /^[\p{White_Space}]*$/u.test(field)) throw new Error('blank '+key);
    const expressions={min_chars:()=>Array.from(field).length,max_chars:()=>Array.from(field).length,max_items:()=>field.length,minimum:()=>field,maximum:()=>field};
    for (const [bound,expression] of Object.entries(expressions)) if (rule[bound] !== undefined && (bound.startsWith('max') ? expression()>rule[bound] : expression()<rule[bound])) throw new Error('bound '+key);
  }
  return value;
}
`;
const {validateSupervisorExecutionValue: validate} = await import('data:text/javascript;base64,'+Buffer.from(js).toString('base64'));
for (const test of spec.valid_cases) validate(test.type,test.value);
for (const test of spec.invalid_cases) { let rejected=false; try { validate(test.type,test.value); } catch { rejected=true; } if (!rejected) throw new Error('invalid fixture was accepted'); }
for (const [path, output] of [[new URL('../../business_os/workjet_supervisor_execution_contract.generated.rs',import.meta.url),rust],[new URL('../../../apps/business-os/shared/workjet-supervisor-execution-contract.generated.mjs',import.meta.url),js]]) {
  if (process.argv.includes('--check')) { if (readFileSync(path,'utf8')!==output) throw new Error('stale observer contract '+path.pathname); }
  else writeFileSync(path,output);
}
console.log('Supervisor execution native/browser contract '+(process.argv.includes('--check')?'current':'generated'));
