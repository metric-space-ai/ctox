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
const { summarizeAccessRevocationBlock } = require("../ctox-dev-live-contract.cjs");

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
  const credentials = await readCredentials();
  const password = credentials.password;
  let progress = { ok: false, baseUrl: options.baseUrl, stage: "password-read" };
  function writeProgress(stage, extra = {}) {
    progress = { ...progress, ...extra, stage };
    writeResult(progress);
  }
  writeProgress("starting-login");
  try {
    if (options.authWindow) {
      await clearCtoxDevSession(session.defaultSession, options.baseUrl);
      writeProgress("initial-session-cleared");
    }
    const login = options.authWindow
      ? await loginWithAuthWindow(options.baseUrl, options.email, password)
      : await loginWithPasswordApi(options.baseUrl, options.email, password);
    writeProgress("logged-in", { login: summarizeLogin(login) });

    const rawSessionPackage = await fetchRawSessionPackage(options.baseUrl);
    writeProgress("session-package-loaded", {
      sessionPackage: summarizeSessionPackage(rawSessionPackage),
    });
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
    writeProgress("instances-loaded", { managedInstanceCount: managedInstances.length });
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

    const selectedForOptionalFlows = (options.launchFirst || options.manageFirst || options.sessionRotation)
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
      writeProgress("management-checked", { management });
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
        launchUrlPath: safePath(launchConfig.launchUrl),
        transport: launchConfig.ctoxConfig.transport,
        httpBridgeAvailable: launchConfig.ctoxConfig.http_bridge_available,
        signalingUrlCount: Array.isArray(launchConfig.ctoxConfig.signaling_urls)
          ? launchConfig.ctoxConfig.signaling_urls.length
          : 0,
        hasRoomPassword: Boolean(launchConfig.ctoxConfig.signaling_room_password),
        expiresAt: launchConfig.expiresAt || "",
      };
      if (options.renderLaunchFirst) {
        launch.render = await inspectRenderedLaunch(launchConfig, selectedForOptionalFlows);
        writeProgress("launch-render-checked", { launch });
        if (launch.render.loginPromptVisible) {
          throw new Error("rendered launch asked for a second login");
        }
        if (launch.render.systemStartFailed) {
          throw new Error(`rendered launch failed to start: ${launch.render.failureText || "unknown failure"}`);
        }
        if (!launch.render.ok) {
          throw new Error(`rendered launch did not become ready: ${launch.render.reason || "unknown reason"}`);
        }
      }
      writeProgress("launch-checked", { launch });
    }

    let sessionRotation = null;
    if (options.sessionRotation) {
      if (!selectedForOptionalFlows) throw new Error("no ctox.dev managed instances available for session rotation smoke");
      writeProgress("session-rotation-started");
      sessionRotation = await exerciseSessionRotation({
        baseUrl: options.baseUrl,
        email: options.email,
        password,
        sourceManager,
        selectedInstance: selectedForOptionalFlows,
        expectedTenants: options.expectedTenants,
        useAuthWindow: options.authWindow,
        writeProgress,
      });
    }

    let accessRevocation = null;
    if (options.accessRevocation) {
      writeProgress("access-revocation-started");
      accessRevocation = await exerciseAccessRevocation({
        baseUrl: options.baseUrl,
        adminSession: session.defaultSession,
        adminEmail: options.email,
        memberSession: session.fromPartition("persist:ctox-dev-access-revocation-member"),
        memberEmail: options.accessRevocationMemberEmail,
        memberPassword: credentials.memberPassword,
        tenantSelector: options.accessRevocationTenant,
        adminManagedInstances: managedInstances,
        writeProgress,
      });
    }

    const logout = sessionRotation?.finalLogout
      || await clearCtoxDevSession(session.defaultSession, options.baseUrl);
    writeProgress("final-logout", { logout });
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
      sessionRotation,
      accessRevocation,
      logout,
    };
    writeResult(result);
    exitCode = result.ok ? 0 : 2;
  } catch (error) {
    exitCode = 1;
    writeResult({
      ...progress,
      ok: false,
      baseUrl: options.baseUrl,
      error: error instanceof Error ? error.message : String(error),
    });
  } finally {
    app.exit(exitCode);
  }
});

