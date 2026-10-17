// news.source.auth_assist – Port aus store_news_commands.rs handle_source_auth_assist :252. GEMISCHT.
// Fachlogik hier: source_id-Pruefung, nur X/Reddit, Owner-Pflicht, Rueckschrieb
// status=auth_requested + payload.auth_assist an die Quelle.
// Faehigkeit: Login-Hilfe im CTOX-Browserprofil -> ctx.cap.browser.auth_assist.request.

import {
  NEWS_MODULE,
  NEWS_SOURCE_AUTH_ASSIST,
  accountAuthSource,
  fail,
  firstStringField,
  patchRecord,
  recordIdOf,
  sessionUserId,
  validRecordId,
} from './_lib/news.js';

export default async function handle(command, ctx) {
  if (command.app_id !== NEWS_MODULE) fail('forbidden', 'news control commands must originate from the news module');
  const payload = command.payload ?? {};
  const sourceId = firstStringField(payload, ['source_id']) ?? recordIdOf(command) ?? '';
  if (!validRecordId(sourceId)) fail('invalid', 'news.source.auth_assist requires a valid source_id');
  const source = await ctx.get('news_sources', sourceId);
  if (!source) fail('not_found', 'news source not found');
  const authSource = accountAuthSource(typeof source.kind === 'string' ? source.kind : undefined);
  if (!authSource) fail('invalid', 'only X and Reddit sources sign in through the browser');
  // Owner-Pflicht bleibt App-Prüfung; der Sitzungseigentümer selbst ist im Kern der Core-gebundene Akteur (CAP-3b).
  sessionUserId(command);
  const requestingTaskId = typeof command.id === 'string' && command.id.trim() ? command.id : sourceId;
  // Rust: enqueue_web_stack_auth_assist_request(root, auth_source, None, None, None, None,
  //   requesting_task_id, "news", "news.source.auth_assist", Some(owner), true, true)
  const effect = await ctx.cap.browser.auth_assist.request({
    source_id: authSource,
    target_url_override: null,
    credential_ref: null,
    login_hint: null,
    preferred_auth_assist: null,
    requesting_task_id: requestingTaskId,
    source_module: NEWS_MODULE,
    command_path: NEWS_SOURCE_AUTH_ASSIST,
    // CAP-3b #844 caps/browser.rs AuthAssistRequest (deny_unknown_fields): owner = ctx.actor,
    // deterministic/trusted setzt der Kern; owner_user_id/deterministic_for_task/accept_as_trusted_local sind keine Felder.
  });
  const nextPayload = { ...(source.payload && typeof source.payload === 'object' && !Array.isArray(source.payload) ? source.payload : {}) };
  nextPayload.auth_assist = effect;
  await patchRecord(ctx, 'news_sources', sourceId, ctx.now(), {
    status: 'auth_requested',
    account_handle: source.account_handle ?? '',
    payload: nextPayload,
  });
  return { ok: true, source_id: sourceId, auth_assist: effect };
}
