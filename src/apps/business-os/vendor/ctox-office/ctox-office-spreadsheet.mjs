// src/apps/business-os/office-engine/src/rpc.mjs
var OFFICE_RPC_PROTOCOL = "ctox-office-message-channel-v1";
var OfficeRpcError = class extends Error {
  constructor(message, code = "rpc_error", details = null) {
    super(message);
    this.name = "OfficeRpcError";
    this.code = code;
    this.details = details;
  }
};
var OfficeRpcPeer = class {
  #port;
  #handlers = /* @__PURE__ */ new Map();
  #pending = /* @__PURE__ */ new Map();
  #listeners = /* @__PURE__ */ new Map();
  #nextId = 1;
  #closed = false;
  constructor(port, handlers = {}) {
    if (!port?.postMessage) throw new TypeError("OfficeRpcPeer requires a MessagePort");
    this.#port = port;
    for (const [method, handler] of Object.entries(handlers)) this.handle(method, handler);
    this.#port.addEventListener?.("message", (event) => this.#receive(event.data));
    if (!this.#port.addEventListener) this.#port.onmessage = (event) => this.#receive(event.data);
    this.#port.start?.();
  }
  handle(method, handler) {
    if (typeof handler !== "function") throw new TypeError(`RPC handler for ${method} must be a function`);
    this.#handlers.set(String(method), handler);
    return () => this.#handlers.delete(String(method));
  }
  on(eventName, listener) {
    const name = String(eventName);
    const listeners = this.#listeners.get(name) || /* @__PURE__ */ new Set();
    listeners.add(listener);
    this.#listeners.set(name, listeners);
    return () => listeners.delete(listener);
  }
  emit(eventName, detail, transfer = []) {
    this.#assertOpen();
    this.#port.postMessage({ protocol: OFFICE_RPC_PROTOCOL, type: "event", event: String(eventName), detail }, transfer);
  }
  call(method, params = null, options = {}) {
    this.#assertOpen();
    const id = `${Date.now().toString(36)}_${this.#nextId++}`;
    const timeoutMs = Number.isFinite(Number(options.timeoutMs)) ? Math.max(1, Number(options.timeoutMs)) : 3e4;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#pending.delete(id);
        reject(new OfficeRpcError(`Office RPC timed out: ${method}`, "rpc_timeout", { method, timeout_ms: timeoutMs }));
      }, timeoutMs);
      timeout?.unref?.();
      this.#pending.set(id, { resolve, reject, timeout });
      this.#port.postMessage({ protocol: OFFICE_RPC_PROTOCOL, type: "request", id, method: String(method), params }, options.transfer || []);
    });
  }
  close(reason = "Office RPC peer closed") {
    if (this.#closed) return;
    this.#closed = true;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(new OfficeRpcError(reason, "rpc_closed"));
    }
    this.#pending.clear();
    this.#listeners.clear();
    this.#port.close?.();
  }
  #assertOpen() {
    if (this.#closed) throw new OfficeRpcError("Office RPC peer is closed", "rpc_closed");
  }
  async #receive(message) {
    if (!message || message.protocol !== OFFICE_RPC_PROTOCOL) return;
    if (message.type === "response") {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      clearTimeout(pending.timeout);
      if (message.ok) pending.resolve(message.value);
      else pending.reject(deserializeError(message.error));
      return;
    }
    if (message.type === "event") {
      for (const listener of this.#listeners.get(message.event) || []) {
        try {
          listener(message.detail);
        } catch (error) {
          queueMicrotask(() => {
            throw error;
          });
        }
      }
      return;
    }
    if (message.type !== "request") return;
    const handler = this.#handlers.get(message.method);
    if (!handler) {
      this.#respond(message.id, false, null, new OfficeRpcError(`Unsupported Office RPC method: ${message.method}`, "rpc_method_not_found"));
      return;
    }
    try {
      const result = await handler(message.params);
      if (result && typeof result === "object" && Object.hasOwn(result, "value") && Array.isArray(result.transfer)) {
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
      type: "response",
      id,
      ok,
      value,
      error: ok ? null : serializeError(error)
    }, transfer);
  }
};
function serializeError(error) {
  return {
    name: error?.name || "Error",
    message: error?.message || String(error),
    code: error?.code || "rpc_handler_failed",
    details: error?.details ?? null
  };
}
function deserializeError(value = {}) {
  const error = new OfficeRpcError(value.message || "Office RPC failed", value.code || "rpc_handler_failed", value.details ?? null);
  error.name = value.name || "OfficeRpcError";
  return error;
}

