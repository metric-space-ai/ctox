"use strict";

// Append-only local operation journal. The native runner supplies stateRoot
// and operationId; research input must never select either. Credentials and
// provider bodies are not accepted by this storage contract.
const fs = require("node:fs");
const path = require("node:path");
const { createHash, randomUUID } = require("node:crypto");
const { collectionBinding } = require("./brightdata-core.cjs");
const MAX_REVISIONS = 64;
const MAX_BYTES = 16384;
const hash = value => createHash("sha256").update(value).digest("hex");

function checkedDirectory(directory) {
  if (process.platform === "win32") throw new Error("checkpoint_requires_posix_filesystem");
  if (!path.isAbsolute(directory)) throw new Error("checkpoint_root_not_absolute");
  const resolved = path.resolve(directory);
  let current = path.parse(resolved).root;
  for (const part of resolved.slice(current.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    const stat = fs.lstatSync(current);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error("unsafe_checkpoint_directory");
  }
  if (process.platform !== "win32" && (fs.statSync(resolved).mode & 0o022))
    throw new Error("checkpoint_directory_writable_by_others");
  return resolved;
}

function syncDirectory(directory) {
  const fd = fs.openSync(directory, fs.constants.O_RDONLY);
  try { fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
}

function readBounded(file) {
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > MAX_BYTES || (stat.mode & 0o077)) throw new Error("invalid_checkpoint_file");
    const data = Buffer.alloc(MAX_BYTES + 1);
    let length = 0;
    for (;;) {
      const read = fs.readSync(fd, data, length, data.length - length, null);
      length += read;
      if (length > MAX_BYTES) throw new Error("checkpoint_too_large");
      if (!read) return data.subarray(0, length).toString("utf8");
    }
  } finally { fs.closeSync(fd); }
}

function canonicalState(value, binding) {
  if (!value || value.query_hash !== binding.query_hash ||
      !["submitting", "rejected", "pending", "ready", "completed"].includes(value.phase))
    throw new Error("checkpoint_state_invalid");
  // Strip all non-contract fields, including accidental credentials/errors.
  const attempt = value.submission_attempt ?? 1;
  if (!Number.isInteger(attempt) || attempt < 1 || attempt > 2) throw new Error("checkpoint_attempt_invalid");
  const state = { phase: value.phase, query_hash: binding.query_hash, binding, submission_attempt: attempt };
  if (value.phase === "rejected") {
    if (value.error_code !== "api_unauthorized") throw new Error("checkpoint_rejection_invalid");
    state.error_code = "api_unauthorized";
  } else if (value.phase !== "submitting") {
    if (!/^(?:sd|s)_[A-Za-z0-9]{1,100}$/.test(value.snapshot_id || ""))
      throw new Error("checkpoint_snapshot_invalid");
    state.snapshot_id = value.snapshot_id;
  }
  if (value.binding && JSON.stringify(collectionBinding(value.binding,
      value.binding.company_profile_url, value.binding.urls)) !== JSON.stringify(binding))
    throw new Error("checkpoint_binding_mismatch");
  return state;
}

function validateTransition(previous, next) {
  const allowed = { submitting: ["pending", "rejected"], rejected: ["submitting"], pending: ["pending", "ready"], ready: ["completed"], completed: ["completed"] };
  if (!previous ? next.phase !== "submitting" : !allowed[previous.phase].includes(next.phase))
    throw new Error("checkpoint_transition_invalid");
  const expectedAttempt = !previous ? 1 : previous.submission_attempt + (previous.phase === "rejected" ? 1 : 0);
  if (next.submission_attempt !== expectedAttempt) throw new Error("checkpoint_attempt_transition_invalid");
  if (previous?.snapshot_id && previous.snapshot_id !== next.snapshot_id)
    throw new Error("checkpoint_snapshot_changed");
}

