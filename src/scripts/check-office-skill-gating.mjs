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
const SURFACE_DOCS = [
  "src/skills/packs/content/doc/references/execution-surfaces.md",
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
  if (!seen.has(id) && !id.endsWith("-corpus")) {
    console.error(
      `feature group ${id} is not referenced by any execution-surfaces.md gating table`,
    );
    drift += 1;
  }
}

if (drift > 0) {
  console.error(
    `\n${drift} gating drift issue(s). Update the execution-surfaces.md tables to match features.json.`,
  );
  process.exit(1);
}
console.log(
  `office skill gating in sync (${seen.size} feature-group rows checked).`,
);
