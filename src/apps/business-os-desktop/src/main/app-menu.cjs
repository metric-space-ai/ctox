"use strict";

function buildApplicationMenuTemplate({
  appName = "CTOX Business-OS Desktop Beta",
  version = "",
  platform = process.platform,
  handlers = {},
} = {}) {
  const aboutItem = {
    label: `Über ${appName}`,
    click: handlers.showAbout,
  };
  const template = [];

  if (platform === "darwin") {
    template.push({
      label: appName,
      submenu: [
        aboutItem,
        { type: "separator" },
        { role: "services" },
        { type: "separator" },
        { role: "hide" },
        { role: "hideOthers" },
        { role: "unhide" },
        { type: "separator" },
        { role: "quit" },
      ],
    });
  } else {
    template.push({
      label: "Datei",
      submenu: [
        aboutItem,
        { type: "separator" },
        { role: "quit" },
      ],
    });
  }

  template.push(
    {
      label: "Instanz",
      submenu: [
        {
          label: "Instanz wechseln",
          accelerator: "CmdOrCtrl+K",
          click: handlers.openSwitcher,
        },
        {
          label: "Instanz verbinden …",
          accelerator: "CmdOrCtrl+N",
          click: handlers.openConnection,
        },
        { type: "separator" },
        {
          label: "Aktive Instanz neu verbinden",
          accelerator: "CmdOrCtrl+Shift+R",
          click: handlers.refreshActiveInstance,
        },
      ],
    },
    {
      label: "Bearbeiten",
      submenu: [
        { role: "undo" },
        { role: "redo" },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { role: "selectAll" },
      ],
    },
    {
      label: "Ansicht",
      submenu: [
        { role: "resetZoom" },
        { role: "zoomIn" },
        { role: "zoomOut" },
        { type: "separator" },
        { role: "togglefullscreen" },
      ],
    },
    {
      label: "Fenster",
      submenu: [
        { role: "minimize" },
        { role: "zoom" },
        ...(platform === "darwin"
          ? [{ type: "separator" }, { role: "front" }]
          : [{ role: "close" }]),
      ],
    },
    {
      role: "help",
      label: "Hilfe",
      submenu: [
        {
          label: "CTOX Dokumentation",
          click: handlers.openDocumentation,
        },
        {
          label: `Release-Hinweise${version ? ` für v${version}` : ""}`,
          click: handlers.openReleaseNotes,
        },
      ],
    },
  );

  return template;
}

module.exports = { buildApplicationMenuTemplate };
