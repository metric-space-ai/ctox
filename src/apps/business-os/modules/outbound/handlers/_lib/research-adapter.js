// ref: store_outbound_commands.rs:2776 outbound_handle_research_source_adapter
// (+ :3873 outbound_apply_research_adapter_credential_status,
//    :3041 outbound_apply_research_adapter_writeback).
// Native parts as capabilities:
//   ctx.cap.secrets.exists({scope:'credentials', name}) → {exists}
//   ctx.cap.browser.auth_assist.request({...}) → effect (enqueue_web_stack_auth_assist_request)
// The scrape-registry effect of generate_adapter is ported in
// _lib/scrape-generation.js; the test path (:3330-3758) belongs to outbound-lead-generation (OUT3: _lib/owner-app.js).
import { str, firstString, fail, load, objectPayload, mergeFields, putDefaultString, putDefaultObject, putDefaultI64, payloadInsert, isObject, asBool } from './outbound.js';

// ref: src/core/secrets.rs:1871 CREDENTIAL_SCOPE
const CREDENTIAL_SCOPE = 'credentials';
const MERGE_KEYS = ['tier', 'credential_secret_name', 'auth_mode', 'auth_status', 'scrape_status', 'last_run_id', 'last_success_at_ms', 'last_error'];

/** ref: store.rs:8090 runtime_app_starter_collection_name */
export function runtimeAppStarterCollectionName(moduleId) {
  let out = '';
  let lastUnderscore = false;
  for (const ch of String(moduleId).trim().toLowerCase()) {
    if (/^[a-z0-9]$/.test(ch)) { out += ch; lastUnderscore = false; } else if (!lastUnderscore) { out += '_'; lastUnderscore = true; }
  }
  const prefix = out.replace(/^_+|_+$/g, '');
  return prefix ? `${prefix}_records` : 'agent_app_records';
}

/** ref: store.rs:14161 is_safe_rxdb_collection_name */
export function isSafeCollectionName(collection) {
  return typeof collection === 'string' && collection.length > 0 && /^[a-z0-9_]+$/.test(collection);
}

async function credentialStatus(ctx, adapterPayload, record, nextStatus) {
  const requiresCredential = asBool(record.requires_credential) ?? asBool(adapterPayload.requires_credential) ?? false;
  if (!requiresCredential) {
    record.auth_status = 'not_required';
    return { required: false, scope: CREDENTIAL_SCOPE, secret_value_in_payload: false };
  }
  const secretName = firstString([str(record, ['credential_secret_name']), str(adapterPayload, ['credential_secret_name'])]);
  if (secretName == null) {
    record.auth_status = 'credential_name_missing';
    return { required: true, scope: CREDENTIAL_SCOPE, name: '', exists: false, secret_value_in_payload: false };
  }
  let exists = false;
  try {
    exists = Boolean((await ctx.cap.secrets.exists({ scope: CREDENTIAL_SCOPE, name: secretName }))?.exists);
  } catch {
    exists = false; // Rust: secret_exists(..).unwrap_or(false)
  }
  const status = exists ? 'credential_available' : nextStatus === 'auth_requested' ? 'auth_requested' : 'credential_missing';
  record.auth_status = status;
  return { required: true, scope: CREDENTIAL_SCOPE, name: secretName, exists, status, secret_value_in_payload: false };
}

function writebackProjection(command, adapterId, record, now) {
  const writeback = command.payload?.writeback;
  if (writeback === undefined) return null;
  const collection = typeof writeback?.collection === 'string' ? writeback.collection.trim() : '';
  if (!collection) fail('invalid', 'adapter writeback collection is required');
  if (!isSafeCollectionName(collection)) fail('invalid', 'invalid adapter writeback collection');
  const clientContext = command.client_context ?? {};
  const sourceModule = firstString([
    str(clientContext, ['source_module']), str(clientContext, ['module_id']), str(clientContext, ['app_id']),
  ]);
  if (sourceModule == null) fail('invalid', 'adapter writeback requires a source module');
  const modulePrefix = runtimeAppStarterCollectionName(sourceModule).replace(/(_records)+$/, '');
  if (!modulePrefix || !collection.startsWith(`${modulePrefix}_`)) {
    fail('forbidden', 'adapter writeback collection must be scoped to the source module');
  }
  const recordId = (typeof writeback.record_id === 'string' && writeback.record_id.trim()) || adapterId;
  if (recordId !== adapterId) fail('invalid', 'adapter writeback record_id must match adapter_id');
  const projection = JSON.parse(JSON.stringify(record));
  projection.id = recordId;
  projection.last_command_id = command.id ?? '';
  projection.updated_at_ms = now;
  return { collection, projection };
}

