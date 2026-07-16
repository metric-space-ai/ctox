"use strict";

const fs = require("node:fs");
const path = require("node:path");

const MAX_NATIVE_DRAG_BYTES = 512 * 1024 * 1024;
const DRAG_FILE_TTL_MS = 24 * 60 * 60 * 1000;
const TRANSPARENT_PNG = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+AvzZAAAAAElFTkSuQmCC";

function installNativeFileDragBridge({ ipcMain, app, nativeImage, viewsProvider }) {
  if (!ipcMain?.on) throw new Error("Electron ipcMain is required for native file drag");
  ipcMain.on("instance:file-drag-start", (event, payload) => {
    if (!isManagedInstanceSender(event?.sender, viewsProvider?.())) return;
    try {
      const name = sanitizeDragFilename(payload?.name);
      const bytes = dragBytes(payload?.bytes);
      if (!bytes.length) throw new Error("drag payload is empty");
      if (bytes.length > MAX_NATIVE_DRAG_BYTES) throw new Error("drag payload exceeds the 512 MB native bridge limit");
      const root = path.join(app.getPath("temp"), "ctox-business-os-drag");
      fs.mkdirSync(root, { recursive: true, mode: 0o700 });
      removeExpiredDragFiles(root);
      const filePath = path.join(root, `${Date.now()}-${randomSuffix()}-${name}`);
      fs.writeFileSync(filePath, bytes, { mode: 0o600, flag: "wx" });
      const icon = nativeImage?.createFromDataURL?.(TRANSPARENT_PNG)
        || nativeImage?.createEmpty?.();
      event.sender.startDrag({ file: filePath, icon });
    } catch (error) {
      console.error("Native Business OS file drag failed", error instanceof Error ? error.message : String(error));
    }
  });
}

function isManagedInstanceSender(sender, views) {
  if (!sender || typeof sender.startDrag !== "function") return false;
  return Array.from(views?.values?.() || []).some((view) => view?.webContents?.id === sender.id);
}

function dragBytes(value) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof ArrayBuffer) return Buffer.from(value);
  if (ArrayBuffer.isView(value)) return Buffer.from(value.buffer, value.byteOffset, value.byteLength);
  if (Array.isArray(value)) return Buffer.from(value);
  throw new Error("drag payload bytes are invalid");
}

function sanitizeDragFilename(value) {
  const name = String(value || "Datei")
    .replace(/[\u0000-\u001f<>:"/\\|?*]+/g, "_")
    .replace(/^\.+/, "")
    .trim();
  return (name || "Datei").slice(0, 180);
}

function removeExpiredDragFiles(root, now = Date.now()) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const filePath = path.join(root, entry.name);
    try {
      if (now - fs.statSync(filePath).mtimeMs > DRAG_FILE_TTL_MS) fs.unlinkSync(filePath);
    } catch (_error) {
      // A concurrent drag cleanup can win the race; the active export is unaffected.
    }
  }
}

function randomSuffix() {
  return Math.random().toString(36).slice(2, 10);
}

module.exports = {
  MAX_NATIVE_DRAG_BYTES,
  dragBytes,
  installNativeFileDragBridge,
  isManagedInstanceSender,
  sanitizeDragFilename,
};
