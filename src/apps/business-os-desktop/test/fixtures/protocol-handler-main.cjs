"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { app } = require("electron");
const { installDesktopProtocolHandling } = require("../../src/main/protocol-handler.cjs");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];
const coldStartUrl = process.argv[4];
const openUrl = process.argv[5];
const secondInstanceUrl = process.argv[6];
const authCallbackUrl = process.argv[7];

if (!outputPath || !userDataPath || !coldStartUrl || !openUrl || !secondInstanceUrl || !authCallbackUrl) {
  throw new Error("usage: electron protocol-handler-main.cjs <outputPath> <userDataPath> <coldStartUrl> <openUrl> <secondInstanceUrl> <authCallbackUrl>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

const events = [];
let ready = false;

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

app.emit("open-url", {
  preventDefault() {
    events.push({ type: "prevented-open-url-default" });
  },
}, openUrl);

app.whenReady().then(async () => {
  ready = true;
  const flushResults = await protocolHandling.flushPending();
  app.emit("second-instance", {}, ["--from-smoke", secondInstanceUrl]);
  app.emit("open-url", {
    preventDefault() {
      events.push({ type: "prevented-auth-default" });
    },
  }, authCallbackUrl);
  await protocolHandling.waitForIdle();
  const result = {
    ok: events.length === 6
      && events[0].type === "prevented-open-url-default"
      && events[1].instanceId === "managed:tenant_cold"
      && events[2].rawInvite === openUrl
      && events[3].instanceId === "managed:tenant_second"
      && events[4].type === "prevented-auth-default"
      && events[5].callbackUrl === authCallbackUrl
      && protocolHandling.getPendingUrls().length === 0,
    events,
    flushResultCount: flushResults.length,
    pendingUrls: protocolHandling.getPendingUrls(),
  };
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
  process.exit(result.ok ? 0 : 2);
}).catch((error) => {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, JSON.stringify({
    ok: false,
    error: error instanceof Error ? error.stack || error.message : String(error),
    events,
  }, null, 2));
  process.exit(1);
});
