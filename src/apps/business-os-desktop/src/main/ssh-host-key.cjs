"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { execFile } = require("node:child_process");
const { promisify } = require("node:util");

const execFileAsync = promisify(execFile);

const HOST_KEY_TYPES = Object.freeze([
  "ssh-ed25519",
  "ecdsa-sha2-nistp256",
  "ecdsa-sha2-nistp384",
  "ecdsa-sha2-nistp521",
  "rsa-sha2-512",
  "rsa-sha2-256",
  "ssh-rsa",
]);

const HOST_KEY_PREFERENCE = new Map(HOST_KEY_TYPES.map((keyType, index) => [keyType, index]));

function normalizeHostKeyTarget(options = {}) {
  const host = String(options.host || "").trim();
  const port = Number(options.port || 22);
  if (!host) throw new Error("ssh host is required");
  if (host.startsWith("-") || /\s/.test(host) || /["'`;|&$<>\\]/.test(host)) {
    throw new Error("ssh host contains unsupported characters");
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error("ssh port must be between 1 and 65535");
  }
  return { host, port };
}

async function inspectSshHostKey(options = {}, deps = {}) {
  const target = normalizeHostKeyTarget(options);
  const runKeyscan = deps.runKeyscan || runSshKeyscan;
  const output = await runKeyscan(target);
  const keys = parseSshKeyscanOutput(output, target);
  const selected = selectPreferredHostKey(keys);
  if (!selected) {
    throw new Error("ssh host key scan returned no supported host keys");
  }
  return {
    host: target.host,
    port: target.port,
    keyType: selected.keyType,
    algorithm: "SHA256",
    fingerprint: selected.fingerprint,
    knownHostsLine: canonicalKnownHostLine(target, selected),
    scannedAt: new Date().toISOString(),
  };
}

async function runSshKeyscan(target) {
  const { stdout } = await execFileAsync("ssh-keyscan", buildSshKeyscanArgs(target), {
    timeout: 15000,
    windowsHide: true,
  });
  return stdout;
}

function buildSshKeyscanArgs(options = {}) {
  const target = normalizeHostKeyTarget(options);
  return [
    "-p",
    String(target.port),
    "-T",
    "10",
    "-t",
    "ed25519,ecdsa,rsa",
    target.host,
  ];
}

function parseSshKeyscanOutput(output, target = {}) {
  const normalizedTarget = target.host ? normalizeHostKeyTarget(target) : null;
  return String(output || "")
    .split(/\r?\n/)
    .map((line) => parseKnownHostLine(line, normalizedTarget))
    .filter(Boolean);
}

function parseKnownHostLine(line, target) {
  const trimmed = String(line || "").trim();
  if (!trimmed || trimmed.startsWith("#")) return null;
  const parts = trimmed.split(/\s+/);
  if (parts.length < 3) return null;
  const [hosts, keyType, keyData] = parts;
  if (!HOST_KEY_PREFERENCE.has(keyType)) return null;
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(keyData)) return null;
  const keyBuffer = Buffer.from(keyData, "base64");
  if (keyBuffer.length === 0) return null;
  return {
    hosts,
    host: target?.host || hosts.split(",")[0],
    port: target?.port || 22,
    keyType,
    keyData,
    fingerprint: `SHA256:${crypto.createHash("sha256").update(keyBuffer).digest("base64").replace(/=+$/, "")}`,
    rawLine: trimmed,
  };
}

function selectPreferredHostKey(keys) {
  return [...(keys || [])].sort((left, right) => {
    const leftRank = HOST_KEY_PREFERENCE.get(left.keyType) ?? Number.MAX_SAFE_INTEGER;
    const rightRank = HOST_KEY_PREFERENCE.get(right.keyType) ?? Number.MAX_SAFE_INTEGER;
    return leftRank - rightRank;
  })[0] || null;
}

function verifyTrustedHostKey(inspectedHostKey, trustedFingerprint) {
  const inspected = normalizeFingerprint(inspectedHostKey?.fingerprint);
  const trusted = normalizeFingerprint(trustedFingerprint);
  if (!inspected) throw new Error("ssh host key fingerprint is missing");
  if (!trusted) throw new Error("ssh host key fingerprint confirmation is required");
  if (inspected !== trusted) {
    throw new Error(`ssh host key fingerprint mismatch: expected ${trusted}, got ${inspected}`);
  }
  return true;
}

function normalizeFingerprint(value) {
  const fingerprint = String(value || "").trim();
  if (!fingerprint) return "";
  return fingerprint.startsWith("SHA256:") ? fingerprint : `SHA256:${fingerprint}`;
}

function ensureKnownHost({ knownHostsPath, host, port, knownHostsLine }) {
  if (!knownHostsPath) return false;
  const target = normalizeHostKeyTarget({ host, port });
  const parsed = parseKnownHostLine(knownHostsLine, target);
  if (!parsed) throw new Error("known host line is invalid");
  const canonical = canonicalKnownHostLine(target, parsed);
  const hostPattern = knownHostPattern(target);
  fs.mkdirSync(path.dirname(knownHostsPath), { recursive: true });
  const existing = fs.existsSync(knownHostsPath) ? fs.readFileSync(knownHostsPath, "utf8") : "";
  const retained = existing
    .split(/\r?\n/)
    .filter((line) => line.trim())
    .filter((line) => !knownHostLineMatches(line, hostPattern));
  retained.push(canonical);
  fs.writeFileSync(knownHostsPath, `${retained.join("\n")}\n`, { mode: 0o600 });
  fs.chmodSync(knownHostsPath, 0o600);
  return true;
}

function canonicalKnownHostLine(target, key) {
  return `${knownHostPattern(target)} ${key.keyType} ${key.keyData}`;
}

function knownHostLineMatches(line, hostPattern) {
  const hosts = String(line || "").trim().split(/\s+/)[0] || "";
  return hosts.split(",").includes(hostPattern);
}

function knownHostPattern(options = {}) {
  const target = normalizeHostKeyTarget(options);
  if (target.port === 22) return target.host;
  return `[${target.host.replace(/^\[|\]$/g, "")}]:${target.port}`;
}

module.exports = {
  normalizeHostKeyTarget,
  inspectSshHostKey,
  buildSshKeyscanArgs,
  parseSshKeyscanOutput,
  selectPreferredHostKey,
  verifyTrustedHostKey,
  ensureKnownHost,
  knownHostPattern,
};