function summarizeLogin(login) {
  return {
    ok: login?.ok === true,
    completed: login?.completed === true,
    method: login?.method || "",
    via: login?.via || "",
  };
}

function summarizeSessionPackage(rawSessionPackage) {
  return {
    ok: rawSessionPackage?.ok === true,
    desktopProtocol: rawSessionPackage?.desktopProtocol || "",
    accountAuthenticated: rawSessionPackage?.account?.authenticated === true,
    tenantCount: Array.isArray(rawSessionPackage?.account?.tenants)
      ? rawSessionPackage.account.tenants.length
      : 0,
  };
}

function parseArgs(args) {
  const parsed = {
    baseUrl: "https://ctox.dev",
    email: "",
    expectedTenants: [],
    launchFirst: false,
    renderLaunchFirst: false,
    manageFirst: false,
    authWindow: false,
    sessionRotation: false,
    accessRevocation: false,
    accessRevocationTenant: "",
    accessRevocationMemberEmail: "",
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
    } else if (arg === "--render-launch-first") {
      parsed.renderLaunchFirst = true;
      parsed.launchFirst = true;
    } else if (arg === "--manage-first") {
      parsed.manageFirst = true;
    } else if (arg === "--auth-window") {
      parsed.authWindow = true;
    } else if (arg === "--session-rotation") {
      parsed.sessionRotation = true;
    } else if (arg === "--access-revocation") {
      parsed.accessRevocation = true;
    } else if (arg === "--access-revocation-tenant") {
      parsed.accessRevocationTenant = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--access-revocation-member-email") {
      parsed.accessRevocationMemberEmail = String(args[index + 1] || "").trim();
      index += 1;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!parsed.email) throw new Error("--email is required");
  if (parsed.accessRevocation) {
    if (!parsed.accessRevocationTenant) {
      throw new Error("--access-revocation-tenant is required with --access-revocation");
    }
    if (!parsed.accessRevocationMemberEmail) {
      throw new Error("--access-revocation-member-email is required with --access-revocation");
    }
  }
  parsed.expectedTenants = parsed.expectedTenants.filter(Boolean);
  return parsed;
}

