import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import { __reactSettingsTestHooks as hooks } from './react-settings.js';

const reactSettingsSource = readFileSync(new URL('./react-settings.js', import.meta.url), 'utf8');

test('runtime save button forwards command dependencies and missing access is a visible warning', () => {
  const handler = reactSettingsSource.slice(
    reactSettingsSource.indexOf("body.querySelector('[data-runtime-save]')"),
    reactSettingsSource.indexOf("body.querySelector('[data-runtime-refresh]')"),
  );
  assert.match(handler, /saveRuntimeSettings\(\s*runtimePayload,\s*\{ commandBus, db, session, sync \}/);
  const html = baseTemplate({
    tab: 'runtime',
    runtimeSettings: {
      can_manage: true,
      runtime: { provider: 'minimax', chat_model: 'MiniMax-M3' },
      auth: { mode: 'api_key', api_key_configured: false, api_key_name: 'CTOX_SUBSCRIPTION_PROXY_NO_AUTH' },
      diagnostics: { auth_needs_attention: true, service_needs_attention: false },
    },
  });
  assert.match(html, /role="alert"/);
  assert.match(html, /Die Crew kann keine Aufgaben bearbeiten/);
  assert.match(html, /API-Schlüssel fehlt/);
  assert.doesNotMatch(html, /CTOX_SUBSCRIPTION_PROXY_NO_AUTH/);
});

globalThis.document = {
  documentElement: {
    lang: 'de',
    dataset: {},
  },
};

test('CTOX proxy model options follow the discovered proxy catalog', () => {
  assert.deepEqual(
    hooks.runtimeModelOptions('ctox_proxy', 'kimi-k3', ['MiniMax-M3', 'kimi-k3']),
    [
      ['MiniMax-M3', 'MiniMax-M3'],
      ['kimi-k3', 'kimi-k3'],
    ],
  );
  assert.deepEqual(
    hooks.runtimeModelOptions('ctox_proxy', '', []),
    [
      ['', 'Nicht gesetzt'],
      ['MiniMax-M3', 'MiniMax-M3'],
      ['kimi-k3', 'kimi-k3'],
    ],
  );
});

test('OpenAI model options prefer the CLIProxy-discovered 5.6 inference catalog', () => {
  assert.deepEqual(
    hooks.runtimeModelOptions('openai', '', [
      { id: 'gpt-5.5' },
      { id: 'gpt-image-2' },
      { id: 'gpt-5.6-luna' },
      { id: 'codex-auto-review' },
      { id: 'gpt-5.6-sol' },
      { id: 'gpt-5.6-terra' },
    ]),
    [
      ['', 'Nicht gesetzt'],
      ['gpt-5.6-sol', 'gpt-5.6-sol'],
      ['gpt-5.6-terra', 'gpt-5.6-terra'],
      ['gpt-5.6-luna', 'gpt-5.6-luna'],
      ['gpt-5.5', 'gpt-5.5'],
    ],
  );
  assert.deepEqual(
    hooks.runtimeModelOptions('openai', '', []),
    [
      ['', 'Nicht gesetzt'],
      ['gpt-5.6-sol', 'gpt-5.6-sol'],
      ['gpt-5.6-terra', 'gpt-5.6-terra'],
      ['gpt-5.6-luna', 'gpt-5.6-luna'],
      ['gpt-5.4', 'gpt-5.4'],
      ['gpt-5.4-mini', 'gpt-5.4-mini'],
    ],
  );
});

test('subscription provider switches render their discovered model catalog before saving', () => {
  const html = baseTemplate({
    tab: 'runtime',
    runtimeSettings: {
      can_manage: true,
      runtime: {
        provider: 'openai', chat_model: '', reasoning_effort: '', available_models: [],
        available_models_by_provider: {
          openai: [
            { id: 'gpt-5.6-sol', reasoning_levels: ['low', 'medium', 'high', 'xhigh', 'max'] },
            { id: 'gpt-5.6-terra', reasoning_levels: ['low', 'medium', 'high', 'xhigh', 'max'] },
            { id: 'gpt-5.6-luna', reasoning_levels: ['low', 'medium', 'high', 'xhigh', 'max'] },
          ],
        },
      },
      auth: { mode: 'subscription', subscription_session_configured: true },
      diagnostics: {},
    },
  });
  assert.match(html, /data-value="gpt-5\.6-sol"/);
  assert.match(html, /data-value="gpt-5\.6-terra"/);
  assert.match(html, /data-value="gpt-5\.6-luna"/);
  assert.doesNotMatch(html, /data-value="gpt-5\.5"/);
});

test('runtime settings follow the Workjet provider-access-model-reasoning flow', () => {
  const html = baseTemplate({
    tab: 'runtime',
    runtimeSettings: {
      can_manage: true,
      runtime: {
        provider: 'kimi', chat_model: 'kimi-k3[1m]', reasoning_effort: 'high',
        available_models: ['kimi-k3[1m]'],
      },
      auth: { mode: 'subscription', configured: true, subscription_session_configured: true },
      diagnostics: {},
      provider_subscriptions: {
        schema: 'ctox.provider-subscriptions.v1',
        providers: [
          { id: 'codex', label: 'ChatGPT / Codex' },
          { id: 'claude', label: 'Claude' },
          { id: 'antigravity', label: 'Google Antigravity' },
          { id: 'kimi', label: 'Kimi', access_mode: 'Subscription', preset_ids: ['kimi-subscription-kimi-k3'] },
          { id: 'kimi_coding', label: 'Kimi Coding Plan', access_mode: 'Coding Plan', preset_ids: ['kimi-coding-kimi-k3'] },
          { id: 'minimax', label: 'MiniMax', access_mode: 'Coding Plan', preset_ids: ['minimax-coding-minimax-m3'] },
        ],
        accounts: [
          { id: 'claude-primary', provider: 'claude', enabled: true },
          {
            id: 'kimi-primary', provider: 'kimi', enabled: true, status: 'ready',
            models: ['kimi-k3[1m]'], preset_ids: ['kimi-subscription-kimi-k3'],
            access_token: 'must-never-render', refresh_token: 'also-secret',
          },
          {
            id: 'minimax-coding-primary', provider: 'minimax', enabled: true,
            models: ['MiniMax-M3'], preset_ids: ['minimax-coding-minimax-m3'],
          },
        ],
      },
    },
  });
  assert.doesNotMatch(html, /Provider Subscriptions/);
  assert.match(html, /Anbieter, Zugang und Modell/);
  assert.match(html, /data-runtime-choice="data-runtime-provider"/);
  assert.match(html, /data-runtime-choice="data-runtime-auth-mode"/);
  assert.match(html, /data-runtime-choice="data-runtime-model"/);
  assert.match(html, /data-runtime-choice="data-runtime-reasoning"/);
  assert.match(html, /data-value="high"[^>]*aria-pressed="true"/);
  assert.doesNotMatch(html, /Queue Policy|Preset|Context/);
  assert.match(html, /data-runtime-authorize-subscription="kimi"/);
  assert.match(html, /kimi-k3\[1m\]/);
  assert.doesNotMatch(html, /Neue Account-ID|von dieser CTOX Instanz noch nicht angeboten/);
  assert.doesNotMatch(html, /must-never-render|also-secret|access_token|refresh_token/);
});

test('provider logo mapping reuses brand artwork across access modes and fails closed to a mark', () => {
  assert.deepEqual(hooks.providerLogoSpec('codex'), {
    provider: 'codex', asset: 'openai.svg', fallbackMark: 'O',
  });
  assert.deepEqual(hooks.providerLogoSpec('kimi_coding'), {
    provider: 'kimi_coding', asset: 'kimi.svg', fallbackMark: 'K',
  });
  assert.deepEqual(hooks.providerLogoSpec('unknown-provider'), {
    provider: 'unknown-provider', asset: '', fallbackMark: 'U',
  });
  const fallback = hooks.providerLogoHtml('unknown-provider');
  assert.match(fallback, /data-logo-state="fallback"/);
  assert.match(fallback, />U<\/span>/);
  assert.doesNotMatch(fallback, /<img/);
});

test('provider logo loader exposes the fallback when an artwork asset cannot load', () => {
  const parentElement = { dataset: { logoState: 'artwork' } };
  const image = {
    complete: true,
    naturalWidth: 0,
    hidden: false,
    parentElement,
    addEventListener() {},
  };
  hooks.wireProviderLogoFallbacks({
    querySelectorAll: () => [image],
  });
  assert.equal(image.hidden, true);
  assert.equal(parentElement.dataset.logoState, 'fallback');
});

test('subscription provider artwork preserves the pinned Workjet byte hashes', () => {
  // Source: metric-space-ai/claude-workjet@4341f5d, Resources/Providers/*.svg.
  const expected = {
    'openai.svg': '14ebfcb27d0afdf729c8c2cc086b0793ace642bd03fbccea98688479222f5c65',
    'anthropic.svg': 'b09586c10df19be04289eaa70f15b98e797ffdcd8cbe71f998d17dbebae5faa4',
    'antigravity.svg': 'd30cdf2e33d2c1c694f0096694d518a2db05b0f5311a330aabb2ac68df4e3789',
    'kimi.svg': 'cf348773f67e13abb6d3d15ae94798028085918d131dab93e3192450ba035d34',
  };
  for (const [fileName, expectedSha256] of Object.entries(expected)) {
    const bytes = readFileSync(new URL(`../assets/provider-logos/${fileName}`, import.meta.url));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), expectedSha256, fileName);
  }
});

