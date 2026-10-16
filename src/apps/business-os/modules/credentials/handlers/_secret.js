// Origin: CTOX
// License: Apache-2.0
// ref: store.rs:15171-15185 is_valid_credential_key @ 4794af03d – Schlüssel im Env-Format (UPPER_SNAKE_CASE, ≤ 64).
export function isValidCredentialKey(name) {
  const trimmed = String(name ?? '').trim();
  if (trimmed === '' || trimmed.length > 64) return false;
  return /^[A-Z][A-Z0-9_]*$/.test(trimmed);
}
export const credentialName = (payload) => (typeof payload?.name === 'string' ? payload.name.trim() : '');

// CAP-3 #831 (caps/secrets.rs resolve_scope): ohne `scope` legt der Kern ein Geheimnis im App-Schlüsselbund
// `app:credentials` ab. Instanzschlüssel, die CTOX selbst liest (Anbieter-Schlüssel, Postfach-/IoT-Passwörter …),
// gehören in den Instanz-Schlüsselbund `credentials` (nur Rollen mit secrets.manage, prüft der Kern).
// Katalog = secrets.rs CREDENTIAL_CATALOG (known_credential_keys) – vorher lieferte ihn list_credentials_command.
export const INSTANCE_SCOPE = 'credentials';
export const INSTANCE_CREDENTIALS = Object.freeze([
  ['OPENAI_API_KEY', 'OpenAI API key'],
  ['ANTHROPIC_API_KEY', 'Anthropic (Claude) API key'],
  ['OPENROUTER_API_KEY', 'OpenRouter API key'],
  ['MISTRAL_API_KEY', 'Mistral API key'],
  ['CTOX_MISTRAL_API_KEY', 'Mistral API key (CTOX-scoped)'],
  ['KIMI_API_KEY', 'Kimi Coding Plan API key'],
  ['MINIMAX_API_KEY', 'MiniMax API key'],
  ['CTOX_LLM_PROXY_API_KEY', 'CTOX LLM proxy API key'],
  ['CTOX_TURN_EDGE_KEY', 'CTOX TURN edge key'],
  ['AZURE_FOUNDRY_API_KEY', 'Azure AI Foundry API key'],
  ['DATABASE_URL', 'Database connection URL'],
  ['CTO_EMAIL_PASSWORD', 'Email account password'],
  ['CTO_EMAIL_GRAPH_PASSWORD', 'Microsoft Graph email password'],
  ['CTO_EMAIL_GRAPH_CLIENT_SECRET', 'Microsoft Graph email client secret'],
  ['CTO_EMAIL_GRAPH_ACCESS_TOKEN', 'Microsoft Graph email access token'],
  ['CTO_TEAMS_PASSWORD', 'Microsoft Teams password'],
  ['CTO_TEAMS_CLIENT_SECRET', 'Microsoft Teams client secret'],
  ['CTO_TEAMS_GRAPH_ACCESS_TOKEN', 'Microsoft Teams Graph access token'],
  ['CTOX_WEBRTC_PASSWORD', 'Business OS WebRTC room password'],
  ['CTO_IOT_MQTT_PASSWORD', 'IoT MQTT device password'],
  ['CTO_IOT_HTTP_AUTH_HEADER', 'IoT HTTP auth header'],
  ['CTO_IOT_WS_AUTH_HEADER', 'IoT WebSocket auth header'],
  ['LEADFEEDER_API_KEY', 'Leadfeeder API key'],
  ['DNB_DIRECT_API_KEY', 'D&B Direct+ API key'],
  ['LINKEDIN_SALES_NAV_TOKEN', 'LinkedIn Sales Navigator token'],
  ['XING_API_TOKEN', 'XING API token'],
  ['HF_TOKEN', 'Hugging Face token'],
  ['HUGGINGFACE_HUB_TOKEN', 'Hugging Face Hub token'],
].map(([name, description]) => Object.freeze({ name, description })));
const INSTANCE_NAMES = new Set(INSTANCE_CREDENTIALS.map((entry) => entry.name));
export const isInstanceCredential = (name) => INSTANCE_NAMES.has(name);
/** Kern-Argumente für secret.put/delete: Instanzschlüssel mit scope 'credentials', sonst App-Scope (kein Feld). */
export const scopeArgs = (name) => (isInstanceCredential(name) ? { scope: INSTANCE_SCOPE } : {});
