import test from "node:test";
import assert from "node:assert/strict";
import * as sucrase from "../vendor/app-importer/sucrase.mjs";
import {
  detectEntry,
  suggestedModuleId,
  transcodeFile,
  buildImportMap,
  transcodeApp,
  scaffoldModule,
} from "./app-transcode.mjs";

// A realistic Vite-style coding-agent app: TSX entry, component with hooks
// and types, a plain-TS util, CSS import, and one unsupported dependency
// in a variant fixture.
const FIXTURE = {
  "package.json": JSON.stringify({
    name: "@meridian/po-tracker",
    module: "src/main.tsx",
    dependencies: { react: "^19.0.0", "react-dom": "^19.0.0" },
  }),
  "src/main.tsx": `import { createRoot } from "react-dom/client";
import App from "./App";
import "./app.css";

createRoot(document.getElementById("root")!).render(<App />);
`,
  "src/App.tsx": `import { useState } from "react";
import { formatQty, type Item } from "./lib/format";

export default function App() {
  const [items, setItems] = useState<Item[]>([{ id: "a", name: "Pallet", qty: 4 }]);
  return (
    <ul>
      {items.map((item) => (
        <li key={item.id} onClick={() => setItems((prev) => prev.filter((p) => p.id !== item.id))}>
          {item.name}: {formatQty(item.qty)}
        </li>
      ))}
    </ul>
  );
}
`,
  "src/lib/format.ts": `export type Item = { id: string; name: string; qty: number };
export function formatQty(qty: number): string {
  return qty.toString().padStart(2, "0");
}
`,
  "src/app.css": `ul { margin: 0; }`,
};

test("detectEntry honors package.json module field", () => {
  assert.equal(detectEntry(FIXTURE), "src/main.tsx");
});

test("detectEntry falls back to conventional paths", () => {
  const files = { "src/index.tsx": "export {}" };
  assert.equal(detectEntry(files), "src/index.tsx");
  assert.equal(detectEntry({ "nope.txt": "" }), null);
});

test("suggestedModuleId slugs the package name", () => {
  assert.equal(suggestedModuleId(FIXTURE), "po-tracker");
  assert.equal(suggestedModuleId({}), "imported-app");
});

test("transcodeFile strips types, transforms JSX, rewrites relative imports", () => {
  const fileNames = new Set(Object.keys(FIXTURE));
  const { code, cssImports, bareImports } = transcodeFile(
    sucrase,
    "src/main.tsx",
    FIXTURE["src/main.tsx"],
    fileNames,
  );
  assert.match(code, /from "\.\/App\.js"/, "relative import gets explicit .js extension");
  assert.doesNotMatch(code, /\.tsx/, "no tsx specifiers survive");
  assert.match(code, /react\/jsx-runtime|createRoot/, "jsx runtime or entry code present");
  assert.deepEqual(cssImports, ["src/app.css"]);
  assert.ok(bareImports.includes("react-dom/client"));
});

test("transcodeFile resolves directory-relative imports across folders", () => {
  const fileNames = new Set(Object.keys(FIXTURE));
  const { code } = transcodeFile(sucrase, "src/App.tsx", FIXTURE["src/App.tsx"], fileNames);
  assert.match(code, /from "\.\/lib\/format\.js"/);
  assert.doesNotMatch(code, /type Item/, "types are stripped");
});

test("buildImportMap maps only vendored specifiers", () => {
  const map = buildImportMap(["react", "react-dom/client", "axios"], "../../vendor/app-importer");
  assert.equal(map.imports.react, "../../vendor/app-importer/react.mjs");
  assert.equal(map.imports["react-dom/client"], "../../vendor/app-importer/react-dom-client.mjs");
  assert.equal(map.imports.axios, undefined);
});

test("transcodeApp end to end: valid app passes with import map and css", () => {
  const result = transcodeApp(sucrase, FIXTURE);
  assert.equal(result.ok, true);
  assert.equal(result.entry, "src/main.js");
  assert.ok(result.files["src/main.js"].includes('from "./App.js"'));
  assert.ok(result.files["src/App.js"], "App.tsx transcoded to App.js");
  assert.ok(result.files["src/lib/format.js"], "format.ts transcoded");
  assert.equal(result.files["src/app.css"], FIXTURE["src/app.css"], "css passes through");
  assert.deepEqual(result.report.unsupported, []);
  assert.deepEqual(result.cssFiles, ["src/app.css"]);
  assert.ok(result.importMap.imports["react/jsx-runtime"], "automatic runtime is mapped");
});

test("transcodeApp reports unsupported bare dependencies instead of guessing", () => {
  const files = {
    ...FIXTURE,
    "src/App.tsx": FIXTURE["src/App.tsx"].replace(
      'import { useState } from "react";',
      'import axios from "axios";\nimport { useState } from "react";\nvoid axios.get("/health");',
    ),
  };
  const result = transcodeApp(sucrase, files);
  assert.equal(result.ok, false);
  assert.deepEqual(result.report.unsupported, ["axios"]);
});

test("transcodeApp fails honestly without an entry", () => {
  const result = transcodeApp(sucrase, { "readme.md": "hi" });
  assert.equal(result.ok, false);
  assert.equal(result.report.error, "entry_not_found");
});

test("scaffoldModule emits module.json, import map html and app files", () => {
  const transcoded = transcodeApp(sucrase, FIXTURE);
  const moduleFiles = scaffoldModule(
    { id: "po-tracker", title: "PO Tracker", description: "Imported test app" },
    transcoded,
  );
  const manifest = JSON.parse(moduleFiles["module.json"]);
  assert.equal(manifest.id, "po-tracker");
  assert.equal(manifest.entry, "local-modules/po-tracker/index.html");
  assert.equal(manifest.install_scope, "local");
  assert.match(moduleFiles["index.html"], /<script type="importmap">/);
  assert.match(moduleFiles["index.html"], /react-dom-client\.mjs/);
  assert.match(moduleFiles["index.html"], /<link rel="stylesheet" href="\.\/src\/app\.css">/);
  assert.match(moduleFiles["index.html"], /<script type="module" src="\.\/src\/main\.js">/);
  assert.ok(moduleFiles["src/App.js"]);
});

test("transcoded output is syntactically valid ESM", async () => {
  const { execFileSync } = await import("node:child_process");
  const { mkdtempSync, writeFileSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const result = transcodeApp(sucrase, FIXTURE);
  const dir = mkdtempSync(join(tmpdir(), "transcode-check-"));
  try {
    for (const [name, code] of Object.entries(result.files)) {
      if (!name.endsWith(".js")) continue;
      const file = join(dir, name.replaceAll("/", "__") + ".mjs");
      writeFileSync(file, code);
      execFileSync(process.execPath, ["--check", file], { stdio: "pipe" });
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