test('provider account commands carry only typed non-secret selectors', () => {
  assert.deepEqual(
    hooks.providerSubscriptionCommandRequest('connect', 'kimi_coding', 'kimi-coding-primary'),
    {
      commandType: 'ctox.subscription_auth.start',
      payload: { provider: 'kimi_coding', account_id: 'kimi-coding-primary' },
    },
  );
  assert.deepEqual(
    hooks.providerSubscriptionCommandRequest('connect', 'kimi', 'kimi-primary'),
    {
      commandType: 'ctox.subscription_auth.start',
      payload: { provider: 'kimi', account_id: 'kimi-primary' },
    },
  );
  assert.deepEqual(
    hooks.providerSubscriptionCommandRequest('rotate', 'minimax', 'minimax-coding-primary'),
    {
      commandType: 'ctox.provider_subscription.rotate',
      payload: { provider: 'minimax', account_id: 'minimax-coding-primary' },
    },
  );
  assert.deepEqual(
    hooks.providerSubscriptionCommandRequest('disconnect', 'kimi', 'kimi-primary'),
    {
      commandType: 'ctox.provider_subscription.disconnect',
      payload: { provider: 'kimi', account_id: 'kimi-primary' },
    },
  );
  assert.deepEqual(
    hooks.providerSubscriptionCommandRequest('status'),
    { commandType: 'ctox.provider_subscription.status', payload: {} },
  );
});

test('subscription login starts through the typed business command bus', async () => {
  const calls = [];
  const payload = await hooks.startSubscriptionAuth('codex', 'codex-primary', {
    commandBus: {
      dispatch: async (command, options) => {
        calls.push({ command, options });
        return { result: {
          status: 'device_code',
          user_code: 'ABCD-EFGH',
          verification_url: 'https://auth.example/device',
        } };
      },
    },
    db: { collection: (name) => (name === 'business_commands' ? {} : null) },
    session: { user: { id: 'owner', role: 'chef', is_admin: true } },
    sync: { startCollection: async () => {} },
  });
  assert.equal(payload.status, 'device_code');
  assert.equal(payload.source, 'business_commands');
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command.type, 'ctox.subscription_auth.start');
  assert.deepEqual(calls[0].command.payload, { provider: 'codex', account_id: 'codex-primary' });
  assert.equal(calls[0].options.until, 'terminal');
  assert.doesNotMatch(JSON.stringify(calls[0].command), /token|secret|api[_-]?key/i);
});

test('runtime refresh preserves the selected subscription provider while device login is pending', () => {
  const loaded = {
    runtime: {
      provider: 'ctox_proxy',
      chat_model: 'MiniMax-M3',
      preset: 'Quality',
      context: '256k',
      max_run_secs: 1800,
      available_models: ['MiniMax-M3'],
    },
    auth: { mode: 'api_key', api_key_configured: true },
    diagnostics: { service_message: 'CTOX Service läuft.' },
  };
  const draft = {
    ...loaded,
    runtime: {
      ...loaded.runtime,
      provider: 'openai',
      chat_model: '',
      available_models: [],
    },
    auth: { mode: 'subscription', subscription_selected: true },
  };
  const result = hooks.runtimeSettingsPreservingPendingSubscription(loaded, draft, {
    status: 'device_code',
    provider: 'codex',
    userCode: 'ABCD-EFGHI',
  });
  assert.equal(result.runtime.provider, 'openai');
  assert.equal(result.auth.mode, 'subscription');
  assert.equal(result.auth.subscription_selected, true);
  assert.equal(result.auth.subscription_session_configured, false);
  assert.deepEqual(result.diagnostics, loaded.diagnostics);
});

