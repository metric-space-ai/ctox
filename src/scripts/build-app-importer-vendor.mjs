#!/usr/bin/env node
// Builds the pinned browser-ESM vendor bundles for the Business OS App
// Importer: sucrase (TSX/TS -> plain ESM transcoder) and the React 19
// runtime family. See docs/business-os-app-importer.md for the decision
// record and the three esbuild pitfalls these builds work around.
//
// Usage:
//   node src/scripts/build-app-importer-vendor.mjs <workdir> <outdir>
//
// <workdir> must contain a package.json; the script installs the PINNED
// dependencies there (never in the repo) and writes bundles + provenance
// to <outdir>. Re-running is idempotent.

import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const PINS = {
  sucrase: "3.35.1",
  react: "19.2.7",
  "react-dom": "19.2.7",
  esbuild: "0.28.1",
};

const [workdirArg, outdirArg] = process.argv.slice(2);
if (!workdirArg || !outdirArg) {
  console.error("usage: build-app-importer-vendor.mjs <workdir> <outdir>");
  process.exit(1);
}
const workdir = resolve(workdirArg);
const outdir = resolve(outdirArg);
mkdirSync(workdir, { recursive: true });
mkdirSync(outdir, { recursive: true });

execSync("npm init -y", { cwd: workdir, stdio: "ignore" });
const spec = Object.entries(PINS).map(([name, v]) => `${name}@${v}`).join(" ");
execSync(`npm install ${spec} --no-audit --no-fund --save-exact`, {
  cwd: workdir,
  stdio: "inherit",
});

const esbuild = await import(join(workdir, "node_modules/esbuild/lib/main.js"));

// Pitfall 1: `external: ['react']` would also externalize react/jsx-runtime,
// making that bundle resolve to itself through the import map. Match exactly.
const onlyReactExternal = {
  name: "only-react-external",
  setup(build) {
    build.onResolve({ filter: /^react$/ }, () => ({ path: "react", external: true }));
  },
};

// Pitfall 2: CJS `require('react')` survives into ESM output when react is
// external. Shim it onto the ESM import.
const requireShim = `import * as __react from 'react';
const __reactM = __react.default ?? __react;
const require = (m) => { if (m === 'react') return __reactM; throw new Error('unresolved require: ' + m); };
`;

// Pitfall 3: `export * from` over a CJS module loses named exports.
// Re-export explicitly through a default-interop object.
const interopNamed = (specifier, names) => `
import * as ns from '${specifier}';
const m = ns.default ?? ns;
export default m;
` + names.map((n) => `export const ${n} = m.${n};`).join("\n");

const REACT_NAMED = [
  "useState", "useEffect", "useMemo", "useRef", "useCallback", "useContext",
  "useReducer", "useId", "useSyncExternalStore", "useTransition",
  "useDeferredValue", "useLayoutEffect", "useImperativeHandle",
  "useDebugValue", "useInsertionEffect", "createElement", "createContext",
  "cloneElement", "isValidElement", "Children", "Component", "PureComponent",
  "Fragment", "StrictMode", "Suspense", "memo", "forwardRef", "lazy",
  "startTransition", "version", "act", "cache", "use",
];

const common = {
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2020",
  minify: true,
  define: { "process.env.NODE_ENV": '"production"' },
  absWorkingDir: workdir,
};

const builds = [
  {
    out: "sucrase.mjs",
    contents: "export { transform } from 'sucrase';",
    extra: { define: { ...common.define, global: "globalThis" } },
  },
  {
    out: "react.mjs",
    contents: interopNamed("react", REACT_NAMED),
  },
  {
    out: "react-jsx-runtime.mjs",
    contents: interopNamed("react/jsx-runtime", ["jsx", "jsxs", "Fragment"]),
    extra: { plugins: [onlyReactExternal], banner: { js: requireShim } },
  },
  {
    out: "react-dom-client.mjs",
    contents: interopNamed("react-dom/client", ["createRoot", "hydrateRoot"]),
    extra: { plugins: [onlyReactExternal], banner: { js: requireShim } },
  },
  {
    out: "react-dom.mjs",
    contents: interopNamed("react-dom", ["createPortal", "flushSync", "preload", "preinit", "version"]),
    extra: { plugins: [onlyReactExternal], banner: { js: requireShim } },
  },
];

for (const { out, contents, extra = {} } of builds) {
  await esbuild.build({
    ...common,
    ...extra,
    stdin: { contents, resolveDir: workdir, loader: "js" },
    outfile: join(outdir, out),
  });
  const size = statSync(join(outdir, out)).size;
  console.log(`${out}\t${(size / 1024).toFixed(1)} KB`);
}

writeFileSync(
  join(outdir, "provenance.json"),
  JSON.stringify(
    {
      built_with: "src/scripts/build-app-importer-vendor.mjs",
      pins: PINS,
      note: "Browser-ESM vendor bundles for the Business OS App Importer. Rebuild with the pinned script only; never edit bundles by hand.",
    },
    null,
    2,
  ) + "\n",
);
console.log("provenance.json written");
