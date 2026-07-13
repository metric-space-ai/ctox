export const OFFICE_RPC_PROTOCOL = 'ctox-office-message-channel-v1';

export class OfficeRpcError extends Error {
  constructor(message, code = 'rpc_error', details = null) {
    super(message);
    this.name = 'OfficeRpcError';
    this.code = code;
    this.details = details;
  }
}

export class OfficeRpcPeer {
  #port;
  #handlers = new Map();
  #pending = new Map();
  #listeners = new Map();
  #nextId = 1;
  #closed = false;

  constructor(port, handlers = {}) {
    if (!port?.postMessage) throw new TypeError('OfficeRpcPeer requires a MessagePort');
    this.#port = port;
    for (const [method, handler] of Object.entries(handlers)) this.handle(method, handler);
    this.#port.addEventListener?.('message', (event) => this.#receive(event.data));
    if (!this.#port.addEventListener) this.#port.onmessage = (event) => this.#receive(event.data);
    this.#port.start?.();
  }

  handle(method, handler) {
    if (typeof handler !== 'function') throw new TypeError(`RPC handler for ${method} must be a function`);
    this.#handlers.set(String(method), handler);
    return () => this.#handlers.delete(String(method));
  }

  on(eventName, listener) {
    const name = String(eventName);
    const listeners = this.#listeners.get(name) || new Set();
    listeners.add(listener);
    this.#listeners.set(name, listeners);
    return () => listeners.delete(listener);
  }

  emit(eventName, detail, transfer = []) {
    this.#assertOpen();
    this.#port.postMessage({ protocol: OFFICE_RPC_PROTOCOL, type: 'event', event: String(eventName), detail }, transfer);
  }

  call(method, params = null, options = {}) {
    this.#assertOpen();
    const id = `${Date.now().toString(36)}_${this.#nextId++}`;
    const timeoutMs = Number.isFinite(Number(options.timeoutMs)) ? Math.max(1, Number(options.timeoutMs)) : 30000;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#pending.delete(id);
        reject(new OfficeRpcError(`Office RPC timed out: ${method}`, 'rpc_timeout', { method, timeout_ms: timeoutMs }));
      }, timeoutMs);
      timeout?.unref?.();
      this.#pending.set(id, { resolve, reject, timeout });
      this.#port.postMessage({ protocol: OFFICE_RPC_PROTOCOL, type: 'request', id, method: String(method), params }, options.transfer || []);
    });
  }

  close(reason = 'Office RPC peer closed') {
    if (this.#closed) return;
    this.#closed = true;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(new OfficeRpcError(reason, 'rpc_closed'));
    }
    this.#pending.clear();
    this.#listeners.clear();
    this.#port.close?.();
  }

  #assertOpen() {
    if (this.#closed) throw new OfficeRpcError('Office RPC peer is closed', 'rpc_closed');
  }

  async #receive(message) {
    if (!message || message.protocol !== OFFICE_RPC_PROTOCOL) return;
    if (message.type === 'response') {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      clearTimeout(pending.timeout);
      if (message.ok) pending.resolve(message.value);
      else pending.reject(deserializeError(message.error));
      return;
    }
    if (message.type === 'event') {
      for (const listener of this.#listeners.get(message.event) || []) {
        try { listener(message.detail); } catch (error) { queueMicrotask(() => { throw error; }); }
      }
      return;
    }
    if (message.type !== 'request') return;
    const handler = this.#handlers.get(message.method);
    if (!handler) {
      this.#respond(message.id, false, null, new OfficeRpcError(`Unsupported Office RPC method: ${message.method}`, 'rpc_method_not_found'));
      return;
    }
    try {
      const result = await handler(message.params);
      if (result && typeof result === 'object' && Object.hasOwn(result, 'value') && Array.isArray(result.transfer)) {
        this.#respond(message.id, true, result.value, null, result.transfer);
      } else {
        this.#respond(message.id, true, result);
      }
    } catch (error) {
      this.#respond(message.id, false, null, error);
    }
  }

  #respond(id, ok, value = null, error = null, transfer = []) {
    if (this.#closed) return;
    this.#port.postMessage({
      protocol: OFFICE_RPC_PROTOCOL,
      type: 'response',
      id,
      ok,
      value,
      error: ok ? null : serializeError(error),
    }, transfer);
  }
}

function serializeError(error) {
  return {
    name: error?.name || 'Error',
    message: error?.message || String(error),
    code: error?.code || 'rpc_handler_failed',
    details: error?.details ?? null,
  };
}

function deserializeError(value = {}) {
  const error = new OfficeRpcError(value.message || 'Office RPC failed', value.code || 'rpc_handler_failed', value.details ?? null);
  error.name = value.name || 'OfficeRpcError';
  return error;
}