function inspectRenderedLaunch(launchConfig, instance) {
  return new Promise((resolve) => {
    const partition = instance.sessionPartition || `persist:ctox-render-${Date.now()}`;
    const browserSession = session.fromPartition(partition);
    const window = new BrowserWindow({
      show: false,
      width: 1440,
      height: 920,
      webPreferences: {
        contextIsolation: true,
        nodeIntegration: false,
        partition,
      },
    });
    const consoleMessages = [];
    const networkEvents = [];
    let failLoad = null;
    let settled = false;
    const timeout = setTimeout(() => finish("timeout"), 90000);
    let pollTimer = null;
    let lastPage = {};
    let lastFailureText = "";
    let lastLoginPromptVisible = false;
    const webRequest = browserSession.webRequest;
    if (webRequest?.onErrorOccurred) {
      webRequest.onErrorOccurred(
        { urls: ["http://*/*", "https://*/*", "ws://*/*", "wss://*/*"] },
        (details) => pushNetworkEvent(networkEvents, {
          type: "error",
          url: details.url,
          error: details.error,
        }),
      );
    }
    if (webRequest?.onCompleted) {
      webRequest.onCompleted(
        { urls: ["http://*/*", "https://*/*", "ws://*/*", "wss://*/*"] },
        (details) => {
          if (details.statusCode >= 400 || /^wss?:/i.test(String(details.url || ""))) {
            pushNetworkEvent(networkEvents, {
              type: "completed",
              url: details.url,
              statusCode: details.statusCode,
            });
          }
        },
      );
    }
    window.webContents.on("console-message", (_event, details) => {
      consoleMessages.push({
        level: details.level,
        message: String(details.message || "").slice(0, 240),
      });
    });
    window.webContents.once("did-fail-load", (_event, errorCode, errorDescription, validatedUrl) => {
      failLoad = {
        errorCode,
        errorDescription: String(errorDescription || ""),
        origin: safeOrigin(validatedUrl),
      };
      setTimeout(() => finish("did-fail-load"), 1000);
    });
    window.webContents.once("did-finish-load", () => {
      pollRenderedLaunch();
    });
    async function pollRenderedLaunch() {
      if (settled || window.isDestroyed()) return;
      const page = await captureRenderedLaunchPage(window).catch(() => ({}));
      const bodyText = String(page.bodyText || "");
      const failureText = extractLaunchFailureText(bodyText);
      const loginPromptVisible = /magic link|passwort|anmelden|einloggen|sign in|log in/i.test(bodyText);
      lastPage = page;
      lastFailureText = failureText;
      lastLoginPromptVisible = loginPromptVisible;
      if (failLoad) {
        finish("did-fail-load");
        return;
      }
      if (loginPromptVisible) {
        finish("login-prompt-visible");
        return;
      }
      if (failureText || page.startupErrorVisible) {
        finish("startup-error-visible");
        return;
      }
      if (page.decodedCtoxConfig?.redactedMarkerCount > 0) {
        finish("redacted-launch-config");
        return;
      }
      if (page.shellReady === true) {
        finish("shell-ready");
        return;
      }
      pollTimer = setTimeout(pollRenderedLaunch, 1000);
    }
    async function finish(reason) {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (pollTimer) clearTimeout(pollTimer);
      const page = Object.keys(lastPage || {}).length
        ? lastPage
        : await captureRenderedLaunchPage(window).catch(() => ({}));
      if (!window.isDestroyed()) window.destroy();
      const bodyText = String(page.bodyText || "");
      const failureText = lastFailureText || extractLaunchFailureText(bodyText);
      resolve({
        ok: !failLoad && page.shellReady === true && !lastLoginPromptVisible && !failureText && !page.startupErrorVisible,
        reason,
        finalOrigin: safeOrigin(page.url || launchConfig.launchUrl),
        finalPath: safePath(page.url || launchConfig.launchUrl),
        title: String(page.title || ""),
        bodyTextLength: bodyText.length,
        bodyPreview: bodyText.replace(/\s+/g, " ").trim().slice(0, 500),
        appBuild: String(page.appBuild || ""),
        hasCtoxConfigParam: Boolean(page.hasCtoxConfigParam),
        ctoxConfigLength: Number(page.ctoxConfigLength || 0),
        decodedCtoxConfig: page.decodedCtoxConfig || null,
        loggedOutMarker: String(page.loggedOutMarker || ""),
        pairingConfigStored: Boolean(page.pairingConfigStored),
        shellReady: page.shellReady === true,
        moduleLoading: String(page.moduleLoading || ""),
        activeModule: String(page.activeModule || ""),
        startupLoaderVisible: page.startupLoaderVisible === true,
        startupErrorVisible: page.startupErrorVisible === true,
        statusText: String(page.statusText || "").slice(0, 240),
        loginPromptVisible: lastLoginPromptVisible,
        systemStartFailed: Boolean(failureText) || page.startupErrorVisible === true,
        failureText,
        failLoad,
        consoleMessageCount: consoleMessages.length,
        consoleMessages: consoleMessages.slice(0, 8),
        networkEvents: networkEvents.slice(-12),
      });
    }
    window.loadURL(launchConfig.launchUrl).catch((error) => {
      failLoad = {
        errorCode: 0,
        errorDescription: error instanceof Error ? error.message : String(error),
        origin: safeOrigin(launchConfig.launchUrl),
      };
      finish("loadURL-error");
    });
  });
}

