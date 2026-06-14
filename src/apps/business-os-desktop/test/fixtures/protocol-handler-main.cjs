"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { app } = require("electron");
const { installDesktopProtocolHandling } = require("../../src/main/protocol-handler.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];
const inputPath = process.argv[4];

if (!outputPath || !userDataPath || !inputPath) {
  throw new Error("usage: electron protocol-handler-main.cjs <outputPath> <userDataPath> <inputPath>");
}

const {
  coldStartUrl,
  openUrl,
  secondInstanceUrl,
  authCallbackUrl,
} = JSON.parse(fs.readFileSync(inputPath, "utf8"));

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

const events = [];
let ready = false;
const isWindows = process.platform === "win32";
const keepAlive = setInterval(() => {}, 1000);

const protocolHandling = installDesktopProtocolHandling({
  app,
  argv: [coldStartUrl],
  registerDefaultProtocolClient: false,
  singleInstanceLock: false,
  isReady: () => ready,
  handlersProvider: () => ({
    importInvite: async (rawInvite) => {
      events.push({ type: "invite", rawInvite });
      return { ok: true };
    },
    activateManagedInstance: async (instanceId) => {
      events.push({ type: "managed", instanceId });
      return { ok: true };
    },
    handleCtoxDevAuthCallback: async (callbackUrl) => {
      events.push({ type: "auth-callback", callbackUrl });
      return { ok: true, completed: true };
    },
  }),
});

if (!isWindows) {
  app.emit("open-url", {
    preventDefault() {
      events.push({ type: "prevented-open-url-default" });
    },
  }, openUrl);
}

app.whenReady().then(async () => {
  ready = true;
  const flushResults = await protocolHandling.flushPending();
  if (isWindows) {
    app.emit("second-instance", {}, ["--from-smoke", openUrl]);
    app.emit("second-instance", {}, ["--from-smoke", secondInstanceUrl]);
    app.emit("second-instance", {}, ["--from-smoke", authCallbackUrl]);
  } else {
    app.emit("second-instance", {}, ["--from-smoke", secondInstanceUrl]);
    app.emit("open-url", {
      preventDefault() {
        events.push({ type: "prevented-auth-default" });
      },
    }, authCallbackUrl);
  }
  await protocolHandling.waitForIdle();
  const expectedEventTypes = isWindows
    ? ["managed", "invite", "managed", "auth-callback"]
    : [
      "prevented-open-url-default",
      "managed",
      "invite",
      "managed",
      "prevented-auth-default",
      "auth-callback",
    ];
  const eventTypes = events.map((event) => event.type);
  const result = {
    ok: JSON.stringify(eventTypes) === JSON.stringify(expectedEventTypes)
      && events.some((event) => event.instanceId === "managed:tenant_cold")
      && events.some((event) => event.rawInvite === openUrl)
      && events.some((event) => event.instanceId === "managed:tenant_second")
      && events.some((event) => event.callbackUrl === authCallbackUrl)
      && protocolHandling.getPendingUrls().length === 0,
    events,
    flushResultCount: flushResults.length,
    pendingUrls: protocolHandling.getPendingUrls(),
  };
  writeResultAndExit(result, result.ok ? 0 : 2);
}).catch((error) => {
  writeResultAndExit({
    ok: false,
    error: error instanceof Error ? error.stack || error.message : String(error),
    events,
  }, 1);
});

function writeResultAndExit(result, code) {
  clearInterval(keepAlive);
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
  process.exit(code);
}
