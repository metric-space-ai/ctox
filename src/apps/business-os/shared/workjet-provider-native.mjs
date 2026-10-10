// Origin: CTOX
// License: AGPL-3.0-only
// Provider metadata and model choices use the existing admitted RxDB command
// plane. This public result is never a credential or an inference permit.
const schema = 'ctox.provider-federation-registry.v1';
const timeoutMs = 25_000;
const actions = new Map([
  ['instance.providers.read', 'ctox.workjet.providers.list'],
  ['instance.providers.adopt', 'ctox.workjet.providers.adopt_native'],
  ['instance.providers.observe', 'ctox.workjet.providers.observe_native'],
  ['instance.providers.models.select', 'ctox.workjet.providers.models.select'],
  ['instance.providers.models.exclude', 'ctox.workjet.providers.models.exclude'],
  ['instance.providers.account.enable', 'ctox.workjet.providers.account.enable'],
  ['instance.providers.account.remove', 'ctox.workjet.providers.account.remove'],
]);

function invalid() { throw new TypeError('Invalid native provider metadata or request.'); }
function object(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid();
  return value;
}
function text(value, max = 256) {
  if (typeof value !== 'string' || !value || value.length > max
    || value.trim() !== value || /[\u0000-\u001f\u007f]/.test(value)) invalid();
  return value;
}
function integer(value, minimum = 0, maximum = Number.MAX_SAFE_INTEGER - 1) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) invalid();
  return value;
}
function boolean(value) { if (typeof value !== 'boolean') invalid(); return value; }
function models(value, maximum = 256) {
  if (!Array.isArray(value) || value.length > maximum) invalid();
  const result = value.map(value => text(value));
  if (new Set(result).size !== result.length) invalid();
  return result;
}
function nullableInteger(value, min = 0, max = Number.MAX_SAFE_INTEGER - 1) {
  return value === null ? null : integer(value, min, max);
}
function catalog(value) {
  object(value);
  let lastAttempt = null;
  if (value.lastAttempt !== null) {
    const attempt = object(value.lastAttempt);
    const failure = attempt.failure === null ? null : text(attempt.failure, 64);
    if (failure !== null && !/^[a-z][a-z0-9_]*$/.test(failure)) invalid();
    lastAttempt = {
      checkedAtMs: integer(attempt.checkedAtMs, 1),
      httpStatus: nullableInteger(attempt.httpStatus, 100, 599),
      elapsedMs: integer(attempt.elapsedMs),
      retryAfterSeconds: nullableInteger(attempt.retryAfterSeconds),
      failure, success: boolean(attempt.success),
    };
    if (lastAttempt.success && (lastAttempt.httpStatus !== 200 || failure !== null)) invalid();
  }
  const result = {
    observed: boolean(value.observed), fresh: boolean(value.fresh),
    models: models(value.models, 1024),
    lastSuccessAtMs: nullableInteger(value.lastSuccessAtMs, 1), lastAttempt,
  };
  if (result.fresh && (!result.observed || result.lastSuccessAtMs === null || !lastAttempt?.success)) invalid();
  return result;
}
export function projectNativeProviderRegistry(value) {
  object(value);
  if (value.ok !== true || value.schema !== schema || !Array.isArray(value.accounts)
    || value.accounts.length > 256 || !Array.isArray(value.providers) || value.providers.length > 256) invalid();
  const ids = new Set();
  const accounts = value.accounts.map(account => {
    object(account);
    const id = text(account.id);
    const holder = object(account.holder);
    if (holder.kind !== 'ctox_instance' || ids.has(id)) invalid();
    ids.add(id);
    const holderId = text(holder.id);
    const revision = integer(account.revision, 1);
    const reference = object(account.nativeAccountReference);
    if (reference.accountId !== id || reference.holderInstanceId !== holderId
      || reference.accountRevision !== revision) invalid();
    // Do not pass unknown fields, holder-local selectors or credentials through.
    return {
      id, holder: { kind: 'ctox_instance', id: holderId },
      provider: text(account.provider), enabled: boolean(account.enabled),
      credentialReady: boolean(account.credentialReady), revision,
      observedAtMs: integer(account.observedAtMs, 1),
      nativeAccountReference: { accountId: id, holderInstanceId: holderId, accountRevision: revision },
      ...(account.controls === undefined ? {} : { controls: {
        canEnable: boolean(object(account.controls).canEnable),
        canRemove: boolean(account.controls.canRemove),
      } }),
      modelCatalogObserved: boolean(account.modelCatalogObserved),
      modelCatalog: catalog(account.modelCatalog),
      excludedModels: models(account.excludedModels),
      effectiveModels: models(account.effectiveModels, 1024),
      // A GET /models result cannot attest a successful inference request.
      inferenceVerified: false,
      ...(account.modelChecks === undefined ? {} : { modelChecks: (() => {
        if (!Array.isArray(account.modelChecks) || account.modelChecks.length > 256) invalid();
        const checks = account.modelChecks.map(projectNativeModelProbe);
        if (new Set(checks.map(check => check.modelId)).size !== checks.length) invalid();
        return checks;
      })() }),
    };
  });
  const providersSeen = new Set();
  const providers = value.providers.map(provider => {
    object(provider);
    const name = text(provider.provider);
    if (providersSeen.has(name)) invalid();
    providersSeen.add(name);
    return { provider: name, selection: provider.selection === null ? null : models(provider.selection) };
  });
  if (accounts.some(account => !providersSeen.has(account.provider))) invalid();
  return { ok: true, schema, revision: integer(value.revision), accounts, providers };
}
function commandFor(request) {
  object(request);
  const commandType = actions.get(request.action);
  if (!commandType || request.version !== 1) invalid();
  const operationId = text(request.operationId, 128);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(operationId)) invalid();
  const allowed = new Set(['version', 'action', 'operationId']);
  const payload = {};
  if (request.action === 'instance.providers.observe' || request.action === 'instance.providers.models.exclude'
    || request.action.startsWith('instance.providers.account.')) {
    allowed.add('accountId'); allowed.add('expectedAccountRevision');
    payload.account_id = text(request.accountId);
    payload.expected_account_revision = integer(request.expectedAccountRevision, 1);
  }
  if (request.action.startsWith('instance.providers.account.')) {
    allowed.add('expectedRevision');
    payload.expected_revision = integer(request.expectedRevision);
    if (request.action === 'instance.providers.account.enable') {
      allowed.add('enabled'); payload.enabled = boolean(request.enabled);
    }
  }
  if (request.action === 'instance.providers.models.select') {
    allowed.add('provider'); payload.provider = text(request.provider);
  }
  if (request.action.startsWith('instance.providers.models.')) {
    allowed.add('models'); allowed.add('expectedRevision');
    payload.models = models(request.models);
    payload.expected_revision = integer(request.expectedRevision);
  }
  if (Object.keys(request).some(key => !allowed.has(key))) invalid();
  return { id: operationId, command_id: operationId, module: 'ctox',
    command_type: commandType, record_id: null, payload };
}
export async function requestNativeProviders(request, { dispatch, assertCurrent }) {
  const command = commandFor(request);
  if (typeof dispatch !== 'function' || typeof assertCurrent !== 'function') invalid();
  assertCurrent();
  let timer;
  try {
    const receipt = await Promise.race([
      dispatch(command, { until: 'terminal', sync_queue_tasks: false, timeoutMs }),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(Object.assign(new Error('Provider request timed out. Refresh to read its current state.'),
          { code: 'PROVIDER_CONTROL_TIMEOUT' })), timeoutMs);
      }),
    ]);
    assertCurrent();
    if (!receipt || receipt.command_id !== command.id || receipt.ok !== true
      || receipt.status !== 'completed' || receipt.transport !== 'rxdb-command-bus'
      || (receipt.target_record_id != null && receipt.target_record_id !== '')) {
      throw Object.assign(new Error('The native provider command did not complete. Check Owner/Admin access and the connection.'),
        { code: 'PROVIDER_CONTROL_UNCONFIRMED' });
    }
    const payload = object(receipt.payload);
    if (Object.keys(payload).some(key => key !== 'inbound_channel' && !Object.hasOwn(command.payload, key))
      || Object.entries(command.payload).some(([key, value]) => JSON.stringify(payload[key]) !== JSON.stringify(value))) invalid();
    return { version: 1, action: request.action, operationId: request.operationId,
      registry: projectNativeProviderRegistry(receipt.result) };
  } catch (error) {
    if (['PROVIDER_CONTROL_TIMEOUT', 'PROVIDER_CONTROL_UNCONFIRMED', 'PROVIDER_SCOPE_CHANGED'].includes(error?.code)) throw error;
    throw Object.assign(new Error('Native provider control is unavailable. Check the connection, Owner/Admin access and installed CTOX version.'),
      { code: 'PROVIDER_CONTROL_UNAVAILABLE' });
  } finally { clearTimeout(timer); }
}
export function projectNativeModelProbe(value) {
  object(value);
  const result = {
    modelId: text(value.modelId), checkedAtMs: integer(value.checkedAtMs, 1),
    elapsedMs: integer(value.elapsedMs), status: value.status, source: value.source,
    failure: value.failure === null ? null : text(value.failure, 64),
    httpStatus: nullableInteger(value.httpStatus, 100, 599),
    retryAtMs: nullableInteger(value.retryAtMs, 1),
  };
  const gatewayFailures = ['account_unavailable', 'authority_unavailable', 'gateway_cooldown',
    'gateway_state_unavailable', 'unverified_failure', 'transport', 'timeout', 'response_too_large'];
  if (result.status === 'ok') {
    if (result.source !== 'upstream' || result.failure !== null || result.retryAtMs !== null
      || result.httpStatus < 200 || result.httpStatus >= 300) invalid();
  } else if (result.status === 'unavailable') {
    if (result.source !== 'gateway' || !gatewayFailures.includes(result.failure)) invalid();
  } else if (result.status === 'failed') {
    const statuses = { auth: [401,403], model_not_found: [400,404], quota_rate_limit: [402,429] };
    const valid = result.failure === 'invalid_response'
      ? result.httpStatus >= 200 && result.httpStatus < 300
      : result.failure === 'provider'
        ? result.httpStatus >= 400 && result.httpStatus <= 599
        : statuses[result.failure]?.includes(result.httpStatus);
    if (result.source !== 'upstream' || !valid || result.retryAtMs !== null) invalid();
  } else invalid();
  return result;
}
export async function requestNativeModelCheck(sync, request, assertCurrent) {
  object(request);
  if (request.version !== 1 || request.action !== 'instance.providers.models.check'
    || Object.keys(request).some(key => !['version','action','operationId','accountId',
      'expectedAccountRevision','modelId'].includes(key))) invalid();
  const operationId = text(request.operationId, 128);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(operationId)) invalid();
  const accountId = text(request.accountId);
  const accountRevision = integer(request.expectedAccountRevision, 1);
  const modelId = text(request.modelId);
  if (typeof sync?.requestNative !== 'function') throw new Error('Model checks require a connected CTOX instance.');
  assertCurrent();
  let value;
  let timer;
  try {
    value = await Promise.race([
      sync.requestNative('ctox.workjet.models.check.v1', {
        version: 1, op: 'check', commandId: operationId, accountId, accountRevision, modelId,
      }, { requiredCapability: 'ctox-workjet-model-check-v1', timeoutMs: 25000 }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Timed out')), 25000); }),
    ]);
  } catch {
    throw Object.assign(new Error('Native model check unavailable. Check the connection, account and installed CTOX version.'),
      { code: 'PROVIDER_MODEL_CHECK_UNAVAILABLE' });
  } finally { clearTimeout(timer); }
  assertCurrent();
  if (value?.version !== 1 || value.op !== 'check' || value.commandId !== operationId
    || value.accountId !== accountId || value.accountRevision !== accountRevision
    || value.probe?.modelId !== modelId) invalid();
  return { version: 1, action: request.action, operationId, accountId, accountRevision,
    probe: projectNativeModelProbe(value.probe) };
}