function openCheckpoint({ stateRoot, operationId, binding: rawBinding }) {
  const root = checkedDirectory(stateRoot);
  if (typeof operationId !== "string" || !operationId.trim() || operationId.length > 200 ||
      /[\x00-\x1f\x7f]/.test(operationId)) throw new Error("checkpoint_operation_invalid");
  const binding = collectionBinding(rawBinding, rawBinding.company_profile_url, rawBinding.urls);
  if (binding.query_hash !== rawBinding.query_hash) throw new Error("checkpoint_binding_mismatch");
  const operationHash = hash(operationId);
  // Deliberately not keyed by query: changing a query within the SAME durable
  // operation must conflict rather than create a second provider submission.
  const directory = path.join(root, operationHash);
  try { fs.mkdirSync(directory, { mode: 0o700 }); syncDirectory(root); }
  catch (error) { if (error.code !== "EEXIST") throw error; }
  checkedDirectory(directory);
  let observed = null;

  function load() {
    checkedDirectory(directory);
    const entries = fs.readdirSync(directory);
    if (entries.length > 256) throw new Error("checkpoint_directory_limit");
    const names = entries.filter(name => /^revision-\d{3}\.json$/.test(name)).sort();
    if (names.length > MAX_REVISIONS) throw new Error("checkpoint_revision_limit");
    let previous = null;
    for (let index = 0; index < names.length; index++) {
      const revision = index + 1;
      if (names[index] !== `revision-${String(revision).padStart(3, "0")}.json`)
        throw new Error("checkpoint_revision_gap");
      const raw = readBounded(path.join(directory, names[index]));
      const entry = JSON.parse(raw);
      if (entry.schema !== "ctox.brightdata.checkpoint.v1" || entry.operation_hash !== operationHash ||
          entry.revision !== revision || entry.previous_sha256 !== (previous?.sha256 || null))
        throw new Error("checkpoint_chain_invalid");
      const state = canonicalState(entry.state, binding);
      if (JSON.stringify(state) !== JSON.stringify(entry.state)) throw new Error("checkpoint_noncanonical_state");
      validateTransition(previous?.state, state);
      previous = { revision, state, sha256: hash(raw) };
    }
    observed = previous;
    return previous ? structuredClone(previous.state) : null;
  }

  function append(value, claim) {
    checkedDirectory(directory);
    const state = canonicalState(value, binding);
    const previous = observed;
    if (claim && previous && previous.state.phase !== "rejected") return false;
    validateTransition(previous?.state, state);
    const revision = (previous?.revision || 0) + 1;
    if (revision > MAX_REVISIONS) throw new Error("checkpoint_revision_limit");
    const entry = { schema: "ctox.brightdata.checkpoint.v1", operation_hash: operationHash,
      revision, previous_sha256: previous?.sha256 || null, state };
    const raw = JSON.stringify(entry);
    if (Buffer.byteLength(raw) > MAX_BYTES) throw new Error("checkpoint_too_large");
    const temporary = path.join(directory, `pending-${randomUUID()}.tmp`);
    const destination = path.join(directory, `revision-${String(revision).padStart(3, "0")}.json`);
    let fd, created = false;
    try {
      fd = fs.openSync(temporary, "wx", 0o600);
      created = true;
      fs.writeFileSync(fd, raw); fs.fsyncSync(fd); fs.closeSync(fd); fd = undefined;
      // Atomic no-replace publication of a completely written, synced inode.
      // Two processes claiming the same revision cannot both succeed.
      fs.linkSync(temporary, destination);
      syncDirectory(directory);
      observed = { revision, state, sha256: hash(raw) };
      return true;
    } catch (error) {
      if (error.code === "EEXIST") {
        if (claim) return false;
        throw new Error("checkpoint_write_conflict");
      }
      throw error;
    } finally {
      if (fd !== undefined) fs.closeSync(fd);
      // Only remove the unique scratch name created by this exact write.
      if (created) {
        try { fs.unlinkSync(temporary); } catch (error) { if (error.code !== "ENOENT") throw error; }
      }
    }
  }

  load();
  return { load, claimSubmission: state => append(state, true),
    saveState: state => { append(state, false); } };
}

module.exports = { openCheckpoint };
