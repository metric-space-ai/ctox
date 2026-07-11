// Declarative Business OS app-action SDK. All effects travel through the
// replicated business_commands collection; this module never calls a data API.

export function createAppActions({ module, commandBus, hasCapability = null, ensureRuntimeReady = null } = {}) {
  const moduleId = clean(module?.id);
  if (!moduleId) throw typedError('app_action_not_registered', 'Module id is required.');
  return Object.freeze({
    async run(name, input = {}, options = {}) {
      const action = clean(name);
      if (!action) throw typedError('app_action_not_registered', 'Action name is required.');
      if (typeof hasCapability === 'function' && hasCapability('ctox-app-runtime-v1') === false) {
        throw typedError('app_action_not_registered', 'The connected CTOX peer does not support app runtime v1.');
      }
      if (!commandBus?.dispatch) {
        throw typedError('app_runtime_reconfiguring', 'The Business OS command runtime is not ready.', true);
      }
      if (typeof ensureRuntimeReady === 'function') {
        try {
          await ensureRuntimeReady();
        } catch (cause) {
          const error = typedError(
            'app_runtime_reconfiguring',
            `The app data runtime is still reconfiguring: ${clean(cause?.message || cause)}`,
            true,
          );
          error.cause = cause;
          throw error;
        }
      }
      const idempotencyKey = clean(options.idempotencyKey || options.idempotency_key);
      const commandId = clean(options.commandId || options.command_id)
        || (idempotencyKey
          ? await deterministicCommandId(moduleId, action, idempotencyKey)
          : `cmd_app_${crypto.randomUUID()}`);
      const result = await commandBus.dispatch({
        id: commandId,
        command_id: commandId,
        module: moduleId,
        command_type: 'ctox.app.action.run',
        record_id: clean(options.recordId || options.record_id) || `${moduleId}:${action}`,
        payload: {
          module_id: moduleId,
          action,
          action_version: positiveInteger(options.version || options.actionVersion) || 1,
          input: cloneJson(input),
          ...(idempotencyKey ? { idempotency_key: idempotencyKey } : {}),
        },
        client_context: {
          action: 'app-action-run',
          module: moduleId,
          module_id: moduleId,
          app_id: moduleId,
          source_module: moduleId,
        },
      }, { until: options.until || 'accepted', timeoutMs: options.timeoutMs });
      throwForActionFailure(result, commandId);
      return result;
    },
    async getStatus(commandId) {
      if (!commandBus?.getStatus) {
        throw typedError('app_runtime_reconfiguring', 'Action status is not available.', true);
      }
      return commandBus.getStatus(clean(commandId));
    },
    subscribe(commandId, listener) {
      if (!commandBus?.subscribe) {
        throw typedError('app_runtime_reconfiguring', 'Action subscriptions are not available.', true);
      }
      if (typeof listener !== 'function' && typeof listener?.next !== 'function') {
        throw new TypeError('Action listener must be a function or observer.');
      }
      const subscription = commandBus.subscribe(clean(commandId), listener);
      const unsubscribe = () => subscription?.unsubscribe?.();
      unsubscribe.ready = subscription?.ready || Promise.resolve(subscription);
      return unsubscribe;
    },
  });
}

async function deterministicCommandId(moduleId, action, key) {
  const bytes = new TextEncoder().encode(`${moduleId}\0${action}\0${key}`);
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const hex = [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, '0')).join('');
  return `cmd_app_${hex}`;
}

function typedError(code, message, retryable = false) {
  const error = new Error(message);
  error.name = 'CtoxAppActionError';
  error.code = code;
  error.retryable = retryable;
  return error;
}

function throwForActionFailure(result, commandId) {
  const status = clean(result?.status || result?.terminal_status);
  const code = clean(result?.error_code || result?.result?.error_code);
  if (!code && !['failed', 'denied', 'cancelled'].includes(status)) return;
  const error = typedError(
    code || 'app_action_input_invalid',
    clean(result?.error_message || result?.error || result?.result?.error)
      || `App action ${commandId} failed.`,
    result?.retryable === true,
  );
  error.commandId = commandId;
  error.status = status || 'failed';
  error.result = result;
  throw error;
}

function clean(value) {
  return String(value || '').trim();
}

function positiveInteger(value) {
  const number = Number(value);
  return Number.isSafeInteger(number) && number > 0 ? number : 0;
}

function cloneJson(value) {
  if (value === undefined) return {};
  try {
    return structuredClone(value);
  } catch {
    return JSON.parse(JSON.stringify(value));
  }
}