test('runtime refresh uses the persisted runtime when no subscription login is pending', () => {
  const loaded = {
    runtime: { provider: 'ctox_proxy', chat_model: 'MiniMax-M3' },
    auth: { mode: 'api_key', api_key_configured: true },
  };
  const result = hooks.runtimeSettingsPreservingPendingSubscription(
    loaded,
    { runtime: { provider: 'openai' }, auth: { mode: 'subscription' } },
    null,
  );
  assert.equal(result, loaded);
});

test('pending OpenAI device login renders code and provider link in the runtime menu', () => {
  const html = baseTemplate({
    tab: 'runtime',
    runtimeSettings: {
      can_manage: true,
      runtime: { provider: 'openai', chat_model: '', available_models: [] },
      auth: { mode: 'subscription', subscription_session_configured: false },
      diagnostics: {},
    },
    subscriptionAuth: {
      status: 'device_code',
      provider: 'codex',
      userCode: 'ABCD-EFGHI',
      verificationUrl: 'https://auth.openai.com/codex/device',
    },
  });
  assert.match(html, /ABCD-EFGHI/);
  assert.match(html, /href="https:\/\/auth\.openai\.com\/codex\/device"/);
  assert.match(html, />OpenAI öffnen<\/a>/);
  assert.match(html, /data-runtime-copy-code="ABCD-EFGHI"/);
  assert.match(html, /Geräte-Code kopieren/);
});

test('projected subscription status is authoritative', () => {
  const projection = {
    schema: 'ctox.provider-subscriptions.v1',
    providers: [{ id: 'codex', label: 'ChatGPT / Codex' }],
    accounts: [{ id: 'codex-primary', provider: 'codex', enabled: true, status: 'ready' }],
  };
  assert.equal(hooks.subscriptionProviderConnected(projection, 'codex'), true);
  assert.equal(hooks.subscriptionProviderConnected(projection, 'claude'), false);
});

test('runtime settings save uses the typed business command bus', async () => {
  const calls = [];
  const request = {
    provider: 'openai',
    auth_mode: 'subscription',
    chat_model: 'gpt-5.5',
    reasoning_effort: 'high',
    preset: 'Quality',
    context: '256k',
    max_run_secs: 1800,
    api_key: '',
  };
  const result = await hooks.saveRuntimeSettings(request, {
    commandBus: {
      dispatch: async (command, options) => {
        calls.push({ command, options });
        return { result: { ok: true } };
      },
    },
    db: { collection: (name) => (name === 'business_commands' ? {} : null) },
    session: { user: { id: 'owner', role: 'chef', is_admin: true } },
    sync: { startCollection: async () => {} },
    waitForProjection: false,
  });
  assert.equal(calls[0].command.type, 'ctox.runtime_settings.save');
  assert.deepEqual(calls[0].command.payload, request);
  assert.equal(calls[0].options.until, 'accepted');
  assert.deepEqual(result, { ok: true });
});

test('runtime settings reload reads the authoritative RxDB projection', async () => {
  const projected = {
    runtime: { provider: 'openai', chat_model: 'gpt-5.5', reasoning_effort: 'high' },
    auth: { mode: 'subscription', subscription_session_configured: true },
  };
  const runtimeSettings = await hooks.loadRuntimeSettings({
    db: {
      collection: (name) => name === 'ctox_runtime_settings' ? {
        findOne: () => ({ exec: async () => ({ toJSON: () => projected }) }),
      } : null,
    },
  });
  assert.equal(runtimeSettings.runtime.provider, 'openai');
  assert.equal(runtimeSettings.runtime.reasoning_effort, 'high');
});

test('ready subscription projection renders one consistent connected state', () => {
  const html = baseTemplate({
    tab: 'runtime',
    runtimeSettings: {
      can_manage: true,
      runtime: { provider: 'openai', chat_model: 'gpt-5.5', available_models: ['gpt-5.5'] },
      auth: { mode: 'subscription', subscription_session_configured: false },
      diagnostics: { service_message: 'CTOX Service läuft.' },
      provider_subscriptions: {
        schema: 'ctox.provider-subscriptions.v1',
        providers: [{ id: 'codex', label: 'ChatGPT / Codex' }],
        accounts: [{ id: 'codex-primary', provider: 'codex', enabled: true, status: 'ready' }],
      },
    },
  });
  assert.match(html, /Subscription verbunden/);
  assert.match(html, /ChatGPT \/ Codex ist verbunden und einsatzbereit/);
  assert.doesNotMatch(html, /Subscription nicht verbunden|CTOX-Harness zur Verfügung/);
});

test('reasoning choices track the selected model capability', () => {
  assert.deepEqual(hooks.runtimeReasoningOptions('openai', 'gpt-5.5'), ['low', 'medium', 'high', 'xhigh']);
  assert.deepEqual(hooks.runtimeReasoningOptions('openai', 'gpt-5.6-luna'), ['low', 'medium', 'high', 'xhigh', 'max']);
  assert.deepEqual(hooks.runtimeReasoningOptions('openai', 'gpt-5.6-sol'), ['low', 'medium', 'high', 'xhigh', 'max']);
  assert.deepEqual(
    hooks.runtimeReasoningOptions('openai', 'future-model', [
      { id: 'future-model', reasoning_levels: ['low', 'high', 'max'] },
    ]),
    ['low', 'high', 'max'],
  );
});

test('subscription auth always opens a fresh same-origin code window', () => {
  const first = hooks.nextSubscriptionAuthWindowName('codex');
  const second = hooks.nextSubscriptionAuthWindowName('codex');
  assert.match(first, /^ctox-codex-subscription-\d+-\d+$/);
  assert.notEqual(second, first);
});

test('all subscription providers expose one secret-free connect/status/rotate/disconnect story', () => {
  const accounts = {
    codex: 'codex-instance-primary',
    claude: 'claude-primary',
    antigravity: 'antigravity-primary',
    kimi: 'kimi-primary',
  };
  for (const [provider, accountId] of Object.entries(accounts)) {
    assert.deepEqual(hooks.providerSubscriptionCommandRequest('connect', provider, accountId), {
      commandType: 'ctox.subscription_auth.start',
      payload: { provider, account_id: accountId },
    });
    assert.deepEqual(hooks.providerSubscriptionCommandRequest('status', provider), {
      commandType: 'ctox.provider_subscription.status',
      payload: { provider },
    });
    assert.deepEqual(hooks.providerSubscriptionCommandRequest('rotate', provider, accountId), {
      commandType: 'ctox.provider_subscription.rotate',
      payload: { provider, account_id: accountId },
    });
    assert.deepEqual(hooks.providerSubscriptionCommandRequest('disconnect', provider, accountId), {
      commandType: 'ctox.provider_subscription.disconnect',
      payload: { provider, account_id: accountId },
    });
  }
});

