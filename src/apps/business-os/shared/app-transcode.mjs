// App Importer transcode core: turns a coding-agent app (React/TypeScript,
// Vite-style layout) into a plain-ESM Business OS module — once, at import
// time. Runs in the browser (importer app) and in Node (deploy skill); the
// only dependency is the vendored sucrase bundle, injected by the caller so
// both runtimes can supply their copy.
//
// Decision record: docs/business-os-app-importer.md

// Bare specifiers we can satisfy at runtime through vendored ESM bundles.
// Everything else is reported, never silently dropped.
export const VENDORED_SPECIFIERS = Object.freeze({
  react: "react.mjs",
  "react/jsx-runtime": "react-jsx-runtime.mjs",
  "react-dom": "react-dom.mjs",
  "react-dom/client": "react-dom-client.mjs",
});

const SOURCE_EXTENSIONS = [".tsx", ".ts", ".jsx", ".js", ".mjs"];

// Build-time configuration is not runtime code: importing a Vite app must
// not drag vite/eslint/tailwind into the runtime-dependency report.
const BUILD_CONFIG_RE = /^(vite|vitest|eslint|postcss|tailwind|prettier|babel|jest|tsup|rollup|webpack)\.config\.[cm]?[jt]sx?$/;

export function isBuildConfigFile(name) {
  return BUILD_CONFIG_RE.test(name.split("/").pop() || "");
}
const ENTRY_CANDIDATES = [
  "src/main.tsx", "src/main.ts", "src/main.jsx", "src/main.js",
  "src/index.tsx", "src/index.ts", "src/index.jsx", "src/index.js",
  "main.tsx", "main.ts", "main.jsx", "main.js",
  "index.tsx", "index.ts", "index.jsx", "index.js",
];

