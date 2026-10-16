// node:test-Ersatz für V2-Orakeldateien (v2-oracle.mjs): registriert nur freigegebene Tests, zählt den Rest.
import realTest from 'node:test';

const cfg = () => globalThis.__WJB_ORACLE;
export default function test(name, ...rest) {
  const c = cfg();
  if (c?.allow(name)) {
    c.registered.push(name);
    return realTest(`[V2-Orakel] ${name}`, ...rest);
  }
  c?.skipped.push(name);
  return undefined;
}
test.skip = (name) => { cfg()?.skipped.push(name); };
test.todo = (name) => { cfg()?.skipped.push(name); };
test.only = test;
