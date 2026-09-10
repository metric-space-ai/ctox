export const LAUNCH_CONTEXT_DEADLINE_MS = 30_000;
export const SHELL_GENERATION_PROBE_DEADLINE_MS = 5_000;

export function startupDeadlineError(message) {
  const error = new Error(message);
  error.name = 'StartupDeadlineError';
  error.code = 'CTOX_STARTUP_NETWORK_TIMEOUT';
  return error;
}

export function isStartupDeadlineError(error) {
  return error?.code === 'CTOX_STARTUP_NETWORK_TIMEOUT' || error?.name === 'StartupDeadlineError';
}

export function withStartupDeadline(operation, timeoutMs, message, options = {}) {
  if (typeof operation !== 'function') throw new TypeError('operation must be a function');
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new RangeError('timeoutMs must be positive');
  if (typeof message !== 'string' || !message) throw new TypeError('message must be a non-empty string');

  const scheduleTimeout = typeof options.setTimeout === 'function' ? options.setTimeout : (callback, delay) => setTimeout(callback, delay);
  const cancelTimeout = typeof options.clearTimeout === 'function' ? options.clearTimeout : (handle) => clearTimeout(handle);
  const controller = new AbortController();
  let timer;

  const deadline = new Promise((_, reject) => {
    timer = scheduleTimeout(() => {
      controller.abort();
      reject(startupDeadlineError(message));
    }, timeoutMs);
  });
  const task = Promise.resolve().then(() => operation(controller.signal));
  task.catch(() => {});

  return Promise.race([task, deadline]).finally(() => cancelTimeout(timer));
}

export function shouldPropagateGenerationProbeError(error, reloadScheduled) {
  return reloadScheduled || isStartupDeadlineError(error);
}

export async function cancelStartupResponseBody(response) {
  try {
    await response?.body?.cancel?.();
  } catch {
    // A failed cleanup must not replace the startup outcome.
  }
}
