import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  extractPlanTools,
  extractRustDescriptorTools,
  extractSkillTools,
  validateSkillContract
} from "../scripts/validate-skill-contract.mjs";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = resolveRepoRoot(testDir);

function resolveRepoRoot(startDir) {
  for (const candidate of [
    process.env.CTOX_REPO_ROOT,
    path.resolve(startDir, "../../.."),
    process.cwd()
  ].filter(Boolean)) {
    const resolved = findRepoRoot(path.resolve(candidate));
    if (resolved) {
      return resolved;
    }
  }
  return path.resolve(startDir, "../../..");
}

function findRepoRoot(candidate) {
  let current = candidate;
  while (true) {
    if (
      fs.existsSync(path.join(current, "src/core/business_os/mcp_channel.rs"))
      && fs.existsSync(path.join(current, "docs/business-os-mcp-channel-v1-implementation-plan.md"))
    ) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

test("skill contract covers exactly the Rust MCP tool descriptors", () => {
  const skillText = fs.readFileSync(
    path.join(repoRoot, "skills/ctox-business-os-mcp/SKILL.md"),
    "utf8"
  );
  const rustText = fs.readFileSync(path.join(repoRoot, "src/core/business_os/mcp_channel.rs"), "utf8");
  const planText = fs.readFileSync(
    path.join(repoRoot, "docs/business-os-mcp-channel-v1-implementation-plan.md"),
    "utf8"
  );
  const result = validateSkillContract({ skillText, rustText, planText });

  assert.deepEqual(result, {
    ok: true,
    errors: [],
    toolCount: 32
  });
});

test("tool extraction reads Rust descriptors, skill, and plan", () => {
  const rustTools = extractRustDescriptorTools(
    'pub fn tool_descriptors() -> Vec<T> { read_tool("business_os.status", "", v) }\n\npub fn mcp_status'
  );
  const skillTools = extractSkillTools(
    "## Expected Tool Classes\n```text\nbusiness_os.status\n```\n## Runtime Policy"
  );
  const planTools = extractPlanTools(
    "### Generic Tools\n| Tool | Klasse | Zweck |\n| --- | --- | --- |\n| `business_os.status` | read | x |\n\n### Explicit Non-Tools"
  );

  assert.deepEqual([...rustTools], ["business_os.status"]);
  assert.deepEqual([...skillTools], ["business_os.status"]);
  assert.deepEqual([...planTools], ["business_os.status"]);
});