async function captureRenderedLaunchPage(window) {
  return window.webContents.executeJavaScript(`(() => {
    function isVisible(element) {
      if (!element || element.hidden) return false;
      const rect = element.getBoundingClientRect();
      const style = window.getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.display !== "none" && style.visibility !== "hidden";
    }
    function decodeConfig(packed) {
      try {
        const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        const normalized = String(packed || "").replace(/-/g, "+").replace(/_/g, "/").replace(/\\s/g, "");
        let buffer = 0;
        let bits = 0;
        const bytes = [];
        for (const char of normalized) {
          if (char === "=") break;
          const index = alphabet.indexOf(char);
          if (index < 0) throw new Error("invalid base64url character");
          buffer = (buffer << 6) | index;
          bits += 6;
          if (bits >= 8) {
            bits -= 8;
            bytes.push((buffer >> bits) & 0xff);
          }
        }
        const rawJson = new TextDecoder().decode(new Uint8Array(bytes));
        const decoded = JSON.parse(rawJson);
        const signalingUrls = Array.isArray(decoded?.signaling_urls)
          ? decoded.signaling_urls
          : (Array.isArray(decoded?.signalingUrls) ? decoded.signalingUrls : []);
        const roomPassword = String(decoded?.signaling_room_password || decoded?.signalingRoomPassword || decoded?.room_password || decoded?.roomPassword || "");
        const redactedMarkerCount = (rawJson.match(/<redacted>|\\[redacted\\]/gi) || []).length;
        return {
          ok: true,
          keyCount: Object.keys(decoded || {}).length,
          hasSyncRoom: Boolean(decoded?.sync_room || decoded?.syncRoom),
          signalingUrlCount: signalingUrls.length,
          signalingUrlRedactedMarkerCount: signalingUrls.filter((url) => /<redacted>|\\[redacted\\]/i.test(String(url || ""))).length,
          hasRoomPassword: Boolean(roomPassword),
          roomPasswordLength: roomPassword.length,
          roomPasswordIsRedacted: /<redacted>|\\[redacted\\]/i.test(roomPassword),
          hasSession: Boolean(decoded?.session?.authenticated),
          source: String(decoded?.source || ""),
          redactedMarkerCount
        };
      } catch (error) {
        return { ok: false, error: String(error && error.message || error || "decode failed").slice(0, 160) };
      }
    }
    const scripts = Array.from(document.scripts || []).map((script) => script.src || "").filter(Boolean);
    const appScript = scripts.find((src) => /\\/app\\.js(?:\\?|$)/.test(src)) || "";
    const params = new URLSearchParams(location.search || "");
    const packedConfig = params.get("ctox_config") || params.get("ctoxConfig") || "";
    const moduleLoading = document.body?.dataset?.moduleLoading || "";
    const activeModule = document.body?.dataset?.activeModule || "";
    const startupLoaderVisible = isVisible(document.getElementById("startup-loader"));
    const startupErrorVisible = isVisible(document.getElementById("startup-error-card"));
    const statusText = document.querySelector("[data-status-text]")?.textContent || "";
    const moduleRootCount = document.querySelectorAll("[data-module-root]").length;
    return {
      title: document.title || "",
      url: location.href,
      bodyText: (document.body && document.body.innerText || "").slice(0, 20000),
      hasCtoxConfigParam: Boolean(packedConfig),
      ctoxConfigLength: packedConfig.length,
      decodedCtoxConfig: decodeConfig(packedConfig),
      loggedOutMarker: localStorage.getItem("ctox.businessOs.loggedOut") || "",
      pairingConfigStored: Boolean(localStorage.getItem("ctox.businessOs.pairingConfig")),
      appScript,
      appBuild: (appScript.match(/[?&]v=([^&#]+)/) || [])[1] || "",
      moduleLoading,
      activeModule,
      moduleRootCount,
      startupLoaderVisible,
      startupErrorVisible,
      statusText,
      shellReady: !moduleLoading && !startupLoaderVisible && !startupErrorVisible && Boolean(activeModule || moduleRootCount > 0)
    };
  })()`, true);
}

