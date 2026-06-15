"use strict";

const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("ctoxBusinessOsDesktop", {
  openSwitcher: () => ipcRenderer.invoke("app-shell:open-switcher"),
});
