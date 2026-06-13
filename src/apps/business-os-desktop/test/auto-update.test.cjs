"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  configureAutoUpdates,
  sanitizeUpdateError,
} = require("../src/main/auto-update.cjs");

test("auto update config is inert in development and disables silent downloads", () => {
  const events = [];
  const scheduled = [];
  const autoUpdater = {
    on: (event) => events.push(event),
  };
  const result = configureAutoUpdates({
    app: { isPackaged: false },
    autoUpdater,
    scheduler: (callback, delay) => scheduled.push({ callback, delay }),
  });

  assert.deepEqual(result, { enabled: false, reason: "not_packaged" });
  assert.equal(autoUpdater.autoDownload, false);
  assert.equal(autoUpdater.autoInstallOnAppQuit, false);
  assert.deepEqual(events, ["error", "update-available", "update-not-available"]);
  assert.equal(scheduled.length, 0);
});

test("auto update schedules packaged update checks without embedding secrets in logs", async () => {
  const warnings = [];
  const autoUpdater = {
    on: () => undefined,
    checkForUpdates: async () => {
      throw new Error("feed failed token=secret-value");
    },
  };
  let scheduled;
  const result = configureAutoUpdates({
    app: { isPackaged: true },
    autoUpdater,
    logger: {
      warn: (...args) => warnings.push(args),
      info: () => undefined,
    },
    scheduler: (callback, delay) => {
      scheduled = { callback, delay };
    },
    startupDelayMs: 1,
  });

  assert.deepEqual(result, { enabled: true });
  assert.equal(scheduled.delay, 1);
  await scheduled.callback();
  assert.equal(JSON.stringify(warnings).includes("secret-value"), false);
  assert.equal(JSON.stringify(warnings).includes("token=[redacted]"), true);
});

test("auto update error sanitizer redacts query-style credentials", () => {
  assert.equal(
    sanitizeUpdateError(new Error("https://updates.ctox.dev/latest.yml?credential=abc123")),
    "https://updates.ctox.dev/latest.yml?credential=[redacted]",
  );
});
