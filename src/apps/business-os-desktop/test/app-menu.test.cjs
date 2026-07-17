"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { buildApplicationMenuTemplate } = require("../src/main/app-menu.cjs");

test("native menu exposes working instance, version and help actions", () => {
  const handlers = {
    showAbout() {},
    openSwitcher() {},
    openConnection() {},
    refreshActiveInstance() {},
    openDocumentation() {},
    openReleaseNotes() {},
  };
  const template = buildApplicationMenuTemplate({
    appName: "CTOX Business-OS Desktop Beta",
    version: "0.3.42",
    platform: "darwin",
    handlers,
  });
  const appMenu = template.find((menu) => menu.label === "CTOX Business-OS Desktop Beta");
  const instanceMenu = template.find((menu) => menu.label === "Instanz");
  const helpMenu = template.find((menu) => menu.label === "Hilfe");

  assert.equal(appMenu.submenu[0].label, "Über CTOX Business-OS Desktop Beta");
  assert.equal(instanceMenu.submenu.find((item) => item.label === "Instanz wechseln").accelerator, "CmdOrCtrl+K");
  assert.equal(instanceMenu.submenu.find((item) => item.label === "Instanz verbinden …").click, handlers.openConnection);
  assert.equal(
    instanceMenu.submenu.find((item) => item.label === "Aktive Instanz neu verbinden").click,
    handlers.refreshActiveInstance,
  );
  assert.equal(helpMenu.submenu.at(-1).label, "Release-Hinweise für v0.3.42");
});

test("non-macOS menu keeps about and quit under Datei", () => {
  const template = buildApplicationMenuTemplate({ platform: "win32" });
  const fileMenu = template.find((menu) => menu.label === "Datei");
  assert.equal(fileMenu.submenu[0].label.startsWith("Über "), true);
  assert.equal(fileMenu.submenu.at(-1).role, "quit");
});
