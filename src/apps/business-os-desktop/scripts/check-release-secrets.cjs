"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const { spawnSync } = require("node:child_process");

const REQUIRED_RELEASE_SECRETS = [
  "APPLE_ID",
  "APPLE_ID_PASSWORD",
  "APPLE_TEAM_ID",
  "CTOX_BUSINESS_OS_DESKTOP_CSC_LINK",
  "CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD",
];

function main() {
  const options = parseArgs(process.argv.slice(2));
  const names = options.secretsJson
    ? readSecretNamesFromJson(options.secretsJson)
    : readSecretNamesFromGitHub(options.repo);
  const missing = missingRequiredSecrets(names);
  if (missing.length > 0) {
    throw new Error(`missing Business OS Desktop release secrets: ${missing.join(", ")}`);
  }
  console.log(`desktop release secrets OK: ${REQUIRED_RELEASE_SECRETS.join(", ")}`);
}

function parseArgs(args) {
  const options = {
    repo: process.env.GITHUB_REPOSITORY || "metric-space-ai/ctox",
    secretsJson: "",
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--repo") {
      options.repo = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--secrets-json") {
      options.secretsJson = String(args[index + 1] || "").trim();
      index += 1;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  assert.match(options.repo, /^[^/\s]+\/[^/\s]+$/, "--repo must be owner/name");
  return options;
}

function readSecretNamesFromGitHub(repo) {
  const result = spawnSync("gh", [
    "secret",
    "list",
    "--repo",
    repo,
    "--json",
    "name",
  ], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(`gh secret list failed: ${String(result.stderr || result.stdout || "").trim()}`);
  }
  return parseSecretNames(result.stdout);
}

function readSecretNamesFromJson(filePath) {
  return parseSecretNames(fs.readFileSync(filePath, "utf8"));
}

function parseSecretNames(rawJson) {
  const payload = JSON.parse(String(rawJson || "[]"));
  if (!Array.isArray(payload)) throw new Error("gh secret list JSON must be an array");
  return payload.map((entry) => {
    if (typeof entry === "string") return entry;
    return entry?.name;
  }).filter(Boolean).map((name) => String(name).trim());
}

function missingRequiredSecrets(names) {
  const present = new Set((names || []).map((name) => String(name).trim()));
  return REQUIRED_RELEASE_SECRETS.filter((name) => !present.has(name));
}

if (require.main === module) {
  main();
}

module.exports = {
  REQUIRED_RELEASE_SECRETS,
  missingRequiredSecrets,
  parseSecretNames,
};
