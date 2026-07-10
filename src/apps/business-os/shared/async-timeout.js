export class OperationTimeoutError extends Error {
  constructor(message, { code = 'deadline_exceeded', timeoutMs = 0 } = {}) {
    super(message);
    this.name = 'OperationTimeoutError';
    this.code = code;
    this.timeout_ms = timeoutMs;
    this.retryable = true;
  }
}

export async function withTimeout(operation, timeoutMs, {
  code = 'deadline_exceeded',
  message = `Operation exceeded ${timeoutMs}ms`,
  onTimeout = null,
} = {}) {
  const duration = Number(timeoutMs);
  if (!Number.isFinite(duration) || duration <= 0) {
    return await Promise.resolve(typeof operation === 'function' ? operation() : operation);
  }

  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      try {
        onTimeout?.();
      } finally {
        reject(new OperationTimeoutError(message, { code, timeoutMs: duration }));
      }
    }, duration);
  });

  try {
    const value = typeof operation === 'function' ? operation() : operation;
    return await Promise.race([Promise.resolve(value), timeout]);
  } finally {
    if (timer !== null) clearTimeout(timer);
  }
}
