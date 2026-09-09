// The advertised subscription catalog can reject a mismatched selection, but
// a matching entry alone never proves that an actual model request will work.
export function subscriptionModelUnavailable(settings) {
  const runtime = settings?.runtime || {};
  const provider = String(runtime.provider || '').trim().toLowerCase();
  const model = String(runtime.chat_model || '').trim().toLowerCase();
  const mode = String(settings?.auth?.mode || '').trim().toLowerCase();
  if (!model || !['subscription', 'chatgpt_subscription', 'codex_subscription', 'chatgpt'].includes(mode)) return false;
  const catalog = runtime.available_models_by_provider?.[provider];
  return Array.isArray(catalog) && catalog.length > 0 && !catalog.some(entry => (
    String(typeof entry === 'string' ? entry : entry?.id || '').trim().toLowerCase() === model
  ));
}
