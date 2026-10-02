import assert from 'node:assert/strict';
import test from 'node:test';
import { functionBody } from './rust-function-body.mjs';

test('static lifetime and nested initialization stop at the classifier end', () => {
  const body = " static EXACT_TYPES: OnceLock<HashSet<&'static str>> = OnceLock::new(); const types = EXACT_TYPES.get_or_init(|| { VALUES.into_iter().collect() }); types.contains(command_type) ";
  const source = 'pub(super) fn classify(command_type: &str) -> bool {' + body + '} fn unrelated() { "unrelated.command" }';
  assert.equal(functionBody(source, 'classify'), body);
});

test('named and inferred lifetimes preserve classifier strings', () => {
  const body = " let first: &'a str = \"first.command\"; let _: Thing<'_> = value; second() ";
  assert.equal(functionBody('fn classify() {' + body + '} fn later() {}', 'classify'), body);
});

for (const literal of ["'}'", "'{'", "'é'", "'\\''", "'\\\\'", "'\\x7d'", "'\\u{7d}'"]) {
  test('character literal ' + literal + ' does not alter brace depth', () => {
    const body = ' let value = ' + literal + '; "{ignored}"; nested({ value }) ';
    assert.equal(functionBody('fn classify() {' + body + '} fn later() {}', 'classify'), body);
  });
}

test('optional absence is retained and required absence rejected', () => {
  assert.equal(functionBody('fn other() {}', 'classify', false), '');
  assert.throws(() => functionBody('fn other() {}', 'classify'), /cannot locate Rust function/);
});

test('incomplete bodies and strings remain rejected', () => {
  for (const source of ['fn classify() { nested({})', 'fn classify() { "unfinished }']) {
    assert.throws(() => functionBody(source, 'classify'), /unterminated Rust function/);
  }
});