test('coding-plan credential guidance is allowlisted and never renders native secrets or URLs', () => {
  assert.deepEqual(
    hooks.providerCredentialRequirement({
      status: 'credential_required',
      credential_name: 'KIMI_API_KEY',
      message: 'ignore native text https://secret.invalid token=abc',
      token: 'abc',
    }, 'kimi_coding'),
    {
      credentialName: 'KIMI_API_KEY',
      instruction: 'KIMI_API_KEY im verschlüsselten CTOX Credential-Bereich hinterlegen und danach den Account erneut verbinden.',
    },
  );
  assert.throws(
    () => hooks.providerCredentialRequirement({
      status: 'credential_required', credential_name: 'OTHER_SECRET',
    }, 'minimax'),
    /ungültige Credential-Anforderung/,
  );

  const rendered = JSON.stringify(hooks.providerCredentialRequirement({
    status: 'credential_required', credential_name: 'KIMI_API_KEY',
    message: 'ignore native text https://secret.invalid token=abc', token: 'abc',
  }, 'kimi_coding'));
  assert.match(rendered, /verschlüsselten CTOX Credential-Bereich/);
  assert.doesNotMatch(rendered, /secret\.invalid|token=abc|OTHER_SECRET/);
});

test('provider account command validation fails closed', () => {
  assert.throws(
    () => hooks.providerSubscriptionCommandRequest('connect', 'unknown', 'primary'),
    /Unbekannter Subscription-Provider/,
  );
  assert.throws(
    () => hooks.providerSubscriptionCommandRequest('rotate', 'kimi', '../secret'),
    /Account-ID/,
  );
  assert.throws(
    () => hooks.providerSubscriptionCommandRequest('delete', 'kimi', 'kimi-primary'),
    /Unbekannte Provider-Aktion/,
  );
});

test('provider projection normalizer retains only public account and preset fields', () => {
  const projection = hooks.normalizeProviderSubscriptions({
    schema: 'ctox.provider-subscriptions.v1',
    revision: 4,
    providers: [{ id: 'kimi', label: 'Kimi', access_mode: 'Subscription' }],
    accounts: [{
      id: 'kimi-primary', provider: 'kimi', enabled: true, status: 'ready',
      models: ['kimi-k3[1m]'], preset_ids: ['kimi-subscription-kimi-k3'],
      access_token: 'secret', upstream_url: 'private-route',
    }],
  });
  assert.equal(projection.revision, 4);
  assert.deepEqual(projection.accounts, [{
    id: 'kimi-primary',
    provider: 'kimi',
    enabled: true,
    status: 'ready',
    models: ['kimi-k3[1m]'],
    presetIds: ['kimi-subscription-kimi-k3'],
  }]);
  assert.equal('access_token' in projection.accounts[0], false);
  assert.equal('upstream_url' in projection.accounts[0], false);
  assert.deepEqual(
    hooks.normalizeProviderSubscriptions({
      schema: 'forged',
      providers: [{ id: 'kimi' }],
      accounts: [{ id: 'forged', provider: 'kimi', enabled: true }],
    }).accounts,
    [],
  );
});

function baseTemplate(overrides = {}) {
  return hooks.settingsTemplate({
    modules: [{ id: 'inventory', title: 'Inventory', description: '', entry: 'index.html' }],
    managedModules: [{ id: 'inventory', title: 'Inventory', description: '', entry: 'index.html' }],
    templates: [],
    session: { user: { id: 'owner', role: 'chef' } },
    syncConfig: {},
    user: { id: 'owner', display_name: 'Owner User', role: 'chef' },
    role: 'chef',
    isAdmin: true,
    canOpenAdmin: true,
    tab: 'users',
    commandStatus: '',
    subscriptionAuth: null,
    runtimeSettings: null,
    runtimeLoading: false,
    users: [{ id: 'owner', display_name: 'Owner User', role: 'chef', active: true }],
    canManageUsers: true,
    activity: { events: [], loading: false, loaded: false, error: '' },
    branding: {
      loading: false,
      document: {
        id: 'workspace-branding',
        name: 'Acme Corporate',
        custom: true,
        light: { bg: '#ffffff', text: '#111827' },
        dark: { bg: '#030712', text: '#f9fafb' },
        module_accents: {},
        updated_at_ms: 123,
      },
      jsonText: '{\n  "name": "Acme Corporate"\n}',
      error: '',
      canManage: true,
    },
    mcp: { loading: false, info: null, error: '', copied: '' },
    editingModuleId: '',
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {},
    },
    channels: { accounts: [], data: {}, status: '' },
    ...overrides,
  });
}

test('sync settings render only signaling server, room, password, QR code, and link', () => {
  const html = baseTemplate({
    tab: 'sync',
    syncConfig: {
      app_hosting: 'ctox_dev_web_deploy',
      sync_mode: 'p2p-first',
      transport: 'webrtc',
      peer_role: 'browser',
      instance_id: 'biz_test',
      native_peer_id: 'ctox-core-test',
      sync_room: 'ctox-business-os:biz_test:room',
      signaling_room_password: 'room-password-test',
      signaling_auth_version: 'ctox-role-bound-v1',
      signaling_urls: [
        'wss://signaling.ctox.dev/v2?token=secret&token_exp=32503680000',
      ],
    },
    workjetPairing: { loading: false, invite: null, error: '' },
  });
  assert.match(html, /Signaling-Server/);
  assert.match(html, />Raum</);
  assert.match(html, />Passwort</);
  assert.match(html, />QR-Code</);
  assert.match(html, />Link</);
  assert.match(html, /ctox-business-os:biz_test:room/);
  assert.match(html, /••••••••••••/);
  assert.doesNotMatch(html, /room-password-test/);
  assert.match(html, /wss:\/\/signaling\.ctox\.dev\/v2/);
  assert.match(html, /data-sync-copy="signaling"/);
  assert.match(html, /data-sync-copy="room"/);
  assert.match(html, /data-sync-copy="password"/);
  assert.match(html, /data-sync-password-toggle/);
  assert.match(html, /data-workjet-pairing-generate/);
  assert.doesNotMatch(html, /data-workjet-pairing-ready|workjet:\/\/pair\?payload=/);
  assert.doesNotMatch(html, /Business-OS-Hosting|Workjet verbinden|Technische Verbindung/);
  assert.doesNotMatch(html, /App-Hosting|Sync-Modus|Transport|Peer-Rolle|Instanz/);
  assert.doesNotMatch(html, /Zugang|Gültig bis|Ziel-Peer|Verbindungsraum/);
  assert.doesNotMatch(html, /Gerätename|QR-Code erstellen|Neuen QR-Code|Sync Konfiguration/);
  assert.doesNotMatch(html, /Klartext-Passwort|Kurzzeit-Token|Token ist im QR/);
});

