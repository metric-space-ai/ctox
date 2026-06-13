"use strict";

function configureAutoUpdates({
  app,
  autoUpdater,
  logger = console,
  checkOnStart = true,
  scheduler = setTimeout,
  startupDelayMs = 30000,
} = {}) {
  if (!autoUpdater) return { enabled: false, reason: "missing_auto_updater" };
  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.on?.("error", (error) => {
    logger.warn?.("desktop auto-update error", sanitizeUpdateError(error));
  });
  autoUpdater.on?.("update-available", () => {
    logger.info?.("desktop auto-update available");
  });
  autoUpdater.on?.("update-not-available", () => {
    logger.info?.("desktop auto-update not available");
  });
  if (!app?.isPackaged) return { enabled: false, reason: "not_packaged" };
  if (checkOnStart) {
    scheduler(() => {
      autoUpdater.checkForUpdates?.().catch((error) => {
        logger.warn?.("desktop auto-update check failed", sanitizeUpdateError(error));
      });
    }, startupDelayMs);
  }
  return { enabled: true };
}

function sanitizeUpdateError(error) {
  const message = error instanceof Error ? error.message : String(error || "");
  return message.replace(/(token|password|secret|credential)=([^&\s]+)/gi, "$1=[redacted]");
}

module.exports = {
  configureAutoUpdates,
  sanitizeUpdateError,
};