// Matches static import/export-from and dynamic import specifiers. Good
// enough for transcoded output (sucrase normalizes exotic whitespace).
const SPECIFIER_RE = /(\bimport\s*(?:[\w${}\s,*]*?\s*from\s*)?|\bexport\s+[\w${}\s,*]*?\s+from\s*|\bimport\s*\(\s*)(["'])([^"']+)\2/g;

function normalizePath(path) {
  const parts = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}

function dirname(path) {
  const i = path.lastIndexOf("/");
  return i === -1 ? "" : path.slice(0, i);
}

function stripExtension(path) {
  const i = path.lastIndexOf(".");
  return i === -1 ? path : path.slice(0, i);
}

function outputName(sourceName) {
  const ext = sourceName.slice(sourceName.lastIndexOf("."));
  if ([".tsx", ".ts", ".jsx"].includes(ext)) return `${stripExtension(sourceName)}.js`;
  return sourceName;
}

function isSourceFile(name) {
  return SOURCE_EXTENSIONS.some((ext) => name.endsWith(ext)) && !name.endsWith(".d.ts");
}

// Resolve a relative import against the file set the way a bundler would:
// exact file, known source extensions, or directory index.
function resolveRelative(fromFile, spec, fileNames) {
  const base = normalizePath(`${dirname(fromFile)}/${spec}`);
  const candidates = [base];
  for (const ext of SOURCE_EXTENSIONS) candidates.push(`${base}${ext}`);
  for (const ext of SOURCE_EXTENSIONS) candidates.push(`${base}/index${ext}`);
  for (const candidate of candidates) {
    if (fileNames.has(candidate)) return candidate;
  }
  return null;
}

export function detectEntry(files) {
  const names = new Set(Object.keys(files));
  const pkgRaw = files["package.json"];
  if (pkgRaw) {
    try {
      const pkg = JSON.parse(pkgRaw);
      for (const key of ["module", "main"]) {
        const value = typeof pkg[key] === "string" ? normalizePath(pkg[key]) : "";
        if (value && names.has(value) && isSourceFile(value)) return value;
      }
    } catch {
      // fall through to convention scan
    }
  }
  for (const candidate of ENTRY_CANDIDATES) {
    if (names.has(candidate)) return candidate;
  }
  return null;
}

export function suggestedModuleId(files, fallback = "imported-app") {
  const pkgRaw = files["package.json"];
  if (pkgRaw) {
    try {
      const name = String(JSON.parse(pkgRaw).name || "");
      const slug = name.split("/").pop().toLowerCase().replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "");
      if (slug) return slug;
    } catch { /* ignore */ }
  }
  return fallback;
}

// Transcode one source file to plain ESM and rewrite its specifiers.
// Returns { code, cssImports, bareImports } — css imports are stripped
// (collected for index.html), bare imports are classified by the caller.
export function transcodeFile({ transform }, name, source, fileNames) {
  const ext = name.slice(name.lastIndexOf("."));
  const transforms = ext === ".tsx" ? ["typescript", "jsx"]
    : ext === ".ts" ? ["typescript"]
    : ext === ".jsx" ? ["jsx"]
    : [];
  const code = transforms.length
    ? transform(source, { transforms, jsxRuntime: "automatic", production: true }).code
    : source;

  const cssImports = [];
  const bareImports = new Set();
  const rewritten = code.replace(SPECIFIER_RE, (match, prefix, quote, spec) => {
    if (spec.startsWith(".") || spec.startsWith("/")) {
      if (spec.endsWith(".css")) {
        cssImports.push(normalizePath(`${dirname(name)}/${spec}`));
        return `/* css import handled by importer: ${spec} */`;
      }
      const resolved = resolveRelative(name, spec, fileNames);
      if (!resolved) return match; // unknown target: leave, validator will flag
      const target = outputName(resolved);
      let rel = target;
      const fromDir = dirname(name);
      if (fromDir && rel.startsWith(`${fromDir}/`)) rel = rel.slice(fromDir.length + 1);
      else if (fromDir) {
        const up = fromDir.split("/").map(() => "..").join("/");
        rel = `${up}/${rel}`;
      }
      if (!rel.startsWith(".")) rel = `./${rel}`;
      return `${prefix}${quote}${rel}${quote}`;
    }
    bareImports.add(spec);
    return match;
  });

  return { code: rewritten, cssImports, bareImports: [...bareImports] };
}

export function buildImportMap(bareImports, vendorBase) {
  const imports = {};
  for (const spec of bareImports) {
    const file = VENDORED_SPECIFIERS[spec];
    if (file) imports[spec] = `${vendorBase}/${file}`;
  }
  return { imports };
}

// Orchestrator: files is a plain object { "relative/path": "content" }.
// Returns transcoded module files plus an honest report; the caller decides
// whether unsupported dependencies block the import.
export function transcodeApp(sucrase, files, options = {}) {
  const vendorBase = options.vendorBase ?? "../../vendor/app-importer";
  const fileNames = new Set(Object.keys(files));
  const entry = options.entry ?? detectEntry(files);
  if (!entry) {
    return { ok: false, report: { error: "entry_not_found", checked: ENTRY_CANDIDATES } };
  }

  const out = {};
  const allCss = [];
  const allBare = new Set();
  for (const [name, content] of Object.entries(files)) {
    if (name === "package.json" || name.endsWith(".d.ts") || isBuildConfigFile(name)) continue;
    if (name.endsWith(".css")) { out[name] = content; continue; }
    if (!isSourceFile(name)) { out[name] = content; continue; }
    const result = transcodeFile(sucrase, name, content, fileNames);
    out[outputName(name)] = result.code;
    allCss.push(...result.cssImports);
    for (const spec of result.bareImports) allBare.add(spec);
  }

  const unsupported = [...allBare].filter((spec) => !VENDORED_SPECIFIERS[spec]);
  const importMap = buildImportMap(allBare, vendorBase);
  const cssFiles = [...new Set(allCss)].filter((name) => fileNames.has(name));

  return {
    ok: unsupported.length === 0,
    entry: outputName(entry),
    files: out,
    importMap,
    cssFiles,
    report: {
      sourceFiles: [...fileNames].filter(isSourceFile).length,
      bareImports: [...allBare],
      unsupported,
      cssFiles,
      entry: outputName(entry),
    },
  };
}

// Business OS scaffold around the transcoded app: an HTML-entry module
// (installed apps run from their own document; the shell frames it).
export function scaffoldModule({ id, title, description = "", version = "0.1.0" }, transcoded) {
  const importMapJson = JSON.stringify(transcoded.importMap, null, 2);
  const cssLinks = transcoded.cssFiles
    .map((href) => `    <link rel="stylesheet" href="./${href}">`)
    .join("\n");
  const indexHtml = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>${title}</title>
    <script type="importmap">
${importMapJson}
    </script>
${cssLinks}
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="./${transcoded.entry}"></script>
  </body>
</html>
`;
  const moduleJson = {
    id,
    title,
    description,
    entry: `local-modules/${id}/index.html`,
    icon: "icon.svg",
    install_scope: "local",
    version,
    category: "Imported",
    developer: "Imported via CTOX App Importer",
    provenance: {
      imported_at: new Date().toISOString(),
      transcoder: "sucrase",
      note: "Transcoded from a coding-agent app; original source kept under source/.",
    },
  };
  return {
    "module.json": `${JSON.stringify(moduleJson, null, 2)}\n`,
    "index.html": indexHtml,
    ...transcoded.files,
  };
}
