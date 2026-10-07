import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  canonicalJson,
  expectedArtifacts,
  generateArtifacts,
  provenanceHash,
} from "./generate-workjet-ui-contract-snapshot.mjs";

const JSON_PATH = new URL("../ui-contract/v1/workjet-ui-contract.json", import.meta.url);
const CSS_PATH = new URL("../ui-contract/v1/workjet-ui-contract.css", import.meta.url);

const cssName = (...parts) => parts.join("-").replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);

const parseHex = (hex) => {
  const value = hex.replace(/^#/, "");
  const expanded = value.length === 3 ? value.split("").map((channel) => `${channel}${channel}`).join("") : value;
  assert.match(expanded, /^[0-9a-f]{6}$/i, `invalid color token ${hex}`);
  return [0, 2, 4].map((offset) => Number.parseInt(expanded.slice(offset, offset + 2), 16));
};

const luminance = (hex) => {
  const channels = parseHex(hex).map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
};

const contrastRatio = (first, second) => {
  const one = luminance(first);
  const two = luminance(second);
  return (Math.max(one, two) + 0.05) / (Math.min(one, two) + 0.05);
};

const readableForeground = (accent) =>
  contrastRatio("#ffffff", accent) >= contrastRatio("#111827", accent) ? "#ffffff" : "#111827";

test("generated browser artifacts are in parity with the versioned source snapshot", async () => {
  const result = await generateArtifacts({ check: true });
  const sourcePayload = result.source.payload;
  const generated = JSON.parse(await readFile(JSON_PATH, "utf8"));

  assert.equal(generated.provenance.sourcePath, result.source.sourcePath);
  assert.equal(generated.provenance.sourceSha256, result.source.sourceSha256);
  assert.equal(generated.provenance.sourceSha256, provenanceHash(sourcePayload));
  const { provenance: _provenance, ...generatedPayload } = generated;
  assert.deepEqual(generatedPayload, sourcePayload);
  assert.equal(canonicalJson(sourcePayload), canonicalJson(generatedPayload));
  assert.match(result.css, new RegExp(`provenance-sha256: ${result.source.sourceSha256}`));
});

test("the snapshot preserves light/dark roles and emits each role into browser CSS", async () => {
  const result = await expectedArtifacts();
  const { payload } = result.source;

  assert.deepEqual(Object.keys(payload.themes).sort(), ["dark", "light"]);
  assert.match(result.css, /:root\[data-theme="dark"\]/);
  assert.match(result.css, /:root, :root\[data-theme="light"\]/);

  for (const [themeName, theme] of Object.entries(payload.themes)) {
    for (const [role, value] of Object.entries(theme.surfaces)) {
      const variable = role === "surface" ? "surface" : `surface-${role}`;
      assert.match(result.css, new RegExp(`--workjet-${variable}:\\s*${value}`), `${themeName} surface ${role}`);
    }
    for (const [role, value] of Object.entries(theme.text)) {
      assert.match(result.css, new RegExp(`--workjet-text-${cssName(role)}:\\s*${value}`), `${themeName} text ${role}`);
    }
    for (const [role, value] of Object.entries(theme.borders)) {
      assert.match(result.css, new RegExp(`--workjet-border-${role}:\\s*${value}`), `${themeName} border ${role}`);
    }
    for (const [role, value] of Object.entries(theme.accent)) {
      const variable = role === "value" ? "accent" : role === "focus" ? "focus" : `accent-${cssName(role)}`;
      assert.match(result.css, new RegExp(`--workjet-${variable}:\\s*${value}`), `${themeName} accent ${role}`);
    }
  }
});

test("every category has an explicit derived foreground and a matching CSS variable", async () => {
  const result = await expectedArtifacts();
  const { payload } = result.source;
  const categories = Object.entries(payload.categories);

  assert.deepEqual(categories.map(([category]) => category), [
    "Workspace",
    "Collaboration",
    "Productivity",
    "Development",
    "Engineering",
    "Knowledge",
    "Research",
    "Sales",
    "Recruiting",
    "Finance",
    "Operations",
    "Governance",
    "Security",
    "Analytics",
    "Entertainment",
    "System",
    "Imported",
  ]);
  assert.equal(new Set(categories.map(([category]) => category)).size, categories.length);

  for (const [category, token] of categories) {
    assert.equal(token.foreground, readableForeground(token.accent), `${category} foreground derivation`);
    assert.ok(
      contrastRatio(token.foreground, token.accent) >= payload.focus.minContrastRatio,
      `${category} contrast is below the contract minimum`,
    );
    const slug = category.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent:\\s*${token.accent}`));
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent-foreground:\\s*${token.foreground}`));
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent-soft:\\s*${token.softLight}`));
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent-border:\\s*${token.borderLight}`));
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent-soft:\\s*${token.softDark}`));
    assert.match(result.css, new RegExp(`--workjet-category-${slug}-accent-border:\\s*${token.borderDark}`));
  }

  assert.equal(payload.categories.Workspace.accent, "#2563eb");
  assert.equal(payload.categories.Engineering.accent, "#7c3aed");
  assert.equal(payload.categories.Security.accent, "#dc2626");
  assert.equal(payload.categories.Imported.accent, "#71717a");
  assert.deepEqual(payload.categories.Entertainment, {
    accent: "#db2777",
    foreground: "#ffffff",
    softLight: "#fce7f3",
    softDark: "#500724",
    borderLight: "#f9a8d4",
    borderDark: "#be185d",
  });
});

test("type rhythm, focus, elevation, and vocabulary remain contract data", async () => {
  const { payload } = (await expectedArtifacts()).source;

  assert.equal(payload.typography.scale.body.fontSize, 16);
  assert.equal(payload.typography.scale.body.lineHeight, 23);
  assert.equal(payload.spacing["4"], 16);
  assert.equal(payload.radii.control, 6);
  assert.equal(payload.focus.outlineWidth, 2);
  assert.equal(payload.focus.outlineOffset, 2);
  assert.equal(typeof payload.elevation.overlay, "string");
  assert.ok(payload.vocabulary.userTerms.includes("Settings"));
  assert.ok(payload.vocabulary.forbiddenTerms.includes("WebRTC"));
  assert.equal(
    payload.vocabulary.userTerms.some((term) => payload.vocabulary.forbiddenTerms.includes(term)),
    false,
  );
});