function pushNetworkEvent(events, event) {
  const normalized = networkEventSummary(event);
  if (!normalized.origin && !normalized.path) return;
  events.push(normalized);
  if (events.length > 60) events.splice(0, events.length - 60);
}

function networkEventSummary(event) {
  return {
    type: String(event?.type || ""),
    origin: safeOrigin(event?.url),
    path: safePathWithoutSearch(event?.url),
    statusCode: Number(event?.statusCode || 0),
    error: String(event?.error || "").slice(0, 180),
  };
}

function extractLaunchFailureText(bodyText) {
  const text = String(bodyText || "");
  const patterns = [
    /System-Start fehlgeschlagen[\s\S]{0,700}/i,
    /Netzwerkverbindung fehlgeschlagen[\s\S]{0,500}/i,
    /Signalisierungs-Server[\s\S]{0,500}/i,
  ];
  for (const pattern of patterns) {
    const match = text.match(pattern);
    if (match) return match[0].replace(/\s+/g, " ").trim().slice(0, 700);
  }
  return "";
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

async function loginWithPasswordApi(baseUrl, email, password, browserSession = session.defaultSession) {
  const response = await browserSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/auth/password`, {
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

async function fetchRawSessionPackage(baseUrl, browserSession = session.defaultSession) {
  const response = await browserSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/desktop/session-package`, {
    cache: "no-store",
    credentials: "include",
    headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
  });
  if (!response.ok) throw new Error(`ctox.dev live session-package failed: ${response.status}`);
  return response.json();
}

async function isCtoxDevSessionAuthenticated(baseUrl) {
  const state = await fetchSessionState(baseUrl);
  return state.accountAuthenticated === true || state.tenantCount > 0;
}

