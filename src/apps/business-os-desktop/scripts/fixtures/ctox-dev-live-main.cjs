"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { app, BrowserWindow, session } = require("electron");
const { SourceManager } = require("../../src/main/source-manager.cjs");
const {
  buildCtoxDevManagedInstanceUrl,
  clearCtoxDevSession,
  openCtoxDevLoginWindow,
} = require("../../src/main/ctox-dev-login.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];
const options = parseArgs(process.argv.slice(4));

if (!outputPath || !userDataPath) {
  throw new Error("usage: electron ctox-dev-live-main.cjs <outputPath> <userDataPath> --email <email>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");
app.on("window-all-closed", () => undefined);

app.whenReady().then(async () => {
  let exitCode = 0;
  const password = await readPassword();
  try {
    if (options.authWindow) {
      await clearCtoxDevSession(session.defaultSession, options.baseUrl);
    }
    const login = options.authWindow
      ? await loginWithAuthWindow(options.baseUrl, options.email, password)
      : await loginWithPasswordApi(options.baseUrl, options.email, password);

    const rawSessionPackage = await fetchRawSessionPackage(options.baseUrl);
    const registry = {
      settings: {
        ctoxDevBaseUrl: options.baseUrl,
        shellUrl: `${options.baseUrl.replace(/\/+$/, "")}/business-os/`,
      },
      instances: [],
      usage: {},
    };
    const sourceManager = new SourceManager({
      registryProvider: () => registry,
      registrySaver: () => undefined,
      secretStore: new MemorySecretStore(),
      ctoxDevBaseUrl: options.baseUrl,
      shellUrl: registry.settings.shellUrl,
      fetchImpl: session.defaultSession.fetch.bind(session.defaultSession),
    });
    const instances = await sourceManager.listInstances();
    const managedInstances = instances.filter((instance) => instance.source === "ctox_dev");
    const expectedTenantsPresent = options.expectedTenants.every((expected) => {
      const normalized = expected.toLowerCase();
      return managedInstances.some((instance) => [
        instance.displayName,
        instance.domain,
        instance.tenantId,
        instance.instanceId,
      ].filter(Boolean).some((value) => String(value).toLowerCase().includes(normalized)));
    });
    if (options.expectedTenants.length > 0 && !expectedTenantsPresent) {
      throw new Error(`expected tenants missing: ${options.expectedTenants.join(", ")}`);
    }

    const selectedForOptionalFlows = (options.launchFirst || options.manageFirst)
      ? selectLaunchInstance(managedInstances, options.expectedTenants)
      : null;

    let management = null;
    if (options.manageFirst) {
      if (!selectedForOptionalFlows) throw new Error("no ctox.dev managed instances available for management smoke");
      management = await inspectManagedDashboard(options.baseUrl, selectedForOptionalFlows);
      if (management.httpStatus !== 200) {
        throw new Error(`ctox.dev dashboard management link failed: ${management.httpStatus}`);
      }
      if (management.redirectedToLogin) {
        throw new Error("ctox.dev dashboard management link redirected to login");
      }
      if (!management.tenantHintPresent) {
        throw new Error("ctox.dev dashboard management link did not expose the selected tenant");
      }
    }

    let launch = null;
    if (options.launchFirst) {
      if (managedInstances.length === 0) throw new Error("no ctox.dev managed instances available for launch smoke");
      const launchConfig = await sourceManager.getLaunchConfig(selectedForOptionalFlows);
      if (launchConfig.ctoxConfig?.transport !== "webrtc") throw new Error("launch config transport is not webrtc");
      if (launchConfig.ctoxConfig?.http_bridge_available !== false) {
        throw new Error("launch config http_bridge_available is not false");
      }
      launch = {
        source: launchConfig.source,
        tenantId: selectedForOptionalFlows.tenantId,
        displayName: selectedForOptionalFlows.displayName,
        launchUrlOrigin: safeOrigin(launchConfig.launchUrl),
        transport: launchConfig.ctoxConfig.transport,
        httpBridgeAvailable: launchConfig.ctoxConfig.http_bridge_available,
        signalingUrlCount: Array.isArray(launchConfig.ctoxConfig.signaling_urls)
          ? launchConfig.ctoxConfig.signaling_urls.length
          : 0,
        hasRoomPassword: Boolean(launchConfig.ctoxConfig.signaling_room_password),
        expiresAt: launchConfig.expiresAt || "",
      };
    }

    const logout = await clearCtoxDevSession(session.defaultSession, options.baseUrl);
    const result = {
      ok: login.ok === true && login.completed === true,
      baseUrl: options.baseUrl,
      login,
      sessionPackage: {
        ok: rawSessionPackage.ok === true,
        desktopProtocol: rawSessionPackage.desktopProtocol || "",
        accountAuthenticated: rawSessionPackage.account?.authenticated === true,
        tenantCount: Array.isArray(rawSessionPackage.account?.tenants)
          ? rawSessionPackage.account.tenants.length
          : 0,
      },
      managedInstanceNames: managedInstances.map((instance) => instance.displayName),
      managedInstanceCount: managedInstances.length,
      expectedTenants: options.expectedTenants,
      expectedTenantsPresent,
      management,
      launch,
      logout,
    };
    writeResult(result);
    exitCode = result.ok ? 0 : 2;
  } catch (error) {
    exitCode = 1;
    writeResult({
      ok: false,
      baseUrl: options.baseUrl,
      error: error instanceof Error ? error.message : String(error),
    });
  } finally {
    app.exit(exitCode);
  }
});

function parseArgs(args) {
  const parsed = {
    baseUrl: "https://ctox.dev",
    email: "",
    expectedTenants: [],
    launchFirst: false,
    manageFirst: false,
    authWindow: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--base-url") {
      parsed.baseUrl = String(args[index + 1] || "").trim().replace(/\/+$/, "");
      index += 1;
    } else if (arg === "--email") {
      parsed.email = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--expected-tenant") {
      parsed.expectedTenants.push(String(args[index + 1] || "").trim());
      index += 1;
    } else if (arg === "--launch-first") {
      parsed.launchFirst = true;
    } else if (arg === "--manage-first") {
      parsed.manageFirst = true;
    } else if (arg === "--auth-window") {
      parsed.authWindow = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!parsed.email) throw new Error("--email is required");
  parsed.expectedTenants = parsed.expectedTenants.filter(Boolean);
  return parsed;
}

async function loginWithAuthWindow(baseUrl, email, password) {
  const automation = {
    attempts: 0,
    submitted: false,
    lastUrl: "",
    lastTitle: "",
    lastInputTypes: [],
    lastButtonLabels: [],
  };
  const login = await openCtoxDevLoginWindow({
    BrowserWindow,
    baseUrl,
    isAuthenticated: () => isCtoxDevSessionAuthenticated(baseUrl),
    show: false,
    timeoutMs: 60000,
    onWindowCreated: (loginWindow) => automateAuthWindowLogin(loginWindow, {
      email,
      password,
      evidence: automation,
    }),
  });
  return {
    ...login,
    method: "browser-window-auth-panel",
    automation,
  };
}

function automateAuthWindowLogin(loginWindow, { email, password, evidence }) {
  const interval = setInterval(async () => {
    if (loginWindow.isDestroyed()) {
      clearInterval(interval);
      return;
    }
    try {
      const result = await loginWindow.webContents.executeJavaScript(authWindowAutomationScript(email, password), true);
      evidence.attempts += 1;
      evidence.submitted = evidence.submitted || result.submitted === true;
      evidence.lastUrl = redactKnown(result.url, [email]);
      evidence.lastTitle = redactKnown(result.title, [email]);
      evidence.lastInputTypes = Array.isArray(result.inputTypes) ? result.inputTypes.slice(0, 8) : [];
      evidence.lastButtonLabels = Array.isArray(result.buttonLabels)
        ? result.buttonLabels.map((label) => redactKnown(label, [email])).slice(0, 8)
        : [];
      if (result.submitted === true) clearInterval(interval);
    } catch (_error) {
      evidence.attempts += 1;
    }
  }, 750);
  loginWindow.on("closed", () => clearInterval(interval));
}

function authWindowAutomationScript(email, password) {
  return `(() => {
    const email = ${JSON.stringify(email)};
    const password = ${JSON.stringify(password)};
    const visible = (element) => {
      if (!element || !element.getBoundingClientRect) return false;
      const rect = element.getBoundingClientRect();
      const style = window.getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
    };
    const descriptorText = (element) => [
      element.type,
      element.name,
      element.id,
      element.placeholder,
      element.getAttribute("aria-label"),
      element.autocomplete
    ].filter(Boolean).join(" ");
    const setValue = (element, value) => {
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(element, value);
      element.dispatchEvent(new Event("input", { bubbles: true }));
      element.dispatchEvent(new Event("change", { bubbles: true }));
    };
    const inputs = Array.from(document.querySelectorAll("input")).filter(visible);
    const buttons = Array.from(document.querySelectorAll("button,[role='button'],input[type='submit']")).filter(visible);
    const emailInput = inputs.find((input) => /email|e-mail|mail/i.test(descriptorText(input)))
      || inputs.find((input) => /text|email|search|tel|url|^$/i.test(input.type || ""));
    const passwordInput = inputs.find((input) => String(input.type || "").toLowerCase() === "password");
    if (!passwordInput) {
      const passwordMode = buttons.find((button) => /password|passwort|email|e-mail|mail|login|sign in|anmelden/i.test(button.innerText || button.value || ""));
      if (passwordMode) passwordMode.click();
    }
    const nextPasswordInput = passwordInput
      || Array.from(document.querySelectorAll("input[type='password']")).find(visible);
    if (!emailInput || !nextPasswordInput) {
      return {
        submitted: false,
        url: location.href,
        title: document.title || "",
        inputTypes: inputs.map((input) => descriptorText(input)).slice(0, 8),
        buttonLabels: buttons.map((button) => (button.innerText || button.value || "").trim()).filter(Boolean).slice(0, 8)
      };
    }
    setValue(emailInput, email);
    setValue(nextPasswordInput, password);
    const submit = buttons.find((button) => /login|sign in|continue|weiter|anmelden|einloggen/i.test(button.innerText || button.value || ""))
      || nextPasswordInput.closest("form")?.querySelector("button[type='submit'],input[type='submit']")
      || buttons[0];
    if (submit) {
      submit.click();
    } else if (nextPasswordInput.form?.requestSubmit) {
      nextPasswordInput.form.requestSubmit();
    } else if (nextPasswordInput.form) {
      nextPasswordInput.form.submit();
    }
    return {
      submitted: true,
      url: location.href,
      title: document.title || "",
      inputTypes: inputs.map((input) => descriptorText(input)).slice(0, 8),
      buttonLabels: buttons.map((button) => (button.innerText || button.value || "").trim()).filter(Boolean).slice(0, 8)
    };
  })()`;
}

function redactKnown(value, secrets) {
  let result = String(value || "");
  for (const secret of secrets) {
    if (secret) result = result.split(secret).join("<redacted>");
  }
  return result;
}

async function loginWithPasswordApi(baseUrl, email, password) {
  const response = await session.defaultSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/auth/password`, {
    method: "POST",
    cache: "no-store",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok || payload.ok !== true) {
    throw new Error(`ctox.dev password login failed: ${response.status}`);
  }
  return {
    ok: true,
    completed: true,
    method: "password-api-cookie-jar",
    userEmail: payload.user?.email || "",
  };
}

async function fetchRawSessionPackage(baseUrl) {
  const response = await session.defaultSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/desktop/session-package`, {
    cache: "no-store",
    credentials: "include",
    headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
  });
  if (!response.ok) throw new Error(`ctox.dev live session-package failed: ${response.status}`);
  return response.json();
}

async function isCtoxDevSessionAuthenticated(baseUrl) {
  const response = await session.defaultSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/desktop/session-package`, {
    cache: "no-store",
    credentials: "include",
    headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
  });
  if (!response.ok) return false;
  const payload = await response.json().catch(() => ({}));
  return payload?.account?.authenticated === true || Array.isArray(payload?.account?.tenants);
}

async function inspectManagedDashboard(baseUrl, instance) {
  const manageUrl = buildCtoxDevManagedInstanceUrl(baseUrl, instance);
  const fetchResponse = await session.defaultSession.fetch(manageUrl, {
    cache: "no-store",
    credentials: "include",
  });
  const html = await fetchResponse.text().catch(() => "");
  const browser = await loadDashboardInBrowserWindow(manageUrl);
  const hints = [
    instance.displayName,
    instance.domain,
    instance.tenantId,
  ].filter(Boolean).map((value) => String(value).toLowerCase());
  const searchable = `${html}\n${browser.title}\n${browser.bodyText}`.toLowerCase();
  const finalPath = safePath(fetchResponse.url || manageUrl);
  return {
    ok: fetchResponse.status === 200,
    tenantId: instance.tenantId,
    displayName: instance.displayName,
    managePath: safePath(manageUrl),
    httpStatus: fetchResponse.status,
    finalOrigin: safeOrigin(fetchResponse.url || manageUrl),
    finalPath,
    redirectedToLogin: finalPath.includes("/login") || finalPath.includes("/auth"),
    tenantHintPresent: hints.some((hint) => hint && searchable.includes(hint)),
    browserTitle: browser.title,
    browserBodyTextLength: browser.bodyText.length,
  };
}

function loadDashboardInBrowserWindow(manageUrl) {
  return new Promise((resolve) => {
    const window = new BrowserWindow({
      show: false,
      width: 1280,
      height: 900,
      webPreferences: {
        contextIsolation: true,
        nodeIntegration: false,
      },
    });
    let settled = false;
    const timeout = setTimeout(() => finish(), 12000);
    function finish() {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      window.webContents.executeJavaScript(`({
        title: document.title || "",
        bodyText: (document.body && document.body.innerText || "").slice(0, 20000)
      })`, true).then((result) => {
        if (!window.isDestroyed()) window.destroy();
        resolve({
          title: String(result?.title || ""),
          bodyText: String(result?.bodyText || ""),
        });
      }).catch(() => {
        if (!window.isDestroyed()) window.destroy();
        resolve({ title: "", bodyText: "" });
      });
    }
    window.webContents.once("did-finish-load", () => {
      setTimeout(finish, 3500);
    });
    window.webContents.once("did-fail-load", () => {
      setTimeout(finish, 1000);
    });
    window.loadURL(manageUrl).catch(() => finish());
  });
}

function selectLaunchInstance(instances, expectedTenants) {
  if (expectedTenants.length === 0) return instances[0];
  for (const expected of expectedTenants) {
    const normalized = expected.toLowerCase();
    const match = instances.find((instance) => [
      instance.displayName,
      instance.domain,
      instance.tenantId,
      instance.instanceId,
    ].filter(Boolean).some((value) => String(value).toLowerCase().includes(normalized)));
    if (match) return match;
  }
  return instances[0];
}

function safeOrigin(rawUrl) {
  try {
    return new URL(String(rawUrl || "")).origin;
  } catch (_error) {
    return "";
  }
}

function safePath(rawUrl) {
  try {
    const url = new URL(String(rawUrl || ""));
    return `${url.pathname}${url.search}`;
  } catch (_error) {
    return "";
  }
}

function readPassword() {
  return new Promise((resolve, reject) => {
    let input = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      input += chunk;
    });
    process.stdin.on("end", () => {
      const password = input.replace(/\r?\n$/, "");
      if (!password) {
        reject(new Error("password stdin was empty"));
        return;
      }
      resolve(password);
    });
    process.stdin.on("error", reject);
  });
}

function writeResult(result) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
}

class MemorySecretStore {
  constructor() {
    this.values = new Map();
  }

  async set(ref, value) {
    this.values.set(ref, value);
  }

  async get(ref) {
    return this.values.get(ref) || "";
  }

  async delete(ref) {
    this.values.delete(ref);
  }
}
