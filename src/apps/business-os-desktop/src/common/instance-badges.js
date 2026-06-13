(function initInstanceBadges(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  root.CtoxInstanceBadges = api;
}(typeof globalThis !== "undefined" ? globalThis : window, function createInstanceBadgesApi() {
  "use strict";

  function badgesForInstance(instance = {}) {
    const badges = [sourceBadge(instance.source)];
    if (instance.role) badges.push(roleBadge(instance.role));
    badges.push(statusBadge(instance.status));
    const health = healthBadge(instance.healthSummary);
    if (health) badges.push(health);
    return badges.filter(Boolean);
  }

  function sourceBadge(source) {
    const labels = {
      ctox_dev: "ctox.dev",
      local_daemon: "local",
      ssh_managed: "ssh",
      pairing_invite: "paired",
    };
    return {
      kind: "source",
      tone: source === "ctox_dev" ? "managed" : "neutral",
      label: labels[source] || String(source || "unknown"),
      title: "Instanzquelle",
    };
  }

  function roleBadge(role) {
    return {
      kind: "role",
      tone: "neutral",
      label: String(role || "").trim().toLowerCase(),
      title: "Rolle",
    };
  }

  function statusBadge(status) {
    const value = String(status || "available").trim();
    const labels = {
      available: "online",
      offline: "offline",
      needs_auth: "auth",
      pairing_expired: "expired",
      installing: "installing",
      error: "error",
    };
    const tones = {
      available: "ok",
      offline: "warn",
      needs_auth: "warn",
      pairing_expired: "warn",
      installing: "progress",
      error: "error",
    };
    return {
      kind: "status",
      tone: tones[value] || "neutral",
      label: labels[value] || value,
      title: "Status",
    };
  }

  function healthBadge(healthSummary) {
    if (!healthSummary || typeof healthSummary !== "object") return null;
    if (healthSummary.httpDataProxy) {
      return {
        kind: "health",
        tone: "error",
        label: "http data",
        title: "HTTP-Datenpfad ist nicht erlaubt",
      };
    }
    if (healthSummary.dataPlane !== "rxdb-webrtc") {
      return {
        kind: "health",
        tone: "warn",
        label: "sync ?",
        title: "Datenpfad unbekannt",
      };
    }
    if (healthSummary.dataPlaneReady && healthSummary.nativePeerObserved) {
      return {
        kind: "health",
        tone: "ok",
        label: "rxdb",
        title: "RxDB/WebRTC bereit",
      };
    }
    return {
      kind: "health",
      tone: "warn",
      label: "sync pending",
      title: "RxDB/WebRTC noch nicht bereit",
    };
  }

  function badgeSearchText(instance = {}) {
    return badgesForInstance(instance)
      .map((badge) => badge.label)
      .join(" ");
  }

  return {
    badgeSearchText,
    badgesForInstance,
    healthBadge,
    roleBadge,
    sourceBadge,
    statusBadge,
  };
}));