async function fetchSessionState(baseUrl, browserSession = session.defaultSession) {
  const response = await browserSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/desktop/session-package`, {
    cache: "no-store",
    credentials: "include",
    headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
  });
  const payload = response.ok ? await response.json().catch(() => ({})) : {};
  return {
    ok: response.ok,
    status: response.status,
    accountAuthenticated: payload?.account?.authenticated === true,
    tenantCount: Array.isArray(payload?.account?.tenants) ? payload.account.tenants.length : 0,
    desktopProtocol: payload?.desktopProtocol || "",
  };
}

async function exerciseSessionRotation({
  baseUrl,
  email,
  password,
  sourceManager,
  selectedInstance,
  expectedTenants,
  useAuthWindow,
  writeProgress = () => undefined,
}) {
  const logout = await clearCtoxDevSession(session.defaultSession, baseUrl);
  const postLogoutSession = await fetchSessionState(baseUrl);
  const postLogoutInstances = await sourceManager.listInstances();
  const postLogoutManagedCount = postLogoutInstances.filter((instance) => instance.source === "ctox_dev").length;
  writeProgress("session-rotation-post-logout", {
    sessionRotation: {
      logout,
      postLogoutSession,
      postLogoutManagedCount,
    },
  });
  if (postLogoutSession.accountAuthenticated === true || postLogoutSession.tenantCount > 0) {
    throw new Error("ctox.dev session-package still authenticated after session logout");
  }
  let launchAfterLogoutError = "";
  try {
    await sourceManager.getLaunchConfig(selectedInstance);
  } catch (error) {
    launchAfterLogoutError = error instanceof Error ? error.message : String(error);
  }
  if (!launchAfterLogoutError) {
    throw new Error("ctox.dev launch unexpectedly succeeded after session logout");
  }
  writeProgress("session-rotation-launch-blocked", {
    sessionRotation: {
      logout,
      postLogoutSession,
      postLogoutManagedCount,
      launchAfterLogoutBlocked: true,
      launchAfterLogoutError,
    },
  });
  if (postLogoutManagedCount !== 0) {
    throw new Error(`ctox.dev managed instances still visible after session logout: ${postLogoutManagedCount}`);
  }

  const relogin = useAuthWindow
    ? await loginWithAuthWindow(baseUrl, email, password)
    : await loginWithPasswordApi(baseUrl, email, password);
  writeProgress("session-rotation-relogin", {
    sessionRotation: {
      logout,
      postLogoutSession,
      postLogoutManagedCount,
      launchAfterLogoutBlocked: true,
      relogin: summarizeLogin(relogin),
    },
  });
  if (relogin.ok !== true || relogin.completed !== true) {
    throw new Error("ctox.dev relogin did not complete after session rotation");
  }
  const postReloginSession = await fetchSessionState(baseUrl);
  if (postReloginSession.accountAuthenticated !== true && postReloginSession.tenantCount === 0) {
    throw new Error("ctox.dev session-package did not recover after relogin");
  }
  const postReloginInstances = await sourceManager.listInstances();
  const managedInstances = postReloginInstances.filter((instance) => instance.source === "ctox_dev");
  writeProgress("session-rotation-post-relogin", {
    sessionRotation: {
      logout,
      postLogoutSession,
      postLogoutManagedCount,
      launchAfterLogoutBlocked: true,
      relogin: summarizeLogin(relogin),
      postReloginSession,
      postReloginManagedCount: managedInstances.length,
    },
  });
  const expectedTenantsPresent = expectedTenants.every((expected) => {
    const normalized = expected.toLowerCase();
    return managedInstances.some((instance) => [
      instance.displayName,
      instance.domain,
      instance.tenantId,
      instance.instanceId,
    ].filter(Boolean).some((value) => String(value).toLowerCase().includes(normalized)));
  });
  if (expectedTenants.length > 0 && !expectedTenantsPresent) {
    throw new Error(`expected tenants missing after session rotation: ${expectedTenants.join(", ")}`);
  }
  const relaunchInstance = selectLaunchInstance(managedInstances, expectedTenants);
  if (!relaunchInstance) throw new Error("no ctox.dev managed instances available after relogin");
  const relaunchConfig = await sourceManager.getLaunchConfig(relaunchInstance);
  if (relaunchConfig.ctoxConfig?.transport !== "webrtc") throw new Error("rotated launch config transport is not webrtc");
  if (relaunchConfig.ctoxConfig?.http_bridge_available !== false) {
    throw new Error("rotated launch config http_bridge_available is not false");
  }
  const finalLogout = await clearCtoxDevSession(session.defaultSession, baseUrl);
  const result = {
    logout,
    postLogoutSession,
    postLogoutManagedCount,
    launchAfterLogoutBlocked: true,
    launchAfterLogoutError,
    relogin,
    postReloginSession,
    postReloginManagedCount: managedInstances.length,
    expectedTenantsPresent,
    relaunch: {
      source: relaunchConfig.source,
      tenantId: relaunchInstance.tenantId,
      displayName: relaunchInstance.displayName,
      launchUrlOrigin: safeOrigin(relaunchConfig.launchUrl),
      launchUrlPath: safePath(relaunchConfig.launchUrl),
      transport: relaunchConfig.ctoxConfig.transport,
      httpBridgeAvailable: relaunchConfig.ctoxConfig.http_bridge_available,
      signalingUrlCount: Array.isArray(relaunchConfig.ctoxConfig.signaling_urls)
        ? relaunchConfig.ctoxConfig.signaling_urls.length
        : 0,
      hasRoomPassword: Boolean(relaunchConfig.ctoxConfig.signaling_room_password),
      expiresAt: relaunchConfig.expiresAt || "",
    },
    finalLogout,
  };
  writeProgress("session-rotation-complete", { sessionRotation: result });
  return result;
}

async function exerciseAccessRevocation({
  baseUrl,
  adminSession,
  adminEmail,
  memberSession,
  memberEmail,
  memberPassword,
  tenantSelector,
  adminManagedInstances,
  writeProgress = () => undefined,
}) {
  if (String(memberEmail || "").toLowerCase() === String(adminEmail || "").toLowerCase()) {
    throw new Error("access revocation member must be different from the admin login");
  }
  const tenant = selectTenantForAccessRevocation(adminManagedInstances, tenantSelector);
  if (!tenant) throw new Error(`no ctox.dev managed tenant matched access revocation selector: ${tenantSelector}`);
  const membersPayload = await fetchTenantMembers(baseUrl, tenant.tenantId, adminSession);
  const targetMember = (membersPayload.members || []).find((member) => (
    String(member.email || "").toLowerCase() === String(memberEmail || "").toLowerCase()
  ));
  if (!targetMember?.user_id) {
    throw new Error(`access revocation member not found on selected tenant: ${memberEmail}`);
  }
  const originalRole = String(targetMember.role || "");
  if (!["admin", "operator", "user"].includes(originalRole)) {
    throw new Error(`access revocation member must start as launchable non-owner role, got: ${originalRole || "missing"}`);
  }

  await clearCtoxDevSession(memberSession, baseUrl);
  const memberLogin = await loginWithPasswordApi(baseUrl, memberEmail, memberPassword, memberSession);
  const memberSourceManager = createSourceManagerForSession(baseUrl, memberSession);
  const preRevocationSession = await fetchSessionState(baseUrl, memberSession);
  const preRevocationInstances = await memberSourceManager.listInstances();
  const preRevocationInstance = findInstanceForTenant(preRevocationInstances, tenant.tenantId);
  if (!preRevocationInstance) {
    throw new Error("access revocation target tenant is not visible to member before role change");
  }
  if (preRevocationInstance.status !== "available") {
    throw new Error(`access revocation target tenant is not launchable before role change: ${preRevocationInstance.status}`);
  }
  const preRevocationLaunch = await memberSourceManager.getLaunchConfig(preRevocationInstance);
  if (preRevocationLaunch.ctoxConfig?.transport !== "webrtc") {
    throw new Error("access revocation pre-check launch transport is not webrtc");
  }
  if (preRevocationLaunch.ctoxConfig?.http_bridge_available !== false) {
    throw new Error("access revocation pre-check launch http_bridge_available is not false");
  }
  writeProgress("access-revocation-prechecked", {
    accessRevocation: {
      tenantId: tenant.tenantId,
      displayName: tenant.displayName,
      memberEmail,
      originalRole,
      memberLogin: summarizeLogin(memberLogin),
      preRevocationSession,
      preRevocationStatus: preRevocationInstance.status,
      preRevocationLaunch: {
        transport: preRevocationLaunch.ctoxConfig.transport,
        httpBridgeAvailable: preRevocationLaunch.ctoxConfig.http_bridge_available,
      },
    },
  });

  let restored = false;
  try {
    await updateTenantMemberRole(baseUrl, tenant.tenantId, targetMember.user_id, "viewer", adminSession);
    const postRevocationSessionPackage = await fetchRawSessionPackage(baseUrl, memberSession);
    const postRevocationSession = summarizeSessionPackage(postRevocationSessionPackage);
    const postRevocationInstances = await memberSourceManager.listInstances();
    const postRevocationInstance = findInstanceForTenant(postRevocationInstances, tenant.tenantId);
    let launchAfterRevocationError = "";
    try {
      await memberSourceManager.getLaunchConfig(postRevocationInstance || preRevocationInstance);
    } catch (error) {
      launchAfterRevocationError = error instanceof Error ? error.message : String(error);
    }
    const blocked = summarizeAccessRevocationBlock({
      postRevocationInstance,
      launchAfterRevocationError,
    });
    writeProgress("access-revocation-blocked", {
      accessRevocation: {
        tenantId: tenant.tenantId,
        displayName: tenant.displayName,
        memberEmail,
        originalRole,
        revokedRole: "viewer",
        postRevocationSession,
        ...blocked,
      },
    });

    await updateTenantMemberRole(baseUrl, tenant.tenantId, targetMember.user_id, originalRole, adminSession);
    restored = true;
    const postRestoreSessionPackage = await fetchRawSessionPackage(baseUrl, memberSession);
    const postRestoreInstances = await memberSourceManager.listInstances();
    const postRestoreInstance = findInstanceForTenant(postRestoreInstances, tenant.tenantId);
    if (!postRestoreInstance || postRestoreInstance.status !== "available") {
      throw new Error("access revocation target tenant did not become available after role restore");
    }
    const result = {
      tenantId: tenant.tenantId,
      displayName: tenant.displayName,
      memberEmail,
      originalRole,
      revokedRole: "viewer",
      preRevocationSession,
      preRevocationStatus: preRevocationInstance.status,
      postRevocationSession,
      ...blocked,
      restored,
      postRestoreSession: summarizeSessionPackage(postRestoreSessionPackage),
      postRestoreStatus: postRestoreInstance.status,
    };
    writeProgress("access-revocation-complete", { accessRevocation: result });
    return result;
  } finally {
    if (!restored) {
      await updateTenantMemberRole(baseUrl, tenant.tenantId, targetMember.user_id, originalRole, adminSession)
        .catch(() => null);
    }
    await clearCtoxDevSession(memberSession, baseUrl).catch(() => null);
  }
}

function createSourceManagerForSession(baseUrl, browserSession) {
  const registry = {
    settings: {
      ctoxDevBaseUrl: baseUrl,
      shellUrl: `${baseUrl.replace(/\/+$/, "")}/business-os/`,
    },
    instances: [],
    usage: {},
  };
  return new SourceManager({
    registryProvider: () => registry,
    registrySaver: () => undefined,
    secretStore: new MemorySecretStore(),
    ctoxDevBaseUrl: baseUrl,
    shellUrl: registry.settings.shellUrl,
    fetchImpl: browserSession.fetch.bind(browserSession),
  });
}

async function fetchTenantMembers(baseUrl, tenantId, browserSession) {
  const response = await browserSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/instances/${encodeURIComponent(tenantId)}/members`, {
    cache: "no-store",
    credentials: "include",
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(`ctox.dev members fetch failed: ${response.status}`);
  }
  return payload;
}