// src/apps/business-os/office-engine/src/capsule.mjs
var VALID_KINDS = /* @__PURE__ */ new Set(["document", "spreadsheet"]);
async function createCtoxOfficeEditor(options = {}) {
  const kind = String(options.kind || "");
  if (!VALID_KINDS.has(kind)) throw new TypeError(`Unsupported CTOX editor kind: ${kind}`);
  const productName = kind === "document" ? "CTOX Documents" : "CTOX Spreadsheets";
  if (!(options.host instanceof Element)) throw new TypeError(`${productName} requires a host Element`);
  const bridge = validateBridge(options.bridge, productName);
  const frame = document.createElement("iframe");
  frame.className = `ctox-office-capsule ctox-office-capsule--${kind}`;
  frame.title = `${productName} Editor`;
  frame.dataset.ctoxOfficeKind = kind;
  frame.style.cssText = "display:block;width:100%;height:100%;border:0;background:transparent";
  frame.setAttribute("referrerpolicy", "no-referrer");
  frame.src = new URL(`./frame.html?kind=${encodeURIComponent(kind)}`, import.meta.url).href;
  options.host.replaceChildren(frame);
  await waitForFrameLoad(frame, options.loadTimeoutMs);
  const channel = new MessageChannel();
  const rpc = new OfficeRpcPeer(channel.port1, {
    "bridge.loadVersion": (request) => bridge.loadVersion(request),
    "bridge.prepare": (request) => bridge.prepare(request),
    "bridge.commit": (request) => bridge.commit(request),
    "bridge.export": (request) => bridge.export(request),
    "bridge.reportIntegrityError": (request) => bridge.reportIntegrityError?.(request)
  });
  const listeners = /* @__PURE__ */ new Map();
  const offEvent = rpc.on("editor.event", ({ name, detail } = {}) => {
    for (const listener of listeners.get(name) || []) listener(detail);
  });
  frame.contentWindow.postMessage({
    type: "ctox-office-connect",
    kind,
    productName,
    locale: options.locale === "en" ? "en" : "de",
    theme: options.theme || "system",
    permissions: normalizePermissions(options.permissions),
    launchArgs: sanitizeLaunchArgs(options.launchArgs)
  }, location.origin, [channel.port2]);
  try {
    await rpc.call("editor.ready", null, { timeoutMs: options.readyTimeoutMs || 3e4 });
  } catch (error) {
    offEvent();
    rpc.close();
    frame.remove();
    throw error;
  }
  let destroyed = false;
  const themeObserver = new MutationObserver(() => {
    if (!destroyed) rpc.call("editor.setTheme", currentShellTheme(options.theme)).catch(() => {
    });
  });
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  return Object.freeze({
    kind,
    open: (request) => rpc.call("editor.open", request),
    save: (request = {}) => rpc.call("editor.save", request),
    export: (request = {}) => rpc.call("editor.export", request),
    focus: () => rpc.call("editor.focus"),
    setPermissions: (permissions) => rpc.call("editor.setPermissions", normalizePermissions(permissions)),
    inspect: () => rpc.call("editor.inspect"),
    on(name, listener) {
      if (typeof listener !== "function") throw new TypeError("Office event listener must be a function");
      const set = listeners.get(name) || /* @__PURE__ */ new Set();
      set.add(listener);
      listeners.set(name, set);
      return () => set.delete(listener);
    },
    async destroy() {
      if (destroyed) return;
      destroyed = true;
      themeObserver.disconnect();
      try {
        await rpc.call("editor.destroy", null, { timeoutMs: 3e3 });
      } catch {
      }
      offEvent();
      rpc.close();
      listeners.clear();
      frame.remove();
    }
  });
}
function currentShellTheme(fallback = "system") {
  const shellTheme = document.documentElement.dataset.theme;
  if (shellTheme === "dark" || shellTheme === "light") return shellTheme;
  return fallback === "dark" || fallback === "light" ? fallback : "system";
}
function validateBridge(bridge, productName) {
  if (!bridge || typeof bridge !== "object") throw new TypeError(`${productName} requires a bridge`);
  for (const method of ["loadVersion", "prepare", "commit", "export"]) {
    if (typeof bridge[method] !== "function") throw new TypeError(`${productName} bridge is missing ${method}()`);
  }
  return bridge;
}
function normalizePermissions(permissions = {}) {
  return Object.freeze({
    read: permissions.read !== false,
    write: permissions.write !== false,
    export: permissions.export !== false,
    comment: permissions.comment !== false,
    review: permissions.review !== false
  });
}
function sanitizeLaunchArgs(args = {}) {
  return {
    runtimeModule: typeof args.runtimeModule === "string" ? args.runtimeModule : "",
    testMode: args.testMode === true,
    recordId: typeof args.recordId === "string" ? args.recordId : "",
    versionId: typeof args.versionId === "string" ? args.versionId : ""
  };
}
function waitForFrameLoad(frame, timeoutValue) {
  const timeoutMs = Number.isFinite(Number(timeoutValue)) ? Number(timeoutValue) : 15e3;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("CTOX product iframe load timed out")), timeoutMs);
    const cleanup = () => {
      clearTimeout(timeout);
      frame.removeEventListener("load", onLoad);
      frame.removeEventListener("error", onError);
    };
    const onLoad = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("CTOX product iframe failed to load"));
    };
    frame.addEventListener("load", onLoad, { once: true });
    frame.addEventListener("error", onError, { once: true });
  });
}

// src/apps/business-os/office-engine/src/spreadsheet.mjs
function createCtoxSpreadsheetsEditor(options = {}) {
  return createCtoxOfficeEditor({ ...options, kind: "spreadsheet" });
}
var createCtoxOfficeEditor2 = createCtoxSpreadsheetsEditor;
var CTOX_SPREADSHEETS_EDITOR_KIND = "spreadsheet";
var CTOX_SPREADSHEETS_PRODUCT_ID = "ctox-spreadsheets";
var CTOX_OFFICE_EDITOR_KIND = CTOX_SPREADSHEETS_EDITOR_KIND;
export {
  CTOX_OFFICE_EDITOR_KIND,
  CTOX_SPREADSHEETS_EDITOR_KIND,
  CTOX_SPREADSHEETS_PRODUCT_ID,
  createCtoxOfficeEditor2 as createCtoxOfficeEditor,
  createCtoxSpreadsheetsEditor
};