test('sync settings use the role-bound signaling token when no room password is exposed', () => {
  const html = baseTemplate({
    tab: 'sync',
    syncConfig: {
      sync_room: 'ctox-business-os:biz_test:room',
      signaling_browser_token: 'role-bound-browser-token',
      signaling_urls: ['wss://signaling.ctox.dev/v2'],
    },
    workjetPairing: { loading: false, invite: null, error: '' },
  });
  assert.match(html, /••••••••••••/);
  assert.doesNotMatch(html, /role-bound-browser-token/);
  assert.doesNotMatch(html, /<dt>Passwort<\/dt><dd>-<\/dd>/);
  assert.equal(hooks.syncCopyValue('password', {
    signaling_browser_token: 'role-bound-browser-token',
  }), 'role-bound-browser-token');

  const revealed = baseTemplate({
    tab: 'sync',
    syncConfig: { signaling_browser_token: 'role-bound-browser-token' },
    workjetPairing: { loading: false, invite: null, error: '', passwordVisible: true },
  });
  assert.match(revealed, /role-bound-browser-token/);
});

test('Workjet pairing response is validated and rendered as an inert SVG image', async () => {
  const response = {
    pairingUri: `workjet://pair?payload=${'A'.repeat(64)}`,
    qrSvg: '<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1H0V0"/></svg>',
    expiresAt: '2999-01-01T00:00:00.000Z',
  };
  const calls = [];
  const invite = await hooks.createWorkjetPairingInvite({
    requestNative: async (...args) => {
      calls.push(args);
      return response;
    },
  }, 'Fold 8');
  assert.deepEqual(invite, response);
  assert.equal(calls[0][0], 'ctox.workjet.device.v1');
  assert.deepEqual(calls[0][1], {
    action: 'invite.create', ttlSeconds: 300, displayName: 'Fold 8',
  });
  assert.equal(calls[0][2].requiredCapability, 'ctox-workjet-device-control-v1');
  assert.match(hooks.workjetPairingSvgDataUrl(response.qrSvg), /^data:image\/svg\+xml/);

  const html = baseTemplate({
    tab: 'sync',
    syncConfig: {},
    workjetPairing: { loading: false, invite, error: '' },
  });
  assert.match(html, /data-workjet-pairing-ready/);
  assert.match(html, /alt="Workjet Pairing QR-Code"/);
  assert.match(html, /data-workjet-pairing-link/);
  assert.match(html, /data-sync-copy="link"/);
  assert.match(html, /href="workjet:\/\/pair\?payload=/);
  assert.match(html, /Gültig für/);
  assert.doesNotMatch(html, /QR-Code mit Workjet scannen|In Workjet öffnen|Gültig bis/);
  assert.doesNotMatch(html, /<script/i);
});

test('Workjet pairing countdown reports the remaining validity window', () => {
  const now = Date.parse('2026-08-29T12:00:00.000Z');
  assert.equal(
    hooks.workjetPairingRemainingLabel('2026-08-29T12:05:00.000Z', now),
    'Gültig für 5:00',
  );
  assert.equal(
    hooks.workjetPairingRemainingLabel('2026-08-29T11:59:00.000Z', now),
    'Gültig für 0:00',
  );
});

test('Workjet pairing retries while the native WebRTC peer is still negotiating', async () => {
  const response = {
    pairingUri: `workjet://pair?payload=${'B'.repeat(64)}`,
    qrSvg: '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1H0V0"/></svg>',
    expiresAt: '2999-01-01T00:00:00.000Z',
  };
  let attempts = 0;
  const invite = await hooks.createWorkjetPairingInvite({
    requestNative: async () => {
      attempts += 1;
      if (attempts === 1) throw new Error('WebRTC peer is closed: protocol-incompatible');
      return response;
    },
  }, 'Phone', [0, 0]);
  assert.equal(attempts, 2);
  assert.deepEqual(invite, response);
});

test('settings user tab renders business-facing role labels', () => {
  const html = baseTemplate();
  assert.match(html, /Team & Zugänge/);
  assert.match(html, /Teammitglied/);
  assert.match(html, /App-Verantwortliche:r/);
  assert.match(html, /Owner/);
  assert.doesNotMatch(html, />User<\/option>/);
  assert.doesNotMatch(html, />Founder<\/option>/);
  assert.doesNotMatch(html, />Chef<\/option>/);
  assert.doesNotMatch(html, /User Management/);
  assert.doesNotMatch(html, /Founder Review/);
});

test('settings users stay subscribed while WebRTC replication fills the collection', () => {
  assert.match(reactSettingsSource, /db\.collection\('business_users'\)\.\$\.subscribe/);
  assert.match(reactSettingsSource, /refreshUsers\(\)\.catch/);
  assert.match(reactSettingsSource, /usersSub\?\.unsubscribe/);
});

test('settings appearance tab renders admin branding controls', () => {
  const html = baseTemplate({ tab: 'appearance' });
  assert.match(html, /data-settings-tab="appearance"/);
  assert.match(html, /Corporate Design/);
  assert.match(html, /data-branding-json/);
  assert.match(html, /data-branding-save/);
  assert.match(html, /data-branding-reset/);
  assert.match(html, /Acme Corporate/);
});

test('compact settings tabs expose their complete labels on hover and to assistive technology', () => {
  const html = baseTemplate({ tab: 'runtime' });
  const expectedTabs = [
    ['runtime', 'Runtime'],
    ['channels', 'Kanäle'],
    ['sync', 'Sync'],
    ['appearance', 'Design'],
    ['mcp', 'MCP'],
    ['users', 'Team'],
    ['activity', 'Aktivität'],
    ['admin', 'Module'],
  ];

  for (const [id, label] of expectedTabs) {
    assert.match(
      html,
      new RegExp(`data-settings-tab="${id}" title="${label}" aria-label="${label}"`),
    );
  }
});

test('settings appearance tab hides when branding permission is absent', () => {
  const html = baseTemplate({
    tab: 'runtime',
    branding: {
      loading: false,
      document: null,
      jsonText: '',
      error: '',
      canManage: false,
    },
  });
  assert.doesNotMatch(html, /data-settings-tab="appearance"/);
  assert.doesNotMatch(html, /data-branding-save/);
});

test('settings MCP panel does not expose unsupported native managed token issuance', () => {
  const html = baseTemplate({
    tab: 'mcp',
    mcp: {
      loading: false,
      error: '',
      copied: '',
      info: {
        ok: true,
        status: 'local_ready_managed_not_connected',
        mode: 'local',
        server_name: 'workspace-business-os-local',
        endpoint: 'http://127.0.0.1:8788/mcp',
        managed: {
          status: 'not_connected',
          endpoint: 'https://mcp.ctox.dev/mcp/workspace',
        },
      },
    },
  });

  assert.match(html, /Managed Endpoint kopieren/);
  assert.match(html, /Agent Tokens werden im ctox\.dev Dashboard rotiert/);
  assert.doesNotMatch(html, /data-mcp-issue-managed/);
  assert.doesNotMatch(html, /Agent Token erstellen/);
  assert.doesNotMatch(reactSettingsSource, /\/api\/business-os\/mcp\/client-token/);
});

test('settings user form keeps owner transfer owner-only', () => {
  const ownerHtml = baseTemplate();
  assert.match(ownerHtml, /<option value="chef">Owner<\/option>/);

  const adminHtml = baseTemplate({
    session: { user: { id: 'ops-admin', role: 'admin' } },
    user: { id: 'ops-admin', display_name: 'Ops Admin', role: 'admin' },
    role: 'admin',
    isAdmin: true,
    users: [{ id: 'ops-admin', display_name: 'Ops Admin', role: 'admin', active: true }],
    canManageUsers: true,
  });
  assert.match(adminHtml, /<option value="admin">Admin<\/option>/);
  assert.doesNotMatch(adminHtml, /<option value="chef">Owner<\/option>/);
});

test('settings user save keeps confirmed upsert result without stale projection reload', () => {
  const users = hooks.confirmedUsersAfterUpsert(
    [
      { id: 'alice', display_name: 'Alice Old', role: 'admin', active: true },
      { id: 'bob', display_name: 'Bob', role: 'user', active: true },
    ],
    null,
    { id: 'alice', display_name: 'Alice New', role: 'admin', active: true, updated_at_ms: 99 },
    { user: { id: 'owner', role: 'admin' } },
  );

  assert.deepEqual(users.map((user) => [user.id, user.display_name]), [
    ['alice', 'Alice New'],
    ['bob', 'Bob'],
  ]);
});

test('settings module tab renders responsible-app wording', () => {
  const html = baseTemplate({ tab: 'admin' });
  assert.match(html, /Verantwortlich: founder-a/);
  assert.match(html, /Verantwortliche:n zuweisen/);
  assert.match(html, /data-module-why="inventory"/);
  assert.match(html, /data-module-support-diagnostics="inventory"/);
  assert.match(html, /Warum\?/);
  assert.match(html, /Support-Paket/);
  assert.match(html, /Freigabe im App Store/);
  assert.match(html, /Settings zeigt Release und Rollback nur als Diagnose/);
  assert.doesNotMatch(html, /data-module-release="/);
  assert.doesNotMatch(html, /data-module-rollback="/);
  assert.doesNotMatch(html, /Founder:/);
  assert.doesNotMatch(html, /Founder zuweisen/);
  assert.doesNotMatch(html, /Keine Datenbereiche deklariert/);
});

test('settings module why diagnostics renders native policy result without raw payload keys', () => {
  const html = baseTemplate({
    tab: 'admin',
    moduleWhyDiagnostics: {
      inventory: {
        ok: true,
        schema_version: 1,
        kind: 'business_os_why_diagnostics',
        actor: {
          id: 'qa-user',
          display_name: 'QA User',
          role: 'user',
        },
        module: {
          id: 'inventory',
          title: 'Inventory',
          version: '0.9.0',
          runtime_installed: true,
        },
        lifecycle: {
          visibility_state: 'private',
          current_semver: '0.9.0',
          public: false,
        },
        decisions: {
          visibility: {
            allowed: true,
            permission: 'apps.view',
            reason_code: 'explicit_or_responsible_app_view',
            display_reason: 'Diese App ist durch App-Verantwortung sichtbar.',
            source: 'explicit_app_view_or_app_responsibility',
          },
          open: {
            allowed: true,
            permission: 'apps.view',
            reason_code: 'explicit_or_responsible_app_view',
            display_reason: 'Allowed.',
          },
          modify: {
            allowed: false,
            permission: 'apps.modify',
            reason_code: 'role_or_scope_denied',
            display_reason: 'App-Aenderungen bleiben Verantwortlichen, Admins oder Ownern vorbehalten.',
          },
          source: {
            allowed: false,
            permission: 'apps.source.view',
            reason_code: 'role_or_scope_denied',
            display_reason: 'This role is not allowed to perform this action for the selected scope.',
          },
          release: {
            allowed: false,
            permission: 'apps.release',
            reason_code: 'role_or_scope_denied',
            display_reason: 'Freigaben brauchen ein Release-Recht fuer diese App.',
          },
          rollback: {
            allowed: false,
            permission: 'apps.rollback',
            reason_code: 'role_or_scope_denied',
            display_reason: 'Rollback braucht ein Rollback-Recht fuer diese App.',
          },
        },
        data_access: {
          status: 'reviewed',
          completed: true,
          areas: [
            {
              collection: 'inventory_items',
              read_review_state: 'granted',
              write_review_state: 'locked',
              read_decision: {
                allowed: true,
                permission: 'data.read',
                display_reason: 'Datenbereich ist zum Lesen freigegeben.',
                collection_decision: { grant_id: 'DO_NOT_LEAK_GRANT_ID' },
              },
              write_decision: {
                allowed: false,
                permission: 'data.write',
                reason_code: 'role_or_scope_denied',
                display_reason: 'Schreiben braucht ein Datenrecht fuer diesen Bereich.',
                module_decision: { reason_code: 'DO_NOT_LEAK_REASON_CODE' },
              },
            },
          ],
        },
        prompt: 'DO_NOT_LEAK_PROMPT',
        token: 'DO_NOT_LEAK_TOKEN',
        selected_text: 'DO_NOT_LEAK_SELECTION',
      },
    },
  });

  assert.match(html, /data-why-diagnostics="inventory"/);
  assert.match(html, /QA User · Teammitglied/);
  assert.match(html, /private · v0\.9\.0/);
  assert.match(html, /App ändern/);
  assert.match(html, /Nicht erlaubt/);
  assert.match(html, /1 Datenbereich\(e\): 1 lesbar, 0 schreibbar/);
  assert.match(html, /Inventory Items/);
  assert.match(html, /Lesen Erlaubt/);
  assert.match(html, /Schreiben Nicht erlaubt/);
  assert.doesNotMatch(html, /policy_decision/);
  assert.doesNotMatch(html, /collection_decision/);
  assert.doesNotMatch(html, /module_decision/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /role_or_scope_denied/);
  assert.doesNotMatch(html, /Allowed\./);
  assert.doesNotMatch(html, /This role is not allowed/);
  assert.doesNotMatch(html, /DO_NOT_LEAK/);
});

test('settings module tab renders read-only agent grant boundary', () => {
  const html = baseTemplate({
    tab: 'admin',
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {},
      permission_model: {
        explicit_grants: [
          {
            grant_id: 'grant-agent-inventory-view',
            subject_type: 'mcp_actor',
            subject_id: 'chatgpt:agent',
            permission: 'apps.view',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            grant_id: 'grant-agent-customer-read',
            subject_type: 'mcp_actor',
            subject_id: 'chatgpt:agent',
            permission: 'data.read',
            scope_type: 'collection',
            scope_id: 'customer_accounts',
            active: true,
          },
        ],
      },
    },
  });

  assert.match(html, /data-agent-grant-boundary/);
  assert.match(html, /Agent- und App-Zugriff/);
  assert.match(html, /2 Sonderfreigaben/);
  assert.match(html, /Änderungen laufen über Owner\/Admin-Policy/);
  assert.match(html, /Agent/);
  assert.match(html, /chatgpt:agent/);
  assert.match(html, /App sehen/);
  assert.match(html, /App Inventory/);
  assert.match(html, /Daten lesen/);
  assert.match(html, /Datenbereich Customer Accounts \(customer_accounts\)/);
  assert.doesNotMatch(html, /grant-agent-inventory-view/);
  assert.doesNotMatch(html, /data-agent-grant-save/);
});

test('settings module tab renders projected release and data access facts', () => {
  const html = baseTemplate({
    tab: 'admin',
    managedModules: [{
      id: 'inventory',
      title: 'Inventory',
      description: '',
      entry: 'installed-modules/inventory/index.html',
      install_scope: 'installed',
      version: '1.2.0',
      lifecycle: {
        runtime_installed: true,
        release_status: 'released',
        release_state: {
          status: 'released',
          current: {
            version_id: 'version-current',
            version: 4,
            target_version: '1.2.0',
          },
          history_count: 4,
        },
        rollback_target: {
          version_id: 'version-prev',
          version: 3,
          target_version: '1.1.0',
        },
        data_access: {
          status: 'reviewed',
          completed: true,
          granted_collection_ids: ['inventory_items'],
          locked_collection_ids: ['supplier_prices'],
          review_is_evidence_only: true,
          grants_implied: false,
          areas: [
            { collection: 'inventory_items', decision: 'granted' },
            { collection: 'supplier_prices', decision: 'locked' },
          ],
        },
      },
    }],
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {
        inventory: [{ version_id: 'version-prev', version: 3, status: 'rolled_back' }],
      },
    },
  });

  assert.match(html, /data-module-release-projection="inventory"/);
  assert.match(html, /<select disabled aria-label="Rollback-Versionen nur Diagnose">/);
  assert.match(html, /Rollback nur Diagnose/);
  assert.doesNotMatch(html, /data-rollback-version=/);
  assert.doesNotMatch(html, /data-module-rollback=/);
  assert.match(html, /Freigabe:<\/b> Aktuell v1\.2\.0/);
  assert.match(html, /Rollback:<\/b> Rollback-Ziel v1\.1\.0/);
  assert.match(html, /Datenzugriff:<\/b> Freigegeben: Inventory Items \(inventory_items\); Gesperrt: Supplier Prices \(supplier_prices\)/);
  assert.match(html, /Review:<\/b> Review ist Nachweis; Datenrechte bleiben explizit\./);
  assert.doesNotMatch(html, /granted_collection_ids/);
  assert.doesNotMatch(html, /locked_collection_ids/);
});

test('settings module support diagnostics renders support-safe artifact summary without raw keys', () => {
  const html = baseTemplate({
    tab: 'admin',
    moduleSupportDiagnostics: {
      inventory: {
        ok: true,
        kind: 'business_os_support_diagnostics_artifact',
        schema_version: 1,
        artifact_schema: 'ctox.business_os.support_diagnostics.v1',
        generated_at_ms: 1781712006000,
        redaction: {
          profile: 'support-safe-v1',
          excluded_fields: [
            'prompt',
            'selected_text',
            'message_body',
            'record_payload',
            'payload_json',
            'token',
            'secret',
          ],
        },
        actor: {
          id: 'ops-admin',
          display_name: 'Ops Admin',
          role: 'admin',
        },
        scope: {
          module_id: 'inventory',
        },
        diagnostics: {
          why: {
            kind: 'business_os_why_diagnostics_summary',
            module: {
              id: 'inventory',
              title: 'Inventory',
              version: '1.2.0',
            },
            lifecycle: {
              visibility_state: 'team',
              current_semver: '1.2.0',
            },
            decisions: {
              modify: {
                allowed: false,
                permission: 'apps.modify',
                reason_code: 'role_or_scope_denied',
                display_reason: 'DO_NOT_LEAK_REASON',
              },
              release: {
                allowed: true,
                permission: 'apps.release',
                reason_code: 'role_default',
                display_reason: 'DO_NOT_LEAK_RELEASE_REASON',
              },
            },
            data_access: {
              areas: [
                {
                  collection: 'inventory_items',
                  read_decision: { allowed: true, permission: 'data.read' },
                  write_decision: { allowed: false, permission: 'data.write' },
                },
              ],
            },
          },
        },
        activity: {
          count: 2,
          events: [
            {
              policy_decision: {
                permission: 'apps.modify',
                reason_code: 'DO_NOT_LEAK_ACTIVITY_REASON',
              },
              client_scope: {
                source: 'DO_NOT_LEAK_SOURCE',
              },
            },
          ],
        },
        prompt: 'DO_NOT_LEAK_PROMPT',
        selected_text: 'DO_NOT_LEAK_SELECTION',
        token: 'DO_NOT_LEAK_TOKEN',
        secret: 'DO_NOT_LEAK_SECRET',
        payload_json: { marker: 'DO_NOT_LEAK_PAYLOAD' },
      },
    },
  });

  assert.match(html, /data-support-diagnostics="inventory"/);
  assert.match(html, /data-support-schema="ctox\.business_os\.support_diagnostics\.v1"/);
  assert.match(html, /data-redaction-profile="support-safe-v1"/);
  assert.match(html, /CTOX Support-Diagnose/);
  assert.match(html, /Support-sicher/);
  assert.match(html, /Keine Nachrichteninhalte, KI-Eingaben, Datensatzinhalte oder Zugangswerte enthalten/);
  assert.match(html, /Inventory/);
  assert.match(html, /Team sichtbar · v1\.2\.0/);
  assert.match(html, /2 Ereignisse zusammengefasst/);
  assert.match(html, /Ändern gesperrt · Freigabe erlaubt/);
  assert.match(html, /1 Datenbereich\(e\): 1 lesbar, 0 schreibbar/);
  assert.doesNotMatch(html, /policy_decision/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /apps\.release/);
  assert.doesNotMatch(html, /payload_json/);
  assert.doesNotMatch(html, /record_payload/);
  assert.doesNotMatch(html, /selected_text/);
  assert.doesNotMatch(html, /message_body/);
  assert.doesNotMatch(html, /\bprompt\b/i);
  assert.doesNotMatch(html, /\btoken\b/i);
  assert.doesNotMatch(html, /\bsecret\b/i);
  assert.doesNotMatch(html, /DO_NOT_LEAK/);
});

test('settings source does not expose an active stale release dispatch path', () => {
  assert.match(reactSettingsSource, /commandType:\s*['"]ctox\.business_os\.why['"]/);
  assert.match(reactSettingsSource, /commandType:\s*['"]ctox\.business_os\.support\.export_diagnostics['"]/);
  assert.match(reactSettingsSource, /data-module-why="\$\{escapeAttr\(mod\.id\)\}"/);
  assert.match(reactSettingsSource, /data-module-support-diagnostics="\$\{escapeAttr\(mod\.id\)\}"/);
  assert.doesNotMatch(reactSettingsSource, /commandType:\s*['"]ctox\.module\.release['"]/);
  assert.doesNotMatch(reactSettingsSource, /commandType:\s*['"]ctox\.module\.rollback['"]/);
  assert.doesNotMatch(reactSettingsSource, /data-module-release="\$\{/);
  assert.doesNotMatch(reactSettingsSource, /data-module-rollback="\$\{/);
  assert.doesNotMatch(reactSettingsSource, /data-rollback-version="\$\{/);
});

test('settings activity tab renders business-facing audit labels', () => {
  const html = baseTemplate({
    tab: 'activity',
    activity: {
      events: [
        {
          type: 'business_os.user.changed',
          observed_at_ms: 1781712000000,
          payload: {
            event_type: 'business_os.user.changed',
            actor: { display_name: 'Ops Admin' },
            previous: { id: 'member', display_name: 'Member', role: 'user', active: true },
            current: { id: 'member', display_name: 'Member', role: 'founder', active: true },
          },
        },
        {
          type: 'business_os.app_responsibility.changed',
          observed_at_ms: 1781712001000,
          payload: {
            event_type: 'business_os.app_responsibility.changed',
            actor: { display_name: 'Ops Admin' },
            module_id: 'inventory',
            user_id: 'module-owner',
            current: { module_id: 'inventory', user_id: 'module-owner', active: true },
          },
        },
        {
          type: 'business_os.policy.denied',
          observed_at_ms: 1781712002000,
          payload: {
            event_type: 'business_os.policy.denied',
            actor: { display_name: 'Viewer' },
            command_type: 'ctox.module.save',
            policy_decision: {
              permission: 'apps.modify',
              reason_code: 'role_or_scope_denied',
            },
          },
        },
        {
          type: 'business_os.policy.allowed',
          observed_at_ms: 1781712002500,
          payload: {
            event_type: 'business_os.policy.allowed',
            actor: { display_name: 'Ops Admin' },
            command_type: 'ctox.module.save',
            policy_decision: {
              permission: 'apps.modify',
              reason_code: 'allowed',
            },
          },
        },
        {
          type: 'business_os.external_approval.decided',
          observed_at_ms: 1781712003000,
          payload: {
            event_type: 'business_os.external_approval.decided',
            actor: { display_name: 'Ops Admin' },
            approval_id: 'approval-1',
            message_id: 'msg-1',
            decision: 'approved',
            message: {
              id: 'msg-1',
              approval_status: 'approved',
            },
          },
        },
        {
          type: 'business_os.module.release.succeeded',
          observed_at_ms: 1781712004000,
          payload: {
            event_type: 'business_os.module.release.succeeded',
            actor: { display_name: 'Release Owner' },
            summary: {
              module_id: 'inventory',
              target_version: '1.0.0',
              release_channel: 'team',
              data_access_review: {
                locked_collection_ids: ['secret_inventory_collection'],
              },
            },
          },
        },
        {
          type: 'business_os.module.rollback.succeeded',
          observed_at_ms: 1781712005000,
          payload: {
            event_type: 'business_os.module.rollback.succeeded',
            actor: { display_name: 'Release Owner' },
            summary: {
              module_id: 'inventory',
              version_id: 'version_baseline_secret_id',
              rollback_check: 'completed',
            },
          },
        },
      ],
      loading: false,
      loaded: true,
      error: '',
    },
  });
  assert.match(html, /Aktivität/);
  assert.match(html, /Teammitglied aktualisiert/);
  assert.match(html, /Member: Teammitglied -&gt; App-Verantwortliche:r/);
  assert.match(html, /App-Verantwortung aktualisiert/);
  assert.match(html, /module-owner ist verantwortlich für inventory/);
  assert.match(html, /Aktion blockiert/);
  assert.match(html, /Modul ändern wurde blockiert/);
  assert.match(html, /Aktion erlaubt/);
  assert.match(html, /Modul ändern wurde erlaubt/);
  assert.match(html, /Externe Freigabe entschieden/);
  assert.match(html, /msg-1: freigegeben/);
  assert.match(html, /App-Version veröffentlicht/);
  assert.match(html, /inventory: Version 1\.0\.0 wurde für Team veröffentlicht/);
  assert.match(html, /App-Rollback angewendet/);
  assert.match(html, /inventory: Rollback auf gewählte Version wurde angewendet/);
  assert.doesNotMatch(html, /business_os\.user\.changed/);
  assert.doesNotMatch(html, /business_os\.policy\.allowed/);
  assert.doesNotMatch(html, /business_os\.module\.release/);
  assert.doesNotMatch(html, /business_os\.module\.rollback/);
  assert.doesNotMatch(html, /business_os\.external_approval\.decided/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /role_or_scope_denied/);
  assert.doesNotMatch(html, /locked_collection_ids/);
  assert.doesNotMatch(html, /secret_inventory_collection/);
  assert.doesNotMatch(html, /version_baseline_secret_id/);
  assert.doesNotMatch(html, /rollback_check/);
});
