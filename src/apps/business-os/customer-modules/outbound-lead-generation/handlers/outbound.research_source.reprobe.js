// outbound.research_source.reprobe — Validierung + Wiederprüfung über den App-Läufer (process.run)
// ref: store_outbound_commands.rs:4090-4121 outbound_handle_source_reprobe
//      :4054-4066 outbound_scrape_operation_timeout_ms (ported)
//      :4123-4139 outbound_source_test_probe_grant → command_rights (Core prüft vor dem Handler, §16.20)
//      capabilities/scrape/execute.rs:377-440 reprobe_registered_account_keep_script (ported, Strang OLGSCR)
//
// Reprobes one existing provider account with the registered script unchanged;
// no registration, no script replacement.
import { asU64, fail, has, isObject, requiredString } from './_lib/olg.js';
import { executeScrape } from './_lib/scrape-runner.js';
import { targetScriptRegistration } from './_lib/scrape-registry.js';

const TIMEOUT_MESSAGE = 'test_input.operation_timeout_ms must be an integer between 1000 and 300000';

/** ref: :4054 — absent → 90 000; otherwise u64 in 1 000..=300 000. */
export function operationTimeoutMs(payload) {
  if (!has(payload, 'operation_timeout_ms')) return 90_000;
  const timeout = asU64(payload.operation_timeout_ms);
  if (timeout == null || timeout < 1_000 || timeout > 300_000) fail('invalid', TIMEOUT_MESSAGE);
  return timeout;
}

export default async function handle(command, ctx) {
  const payload = command.payload;
  if (!(isObject(payload) && payload.keep_script === true)) fail('invalid', 'source reprobe requires keep_script=true');
  const targetKey = requiredString(payload, ['target_key']);
  if (!(targetKey.length <= 128 && /^[A-Za-z0-9._-]+$/.test(targetKey))) fail('invalid', 'invalid source reprobe target_key');
  const input = payload.probe_input;
  if (!isObject(input)) fail('invalid', 'source reprobe requires probe_input with real provider lookup data');
  const timeoutMs = operationTimeoutMs(payload);
  return reprobeKeepScript(ctx, targetKey, input, Math.ceil(timeoutMs / 1000), command.id);
}

/** ref: execute.rs:377-440 — no register/upsert/override; the script must stay unchanged. */
export async function reprobeKeepScript(ctx, targetKey, input, timeoutSeconds, commandId) {
  if (!isObject(input)) fail('invalid', 'account reprobe input must be an object');
  if (!(timeoutSeconds >= 1 && timeoutSeconds <= 300)) fail('invalid', 'invalid account reprobe timeout');
  const before = await targetScriptRegistration(ctx, targetKey);
  if (before == null) fail('not_found', 'account reprobe target is not registered');
  if (before.script_registered !== true) fail('invalid', 'account reprobe requires an existing unchanged registered script');
  const outcome = await executeScrape(ctx, {
    target_key: targetKey, trigger_kind: 'account_reprobe', timeout_seconds: timeoutSeconds, input, command_id: commandId, probe_account: true,
  });
  const after = await targetScriptRegistration(ctx, targetKey);
  if (after == null) fail('not_found', 'account reprobe target disappeared');
  if (!(before.revision_no === after.revision_no && before.script_sha256 === after.script_sha256 && after.script_registered === true)) {
    fail('conflict', 'account reprobe detected a changed registered script');
  }
  return {
    ok: outcome.status === 'succeeded',
    target_key: targetKey,
    keep_script: true,
    script_revision: after.revision_no,
    script_sha256_before: before.script_sha256,
    script_sha256_after: after.script_sha256,
    outcome,
  };
}
