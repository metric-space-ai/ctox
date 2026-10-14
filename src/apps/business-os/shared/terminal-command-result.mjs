// SPDX-License-Identifier: MIT OR AGPL-3.0-only
// A local receipt is an acknowledgement, never the result of a native read.
function inspect(command, commandId, accepts) {
  const doc = command?.toJSON?.() || command;
  if (!doc || typeof doc !== 'object') return null;
  const id = doc.command_id || doc.id;
  if (id && id !== commandId) throw new Error('Command result identity mismatch');
  const status = doc.terminal_status && doc.terminal_status !== 'none'
    ? doc.terminal_status : doc.status;
  if (['failed', 'cancelled'].includes(status)) {
    throw new Error(doc.error || doc.result?.error || `CTOX command ${status}`);
  }
  if (status !== 'completed') return null;
  if (doc.ok === false || doc.result?.ok === false || doc.result?.result?.ok === false) {
    throw new Error(doc.result?.error || doc.result?.result?.error || 'CTOX command failed');
  }
  const result = doc.result?.result || doc.result || doc;
  return accepts(result) ? result : null;
}

export async function readTerminalCommandResult(receipt, {
  commandId, commandBus, accepts, timeoutMs = 15_000,
} = {}) {
  if (!commandId || typeof accepts !== 'function') throw new Error('Command result contract missing');
  const direct = inspect(receipt, commandId, accepts);
  if (direct) return direct;
  // getStatus uses the same business_commands collection as the shell. Read it
  // first: terminal local data can exist even while the global readiness marker
  // is never-synced. Do not dispatch the command a second time.
  const deadline = Date.now() + timeoutMs;
  const remaining = () => Math.max(1, deadline - Date.now());
  let localError;
  if (typeof commandBus?.getStatus === 'function') {
    let local;
    try { local = await bounded(commandBus.getStatus(commandId), remaining()); }
    catch (error) { localError = error; }
    const result = inspect(local, commandId, accepts);
    if (result) return result;
  }
  if (Date.now() < deadline && typeof commandBus?.waitForTerminal === 'function') {
    const terminal = await bounded(commandBus.waitForTerminal(commandId, {
      timeoutMs: remaining(), sync_queue_tasks: false,
    }), remaining());
    const result = inspect(terminal, commandId, accepts);
    if (result) return result;
  }
  throw localError || new Error('CTOX has not supplied a terminal command result');
}

function bounded(value, timeoutMs) {
  let timer;
  return Promise.race([
    Promise.resolve(value),
    new Promise((_, reject) => {
      timer = setTimeout(() => {
        const error = new Error('CTOX command result timed out');
        error.code = 'command_result_timeout';
        reject(error);
      }, timeoutMs);
    }),
  ]).finally(() => clearTimeout(timer));
}
