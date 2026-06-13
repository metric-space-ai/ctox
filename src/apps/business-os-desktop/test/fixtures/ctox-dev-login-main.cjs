"use strict";

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { app, BrowserWindow, session } = require("electron");
const { SourceManager } = require("../../src/main/source-manager.cjs");
const {
  clearCtoxDevSession,
  openCtoxDevLoginWindow,
} = require("../../src/main/ctox-dev-login.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];

if (!outputPath || !userDataPath) {
  throw new Error("usage: electron ctox-dev-login-main.cjs <outputPath> <userDataPath>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

app.whenReady().then(async () => {
  const server = await startCtoxDevMockServer();
  const keepAliveWindow = new BrowserWindow({
    show: false,
    width: 1,
    height: 1,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  let exitCode = 0;
  try {
    const baseUrl = `http://127.0.0.1:${server.address().port}`;
    let registry = {
      settings: {
        ctoxDevBaseUrl: baseUrl,
        shellUrl: `${baseUrl}/shell`,
      },
      instances: [{
        id: "local:lab",
        source: "local_daemon",
        displayName: "Local Lab",
        status: "available",
        healthSummary: {
          dataPlane: "rxdb-webrtc",
          dataPlaneReady: true,
          httpDataProxy: false,
          nativePeerObserved: true,
        },
      }],
      usage: {},
    };
    const secretStore = new MemorySecretStore();
    const sourceManager = new SourceManager({
      registryProvider: () => registry,
      registrySaver: (nextRegistry) => {
        registry = nextRegistry;
      },
      secretStore,
      ctoxDevBaseUrl: baseUrl,
      shellUrl: `${baseUrl}/shell`,
      fetchImpl: session.defaultSession.fetch.bind(session.defaultSession),
    });
    const login = await openCtoxDevLoginWindow({
      BrowserWindow,
      baseUrl,
    });
    const instances = await sourceManager.listInstances();
    const managedInstances = instances.filter((instance) => instance.source === "ctox_dev");
    const launch = await sourceManager.getLaunchConfig(managedInstances[0]);
    const rotatedLaunch = await sourceManager.getLaunchConfig(managedInstances[0]);
    server.revokeTenant("tenant_skf");
    const instancesAfterRevocation = await sourceManager.listInstances();
    const managedInstancesAfterRevocation = instancesAfterRevocation.filter((instance) => instance.source === "ctox_dev");
    const logout = await clearCtoxDevSession(session.defaultSession, baseUrl);
    const instancesAfterLogout = await sourceManager.listInstances();
    const result = {
      ok: login.ok === true
        && login.completed === true
        && managedInstances.map((instance) => instance.displayName).join("|") === "Kunstmen|SKF"
        && launch.source === "ctox_dev"
        && launch.ctoxConfig.transport === "webrtc"
        && launch.ctoxConfig.http_bridge_available === false
        && rotatedLaunch.expiresAt !== launch.expiresAt
        && rotatedLaunch.ctoxConfig.launchEpoch === 2
        && managedInstancesAfterRevocation.map((instance) => instance.displayName).join("|") === "Kunstmen"
        && instancesAfterRevocation.map((instance) => instance.displayName).join("|") === "Kunstmen|Local Lab"
        && logout.ok === true
        && instancesAfterLogout.map((instance) => instance.displayName).join("|") === "Local Lab"
        && server.evidence.sessionPackageSawCookie === true
        && server.evidence.launchTokenSawCookie === true
        && server.evidence.launchConfigSawCookie === true,
      login,
      logout,
      instanceNames: instances.map((instance) => instance.displayName),
      managedInstanceNames: managedInstances.map((instance) => instance.displayName),
      instanceNamesAfterRevocation: instancesAfterRevocation.map((instance) => instance.displayName),
      managedInstanceNamesAfterRevocation: managedInstancesAfterRevocation.map((instance) => instance.displayName),
      instanceNamesAfterLogout: instancesAfterLogout.map((instance) => instance.displayName),
      launch,
      rotatedLaunch,
      evidence: server.evidence,
    };
    writeResult(result);
    exitCode = result.ok ? 0 : 2;
  } catch (error) {
    writeResult({
      ok: false,
      error: error instanceof Error ? error.stack || error.message : String(error),
      evidence: server.evidence,
    });
    exitCode = 1;
  } finally {
    if (!keepAliveWindow.isDestroyed()) keepAliveWindow.destroy();
    await closeServer(server);
    process.exit(exitCode);
  }
});

function startCtoxDevMockServer() {
  const revokedTenants = new Set();
  const evidence = {
    sessionPackageSawCookie: false,
    sessionPackageLastSawCookie: false,
    sessionPackageCookieObservations: [],
    launchTokenSawCookie: false,
    launchTokenTenantIds: [],
    launchConfigSawCookie: false,
    launchConfigUrls: [],
  };
  let launchEpoch = 0;
  const server = http.createServer((request, response) => {
    if (request.url.startsWith("/dashboard?") && request.url.includes("auth_completed=1")) {
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end("<!doctype html><title>done</title>");
      return;
    }
    if (request.url.startsWith("/dashboard?")) {
      response.writeHead(302, {
        "set-cookie": "ctox_session=desktop-ok; Path=/; HttpOnly; SameSite=Lax",
        location: "/dashboard?desktop=1&client=ctox-business-os-desktop&auth_completed=1",
      });
      response.end();
      return;
    }
    if (request.url.startsWith("/desktop/auth?")) {
      response.writeHead(302, {
        "set-cookie": "ctox_session=desktop-ok; Path=/; HttpOnly; SameSite=Lax",
        location: "/desktop/auth/complete",
      });
      response.end();
      return;
    }
    if (request.url === "/desktop/auth/complete") {
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end("<!doctype html><title>done</title>");
      return;
    }
    if (request.url === "/api/desktop/session-package") {
      const sawCookie = hasSessionCookie(request);
      evidence.sessionPackageSawCookie = evidence.sessionPackageSawCookie || sawCookie;
      evidence.sessionPackageLastSawCookie = sawCookie;
      evidence.sessionPackageCookieObservations.push(sawCookie);
      const tenants = [{
        id: "tenant_kunstmen",
        slug: "kunstmen",
        domain: "kunstmen.ctox.dev",
        businessName: "Kunstmen",
        status: "active",
        healthStatus: "ok",
        tenantRole: "admin",
        launchAllowed: true,
      }, {
        id: "tenant_skf",
        slug: "skf",
        domain: "skf.ctox.dev",
        businessName: "SKF",
        status: "active",
        healthStatus: "ok",
        tenantRole: "owner",
        launchAllowed: true,
      }].filter((tenant) => !revokedTenants.has(tenant.id));
      writeJson(response, sawCookie ? {
        account: {
          tenants,
        },
      } : { account: { tenants: [] } });
      return;
    }
    if (request.url === "/api/desktop/launch-token" && request.method === "POST") {
      evidence.launchTokenSawCookie = hasSessionCookie(request);
      readBody(request).then((body) => {
        const payload = JSON.parse(body || "{}");
        launchEpoch += 1;
        evidence.launchTokenTenantIds.push(payload.tenantId);
        writeJson(response, {
          launchConfigUrl: `http://127.0.0.1:${server.address().port}/api/desktop/launch/${payload.tenantId}/${launchEpoch}`,
          expiresAt: `2099-01-01T00:00:0${launchEpoch}.000Z`,
        });
      }).catch((error) => {
        response.writeHead(500);
        response.end(String(error));
      });
      return;
    }
    if (request.url.startsWith("/api/desktop/launch/") && request.method === "POST") {
      evidence.launchConfigSawCookie = hasSessionCookie(request);
      evidence.launchConfigUrls.push(request.url);
      writeJson(response, {
        launchUrl: `http://127.0.0.1:${server.address().port}/shell?ctox_config=packed`,
        pairingConfig: {
          transport: "webrtc",
          http_bridge_available: false,
          launchEpoch,
        },
      });
      return;
    }
    response.writeHead(404);
    response.end("not found");
  });
  server.evidence = evidence;
  server.revokeTenant = (tenantId) => {
    revokedTenants.add(tenantId);
  };
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function hasSessionCookie(request) {
  return String(request.headers.cookie || "").includes("ctox_session=desktop-ok");
}

function readBody(request) {
  return new Promise((resolve, reject) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => resolve(body));
    request.on("error", reject);
  });
}

function writeJson(response, payload) {
  response.writeHead(200, { "content-type": "application/json; charset=utf-8" });
  response.end(`${JSON.stringify(payload)}\n`);
}

function writeResult(result) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
}

function closeServer(server) {
  return new Promise((resolve, reject) => {
    if (typeof server.closeAllConnections === "function") {
      server.closeAllConnections();
    }
    const timeout = setTimeout(resolve, 1000);
    server.close((error) => {
      clearTimeout(timeout);
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

class MemorySecretStore {
  constructor() {
    this.values = new Map();
  }

  async set(key, value) {
    this.values.set(key, value);
  }

  async get(key) {
    return this.values.get(key);
  }

  async delete(key) {
    this.values.delete(key);
  }
}