export async function handleResearchSourceAdapter(command, ctx, nextStatus, scrapeEffect = null) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const adapterPayload = payload.adapter !== undefined ? payload.adapter : payload;
  const recordIdRaw = command.record_id ?? payload.record_id;
  const adapterId = firstString([
    str(payload, ['adapter_id']),
    str(adapterPayload, ['id']),
    typeof recordIdRaw === 'string' ? recordIdRaw.trim() : null,
  ]);
  if (adapterId == null) fail('invalid', 'adapter_id is required');
  const sourceId = firstString([str(adapterPayload, ['source_id']), str(adapterPayload, ['id']), str(payload, ['source_id'])]);
  if (sourceId == null) fail('invalid', 'source_id is required');
  const url = firstString([str(adapterPayload, ['url']), str(payload, ['url'])]) ?? `https://${sourceId}/`;
  const clientContext = command.client_context ?? {};

  const record = (await load(ctx, 'outbound_research_adapters', adapterId)) ?? objectPayload(adapterPayload);
  record.id = adapterId;
  record.campaign_id = firstString([
    str(adapterPayload, ['campaign_id']), str(payload, ['campaign_id']), str(clientContext, ['campaign_id']),
  ]) ?? '';
  record.source_id = sourceId;
  record.label = firstString([str(adapterPayload, ['label']), str(payload, ['label']), sourceId]) ?? sourceId;
  record.url = url;
  record.adapter_kind = firstString([str(adapterPayload, ['adapter_kind']), str(payload, ['adapter_kind'])]) ?? 'custom_url';
  record.target_key = firstString([str(adapterPayload, ['target_key']), str(payload, ['target_key'])]) ?? '';
  record.status = nextStatus;
  if (isObject(record)) {
    record.enabled = asBool(adapterPayload?.enabled) ?? true;
    record.countries = adapterPayload?.countries !== undefined ? adapterPayload.countries : [];
    record.field_keys = adapterPayload?.field_keys !== undefined ? adapterPayload.field_keys : [];
    record.requires_credential = asBool(adapterPayload?.requires_credential) ?? false;
  }
  mergeFields(record, adapterPayload, MERGE_KEYS);
  putDefaultString(record, 'scrape_status', 'target_available');
  if (nextStatus === 'adapter_requested') record.scrape_status = 'registration_requested';
  else if (nextStatus === 'test_requested') record.scrape_status = 'test_requested';
  else if (nextStatus === 'auth_requested') record.auth_status = 'auth_requested';
  record.last_error = '';
  putDefaultObject(record, 'payload');
  payloadInsert(record, 'last_command_id', command.id ?? '');
  payloadInsert(record, 'last_command_type', command.type);
  if (payload.scrape_contract !== undefined) payloadInsert(record, 'scrape_contract', payload.scrape_contract);
  if (payload.target_manifest !== undefined) payloadInsert(record, 'target_manifest', payload.target_manifest);
  payloadInsert(record, 'secret_value_in_payload', false);
  if (scrapeEffect) {
    // ref: :2925 outbound_apply_research_adapter_scrape_effect (only generate_adapter here)
    const effect = await scrapeEffect({ command, ctx, adapterPayload: adapterPayload ?? {}, adapterId, sourceId, record });
    if (effect != null) payloadInsert(record, 'scrape_registry_effect', effect);
  }
  const credentialEffect = await credentialStatus(ctx, adapterPayload ?? {}, record, nextStatus);
  if (str(record, ['status']) === 'test_auth_required') record.auth_status = 'auth_required';
  payloadInsert(record, 'credential_ref', credentialEffect);
  putDefaultI64(record, 'created_at_ms', now);
  record.updated_at_ms = now;
  // Validate the writeback BEFORE any write (Rust errored after writing; the
  // v4 transaction makes the whole command atomic instead — see P10).
  const writebackPreview = writebackProjection(command, adapterId, record, now);
  await ctx.put('outbound_research_adapters', record);

  let authAssist = null;
  if (nextStatus === 'auth_requested') {
    const secretName = str(record, ['credential_secret_name']) ?? '';
    authAssist = await ctx.cap.browser.auth_assist.request({
      source_id: sourceId,
      target_url: url,
      credential_ref: secretName ? `ctox-secret://${CREDENTIAL_SCOPE}/${secretName}` : null,
      login_hint: null,
      preferred_auth_assist: payload.record_snapshot?.auth_assist ?? payload.auth_assist ?? null,
      requesting_task_id: (typeof command.id === 'string' && command.id.trim()) ? command.id : adapterId,
      source_module: command.app_id,
      command_path: command.type,
      // CAP-3b #844 caps/browser.rs AuthAssistRequest (deny_unknown_fields): owner = ctx.actor,
      // deterministic/trusted setzt der Kern; target_url ist serde-Alias von target_url_override.
    });
    record.auth_status = 'browser_session_requested';
    payloadInsert(record, 'auth_assist', authAssist);
    record.updated_at_ms = now;
    await ctx.put('outbound_research_adapters', record);
  }
  if (writebackPreview) {
    const { collection, projection } = writebackProjection(command, adapterId, record, now);
    await ctx.put(collection, projection);
  }
  return {
    ok: true,
    collection: 'outbound_research_adapters',
    adapter: record,
    adapter_id: adapterId,
    command_effect: nextStatus,
    auth_assist: authAssist,
    secret_value_in_payload: false,
  };
}