async function updateTenantMemberRole(baseUrl, tenantId, userId, role, browserSession) {
  const response = await browserSession.fetch(`${baseUrl.replace(/\/+$/, "")}/api/instances/${encodeURIComponent(tenantId)}/members`, {
    method: "PATCH",
    cache: "no-store",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ userId, role }),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok || payload.ok !== true) {
    throw new Error(`ctox.dev member role update failed: ${response.status}`);
  }
  return payload;
}

function selectTenantForAccessRevocation(instances, selector) {
  const normalized = String(selector || "").trim().toLowerCase();
  return (instances || []).find((instance) => [
    instance.displayName,
    instance.domain,
    instance.tenantId,
    instance.instanceId,
  ].filter(Boolean).some((value) => String(value).toLowerCase().includes(normalized)));
}

function findInstanceForTenant(instances, tenantId) {
  return (instances || []).find((instance) => String(instance.tenantId || "") === String(tenantId || ""));
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

function safePathWithoutSearch(rawUrl) {
  try {
    return new URL(String(rawUrl || "")).pathname;
  } catch (_error) {
    return "";
  }
}

function readCredentials() {
  return new Promise((resolve, reject) => {
    let input = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      input += chunk;
    });
    process.stdin.on("end", () => {
      const normalized = input.replace(/\r?\n$/, "");
      const lines = normalized.split(/\r?\n/);
      const password = options.accessRevocation ? String(lines[0] || "") : normalized;
      if (!password) {
        reject(new Error("password stdin was empty"));
        return;
      }
      const memberPassword = options.accessRevocation ? String(lines[1] || "") : "";
      if (options.accessRevocation && !memberPassword) {
        reject(new Error("member password stdin was empty"));
        return;
      }
      resolve({ password, memberPassword });
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
