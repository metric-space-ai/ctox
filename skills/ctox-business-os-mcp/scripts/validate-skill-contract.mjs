// Origin: CTOX
// License: AGPL-3.0-only

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const skillPath = path.resolve(scriptDir, "../SKILL.md");
const repoRoot = path.resolve(scriptDir, "../../..");
const rustPath = path.join(repoRoot, "src/core/business_os/mcp_channel.rs");
const planPath = path.join(repoRoot, "docs/business-os-mcp-channel-v1-implementation-plan.md");

if (import.meta.url === `file://${process.argv[1]}`) {
  const result = validateSkillContract({
    skillText: fs.readFileSync(skillPath, "utf8"),
    rustText: fs.readFileSync(rustPath, "utf8"),
    planText: fs.readFileSync(planPath, "utf8")
  });
  if (!result.ok) {
    for (const error of result.errors) {
      console.error(`fail ${error}`);
    }
    process.exitCode = 1;
  } else {
    console.log(`ok Business OS MCP skill contract covers ${result.toolCount} tools`);
  }
}

export function validateSkillContract({ skillText, rustText, planText }) {
  const descriptorTools = extractRustDescriptorTools(rustText);
  const skillTools = extractSkillTools(skillText);
  const planTools = extractPlanTools(planText);
  const errors = [];

  compareToolSets("skill", skillTools, "Rust descriptor", descriptorTools, errors);
  compareToolSets("plan", planTools, "Rust descriptor", descriptorTools, errors);

  for (const forbidden of [
    "run_cli",
    "run_shell",
    "write_sql",
    "push_rxdb_record",
    "remote_control_browser",
    "execute_raw_business_command"
  ]) {
    if (!skillText.includes(forbidden)) {
      errors.push(`skill must explicitly forbid ${forbidden}`);
    }
  }

  if (!skillText.includes("error.data.code")) {
    errors.push("skill must instruct agents to use stable JSON-RPC error.data.code");
  }
  if (!skillText.includes("response_too_large")) {
    errors.push("skill must document response_too_large handling");
  }
  for (const phrase of [
    "Business OS access is two-layered",
    "apps.view",
    "data.read",
    "data.write",
    "apps.modify",
    "external.approve",
    "0.x.y",
    "1.0.0",
    "business_os_policy"
  ]) {
    if (!skillText.includes(phrase)) {
      errors.push(`skill must document remote role/app/data scope phrase: ${phrase}`);
    }
  }
  for (const phrase of [
    "/api/business-os/mcp/connect-info",
    "managed MCP client token",
    "Web-Login Bootstrap",
    "connect-business-os-mcp.mjs --password-stdin",
    "/api/desktop/session-package",
    "/api/instances/<tenant-id>/managed-mcp",
    "https://ctox.dev/dashboard?tenant=<tenant-id>#mcp",
    "Token rotieren",
    "Neuer Token",
    "business_os.create_app",
    "business_os.modify_app",
    "development_contract",
    "runtime/business-os/installed-modules/<module_id>",
    "business-os-app-module-development",
    "ctox business-os app validate <module_id> --installed",
    "business_os.get_command_status"
  ]) {
    if (!skillText.includes(phrase)) {
      errors.push(`skill must document MCP app development phrase: ${phrase}`);
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    toolCount: descriptorTools.size
  };
}

export function extractRustDescriptorTools(text) {
  const block = text.match(/pub fn tool_descriptors\(\)[\s\S]*?pub fn mcp_status/);
  if (!block) {
    return new Set();
  }
  return extractBusinessOsTools(block[0]);
}

export function extractSkillTools(text) {
  const section = text.match(/## Expected Tool Classes[\s\S]*?## Runtime Policy/);
  if (!section) {
    return new Set();
  }
  return extractBusinessOsTools(section[0]);
}

export function extractPlanTools(text) {
  const table = text.match(/### Generic Tools[\s\S]*?### Explicit Non-Tools/);
  if (!table) {
    return new Set();
  }
  return extractBusinessOsTools(table[0]);
}

function extractBusinessOsTools(text) {
  return new Set([...text.matchAll(/\bbusiness_os\.[a-z_]+\b/g)].map((match) => match[0]));
}

function compareToolSets(leftName, left, rightName, right, errors) {
  for (const tool of right) {
    if (!left.has(tool)) {
      errors.push(`${leftName} is missing ${tool} from ${rightName}`);
    }
  }
  for (const tool of left) {
    if (!right.has(tool)) {
      errors.push(`${leftName} documents unknown tool ${tool}`);
    }
  }
}
