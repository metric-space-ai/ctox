#!/usr/bin/env node
// Conservative native opener ratchet, including tests/vendor Rust. This is a
// source inventory, not proof that indirect/dynamic database access is owned.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const BASELINE = 850; // Measured at the assignment base; lowered as openers retire.

function sourceOnly(text) {
  const chars = text.split('');
  let i = 0;
  const blank = end => { for (; i < end; i++) if (chars[i] !== '\n') chars[i] = ' '; };
  while (i < chars.length) {
    if (text.startsWith('//', i)) { const end = text.indexOf('\n', i); blank(end < 0 ? chars.length : end); }
    else if (text.startsWith('/*', i)) {
      let end = i + 2, depth = 1;
      while (end < chars.length && depth) {
        if (text.startsWith('/*', end)) { depth++; end += 2; }
        else if (text.startsWith('*/', end)) { depth--; end += 2; }
        else end++;
      }
      blank(end);
    } else {
      const char = chars[i] === "'" ? /^'(?:\\.|[^'\\\n])'/.exec(text.slice(i, i + 12)) : null;
      if (char) { blank(i + char[0].length); continue; }
      const raw = chars[i] === 'r' ? /^r(#{0,255})"/.exec(text.slice(i, i + 258)) : null;
      if (raw) {
        const ending = '"' + raw[1];
        const end = text.indexOf(ending, i + raw[0].length);
        blank(end < 0 ? chars.length : end + ending.length);
      } else if (chars[i] === '"') {
        let end = i + 1;
        while (end < chars.length) {
          if (chars[end] === '\\') { end += 2; continue; }
          if (chars[end++] === '"') break;
        }
        blank(end);
      } else i++;
    }
  }
  return chars.join('');
}

function sites(file, original) {
  const text = sourceOnly(original);
  const matches = new Map();
  const add = (offset, kind) => {
    const prefix = text.slice(0, offset);
    if (/\bfn\s*$/.test(prefix)) return;
    const line = prefix.split('\n').length;
    matches.set(`${line}:${kind}`, { file, line, kind });
  };
  if (text.includes('rusqlite')) {
    const names = new Set(['Connection']);
    for (const match of text.matchAll(/\bConnection\s+as\s+(\w+)/g)) names.add(match[1]);
    for (const name of names) {
      const pattern = new RegExp(`\\b${name}\\s*::\\s*(open(?:_with_flags(?:_and_vfs)?|_in_memory(?:_with_flags(?:_and_vfs)?)?|_with_flags_and_vfs|_with_vfs)?)\\s*\\(`, 'g');
      for (const match of text.matchAll(pattern)) add(match.index, `rusqlite.${match[1]}`);
    }
  }
  for (const match of text.matchAll(/\b(?:SqliteConnection|SqlitePool)\s*::\s*(connect(?:_with|_lazy(?:_with)?)?)\s*\(/g)) add(match.index, `sqlx.${match[1]}`);
  // Builder expression, including multiline chains. Semicolons terminate it.
  for (const builder of text.matchAll(/\b(?:SqlitePoolOptions|PoolOptions\s*::\s*<\s*Sqlite\s*>)\s*::\s*new\s*\(/g)) {
    const end = text.indexOf(';', builder.index);
    const expression = text.slice(builder.index, end < 0 ? text.length : end);
    for (const match of expression.matchAll(/\.\s*(connect(?:_with|_lazy(?:_with)?)?)\s*\(/g)) add(builder.index + match.index, `sqlx.pool.${match[1]}`);
  }
  for (const match of text.matchAll(/\bsqlite3_open(?:_v2|16)?\s*\(/g)) add(match.index, 'sqlite.ffi.open');
  return [...matches.values()];
}

function inventory(root, ref) {
  const git = args => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const files = (ref ? git(['ls-tree', '-r', '--name-only', '-z', ref]) : git(['ls-files', '--cached', '--others', '--exclude-standard', '-z'])).split('\0');
  const changed = ref ? new Set(git(['diff', '--name-only', '-z', ref, '--']).split('\0')) : null;
  const result = [];
  for (const file of new Set(files)) {
    if (!file.endsWith('.rs') || file.startsWith('src/core/store/')) continue;
    let text;
    if (ref && (changed.has(file) || !fs.existsSync(path.join(root, file)))) text = git(['show', `${ref}:${file}`]);
    else if (fs.existsSync(path.join(root, file))) text = fs.readFileSync(path.join(root, file), 'utf8');
    else continue;
    result.push(...sites(file, text));
  }
  return result.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line || a.kind.localeCompare(b.kind));
}

const args = process.argv.slice(2);
const value = option => { const i = args.indexOf(option); return i < 0 ? undefined : args[i + 1]; };
if (args.includes('--self-test')) {
  const root = fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'sqlite-ratchet-'));
  try {
    execFileSync('git', ['init', '-q', root]);
    fs.writeFileSync(path.join(root, 'new.rs'), 'use rusqlite::Connection; fn new_writer() { Connection::open("fixture"); }\n');
    const child = spawnSync(process.execPath, [process.argv[1], '--root', root, '--max', '0'], { encoding: 'utf8' });
    assert.notEqual(child.status, 0, 'new direct SQLite opener must violate the ratchet');
    assert.match(child.stderr, /SQLite opener ratchet exceeded/);
    fs.mkdirSync(path.join(root, 'src/core/store'), { recursive: true });
    fs.renameSync(path.join(root, 'new.rs'), path.join(root, 'src/core/store/owner.rs'));
    const owner = spawnSync(process.execPath, [process.argv[1], '--root', root, '--max', '0'], { encoding: 'utf8' });
    assert.equal(owner.status, 0, owner.stderr);
    console.log('SQLite opener addition rejected; store-owned relocation admitted');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
} else {
  const root = path.resolve(value('--root') || process.cwd());
  const ref = value('--ref');
  const locations = inventory(root, ref);
  const count = locations.length;
  const ceiling = value('--max') === undefined ? BASELINE : Number(value('--max'));
  if (!Number.isSafeInteger(ceiling) || ceiling < 0) throw new Error('invalid inventory ceiling');
  console.log(JSON.stringify({ count, ceiling, ref: ref || null, scope: 'tracked and nonignored Rust; tests/vendor included; src/core/store excluded', locations }, null, 2));
  if (!args.includes('--print') && count > ceiling) {
    console.error(`SQLite opener ratchet exceeded: ${count} > ${ceiling}`);
    process.exitCode = 1;
  }
}
