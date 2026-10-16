// node:vm-Ersatz für V2-Orakeldateien (v2-oracle.mjs): legt die v4-Brücke als __wjb in jeden Testkontext.
import vm from 'node:vm';

const inject = (ctx) => {
  if (ctx && typeof ctx === 'object') ctx.__wjb = globalThis.__WJB_ORACLE.factory;
  return ctx;
};
export default {
  ...vm,
  createContext: (ctx, ...rest) => vm.createContext(inject(ctx ?? {}), ...rest),
  runInContext: (code, ctx, ...rest) => vm.runInContext(code, inject(ctx), ...rest),
  runInNewContext: (code, ctx, ...rest) => vm.runInNewContext(code, inject(ctx ?? {}), ...rest),
};
