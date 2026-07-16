"use strict";

const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("ctoxBusinessOsDesktop", {
  openSwitcher: () => ipcRenderer.invoke("app-shell:open-switcher"),
  startFileDrag: (payload) => ipcRenderer.send("instance:file-drag-start", payload),
});
