#!/usr/bin/env node
// Origin: CTOX
// License: AGPL-3.0-only
//
// Guard: the office skill gating tables must not drift from the engine's
// feature matrix. Compares the status column of every feature-group row in
// the skills' execution-surfaces.md files against
// src/apps/business-os/office-engine/features.json and exits non-zero on
// mismatch. Run after every features.json status change:
//
//   node src/scripts/check-office-skill-gating.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

const FEATURES_PATH = join(
  repoRoot,
  "src/apps/business-os/office-engine/features.json",
);
const CLI_PATH = join(repoRoot, "src/core/office-engine/src/main.rs");
const DOCUMENT_SURFACE_PATH =
  "src/skills/packs/content/doc/references/execution-surfaces.md";
const SURFACE_DOCS = [
  DOCUMENT_SURFACE_PATH,
  "src/skills/packs/content/spreadsheet/references/execution-surfaces.md",
];

const features = JSON.parse(readFileSync(FEATURES_PATH, "utf8"));
const statusById = new Map();
for (const editor of Object.values(features.editors)) {
  for (const feature of editor.features) {
    statusById.set(feature.id, feature.status);
  }
}

let drift = 0;
const seen = new Set();

for (const relPath of SURFACE_DOCS) {
  const text = readFileSync(join(repoRoot, relPath), "utf8");
  for (const line of text.split("\n")) {
    // Table rows referencing a feature group look like:
    // | <operation> | `document.tables` | differential_passed |
    const match = line.match(
      /^\|[^|]*\|\s*`((?:document|spreadsheet)\.[a-z0-9-]+)`\s*\|\s*([^|]+)\|/,
    );
    if (!match) continue;
    const [, id, rawStatus] = match;
    seen.add(id);
    const documented = rawStatus.replace(/\*/g, "").trim().split(/[\s(]/)[0];
    const actual = statusById.get(id);
    if (!actual) {
      console.error(`${relPath}: references unknown feature group ${id}`);
      drift += 1;
      continue;
    }
    if (documented !== actual) {
      console.error(
        `${relPath}: ${id} documented as "${documented}" but features.json says "${actual}"`,
      );
      drift += 1;
    }
  }
}

for (const id of statusById.keys()) {
  if (!seen.has(id)) {
    console.error(
      `feature group ${id} is not referenced by any execution-surfaces.md gating table`,
    );
    drift += 1;
  }
}

// Keep the Documents skill's native operation catalog in exact lock-step with
// the ctox-office-engine CLI. The document table is the complete native-op
// reference; the spreadsheet table intentionally lists only kind-agnostic
// package operations because the OOXML batch transforms are DOCX-specific.
const cliSource = readFileSync(CLI_PATH, "utf8");
const cliOperations = new Set(
  [...cliSource.matchAll(/^\s*"([a-z][a-z0-9-]+)"\s*=>\s*\{/gm)].map(
    (match) => match[1],
  ),
);

const documentSurface = readFileSync(
  join(repoRoot, DOCUMENT_SURFACE_PATH),
  "utf8",
);
const nativeSection = documentSurface
  .split("## Native batch operations", 2)[1]
  ?.split("## Known coverage gaps", 1)[0];
if (!nativeSection) {
  console.error(`${DOCUMENT_SURFACE_PATH}: native operation table is missing`);
  drift += 1;
} else {
  const documentedOperations = new Set();
  for (const line of nativeSection.split("\n")) {
    const match = line.match(/^\|[^|]*\|\s*`([^`]+)`\s*\|/);
    if (!match) continue;
    const parts = match[1].split(/\\?\|/);
    const prefixMatch = parts[0].match(/^(.*-)[^-]+$/);
    for (const [index, part] of parts.entries()) {
      documentedOperations.add(
        index > 0 && prefixMatch && !part.includes("-")
          ? `${prefixMatch[1]}${part}`
          : part,
      );
    }
  }

  for (const operation of cliOperations) {
    if (!documentedOperations.has(operation)) {
      console.error(
        `${DOCUMENT_SURFACE_PATH}: CLI operation ${operation} is undocumented`,
      );
      drift += 1;
    }
  }
  for (const operation of documentedOperations) {
    if (!cliOperations.has(operation)) {
      console.error(
        `${DOCUMENT_SURFACE_PATH}: documents unavailable CLI operation ${operation}`,
      );
      drift += 1;
    }
  }
}

if (drift > 0) {
  console.error(
    `\n${drift} gating drift issue(s). Update the execution-surfaces.md tables to match features.json.`,
  );
  process.exit(1);
}
console.log(
  `office skill gating in sync (${seen.size} feature-group rows and ${cliOperations.size} native operations checked).`,
);
