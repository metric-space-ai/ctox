"use strict";

const state = {
  instances: [],
  query: "",
  activeInstanceId: "",
  selectedDetailsId: "",
  switcherOpen: false,
  sudoPasswordRefs: {},
  sshPasswordRefs: {},
};

const list = document.getElementById("instances");
const search = document.getElementById("search");
const openSwitcherButton = document.getElementById("open-switcher");
const emptyOpenSwitcherButton = document.getElementById("empty-open-switcher");
const closeSwitcherButton = document.getElementById("close-switcher");
const switcherBackdrop = document.getElementById("switcher-backdrop");
const switcherCount = document.getElementById("switcher-count");
const currentInstanceName = document.getElementById("current-instance-name");
const currentInstanceMeta = document.getElementById("current-instance-meta");
const loginButton = document.getElementById("login-ctox-dev");
const logoutButton = document.getElementById("logout-ctox-dev");
const manualPairingButton = document.getElementById("manual-pairing");
const emptyState = document.getElementById("empty-state");
const settingsPanel = document.getElementById("instance-settings");
const settingsName = document.getElementById("settings-name");
const settingsMeta = document.getElementById("settings-meta");
const settingsActions = document.getElementById("settings-actions");
const settingsBadges = document.getElementById("settings-badges");
const settingsFields = document.getElementById("settings-fields");
const passwordDialog = document.getElementById("password-dialog");
const passwordForm = document.getElementById("password-form");
const passwordDialogTitle = document.getElementById("password-dialog-title");
const passwordDialogInput = document.getElementById("password-dialog-input");
const passwordDialogCancel = document.getElementById("password-dialog-cancel");
const badgeApi = window.CtoxInstanceBadges;

window.ctoxDesktop.onOpenSwitcher?.(() => {
  openSwitcher({ focus: true }).catch((error) => console.error("open switcher failed", error));
});
openSwitcherButton.addEventListener("click", () => openSwitcher());
emptyOpenSwitcherButton.addEventListener("click", () => openSwitcher());
closeSwitcherButton.addEventListener("click", () => closeSwitcher());
switcherBackdrop.addEventListener("click", (event) => {
  if (event.target === switcherBackdrop) closeSwitcher();
});
loginButton.addEventListener("click", loginCtoxDev);
logoutButton.addEventListener("click", logoutCtoxDev);
manualPairingButton.addEventListener("click", importManualPairing);
search.addEventListener("input", () => {
  state.query = search.value.trim().toLowerCase();
  render();
});
document.addEventListener("keydown", async (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    await openSwitcher();
  }
  if (event.key === "Enter" && state.switcherOpen) {
    event.preventDefault();
    const [first] = filteredInstances();
    if (first) await activateInstance(first);
  }
  if (event.key === "Escape" && state.switcherOpen) {
    event.preventDefault();
    closeSwitcher();
  }
});

async function refresh() {
  setInstances(await window.ctoxDesktop.listInstances());
  render();
}

function setInstances(instances) {
  state.instances = Array.isArray(instances) ? instances : [];
  if (state.selectedDetailsId && !state.instances.some((instance) => instance.id === state.selectedDetailsId)) {
    state.selectedDetailsId = "";
  }
  if (state.activeInstanceId && !state.instances.some((instance) => instance.id === state.activeInstanceId)) {
    state.activeInstanceId = "";
  }
}

function filteredInstances() {
  if (!state.query) return state.instances;
  return state.instances.filter((instance) => {
    const haystack = [
      instance.displayName,
      instance.domain,
      instance.source,
      instance.status,
      instance.role,
      badgeApi.badgeSearchText(instance),
    ].filter(Boolean).join(" ").toLowerCase();
    return haystack.includes(state.query);
  });
}

function render() {
  const filtered = filteredInstances();
  list.replaceChildren();
  for (const instance of filtered) {
    const row = document.createElement("div");
    row.className = "instance";
    const button = document.createElement("button");
    button.className = "instance-main";
    button.type = "button";
    button.setAttribute("aria-selected", String(instance.id === state.activeInstanceId));
    button.innerHTML = `
      <span class="name"></span>
      <span class="badges"></span>
      <span class="meta"></span>
    `;
    button.querySelector(".name").textContent = instance.displayName;
    renderBadges(button.querySelector(".badges"), badgeApi.badgesForInstance(instance));
    button.querySelector(".meta").textContent = instanceMeta(instance);
    button.addEventListener("click", () => activateInstance(instance));
    row.appendChild(button);
    const actions = renderInstanceActions(instance);
    if (actions) row.appendChild(actions);
    list.appendChild(row);
  }
  switcherBackdrop.hidden = !state.switcherOpen;
  switcherCount.textContent = `${filtered.length} von ${state.instances.length} Instanzen`;
  const activeInstance = state.instances.find((instance) => instance.id === state.activeInstanceId);
  currentInstanceName.textContent = activeInstance?.displayName || "Instanz wählen";
  currentInstanceMeta.textContent = activeInstance ? sourceLabel(activeInstance.source) : "⌘K";
  emptyState.hidden = state.instances.length > 0 || Boolean(state.selectedDetailsId);
  emptyOpenSwitcherButton.disabled = state.instances.length === 0;
  renderSettingsPanel();
}

function renderInstanceActions(instance) {
  const actions = document.createElement("div");
  actions.className = "instance-actions";
  const details = document.createElement("button");
  details.className = "instance-action details-instance";
  details.type = "button";
  details.textContent = "Details";
  details.title = "Instanzdetails";
  details.setAttribute("aria-label", `${instance.displayName} Details`);
  details.addEventListener("click", () => showDetails(instance));
  actions.appendChild(details);
  if (instance.source !== "ctox_dev") return actions;
  const manage = document.createElement("button");
  manage.className = "instance-action manage-instance";
  manage.type = "button";
  manage.textContent = "Verwalten";
  manage.title = "In ctox.dev verwalten";
  manage.setAttribute("aria-label", `${instance.displayName} in ctox.dev verwalten`);
  manage.addEventListener("click", () => openManagedInstance(instance));
  actions.appendChild(manage);
  return actions;
}

function renderSettingsPanel() {
  const instance = state.instances.find((entry) => entry.id === state.selectedDetailsId);
  if (!instance) {
    settingsPanel.hidden = true;
    return;
  }
  settingsPanel.hidden = false;
  settingsName.textContent = instance.displayName;
  settingsMeta.textContent = instanceMeta(instance) || sourceLabel(instance.source);
  renderBadges(settingsBadges, badgeApi.badgesForInstance(instance));
  settingsActions.replaceChildren();
  if (instance.source === "ctox_dev") {
    settingsActions.appendChild(actionButton("In ctox.dev verwalten", () => openManagedInstance(instance), "manage-selected-instance"));
  } else if (instance.source === "pairing_invite") {
    settingsActions.appendChild(actionButton("Pairing rotieren", () => rotatePairingInstance(instance), "rotate-pairing-instance"));
    settingsActions.appendChild(actionButton("Pairing widerrufen", () => revokePairingInstance(instance), "revoke-pairing-instance danger"));
  } else if (instance.source === "ssh_managed") {
    settingsActions.appendChild(actionButton("SSH-Passwort speichern", () => storeSshLoginPassword(instance), "store-ssh-password"));
    settingsActions.appendChild(actionButton("Sudo-Passwort speichern", () => storeSshSudoPassword(instance), "store-sudo-password"));
    settingsActions.appendChild(actionButton("Aus App entfernen", () => removeUnmanagedInstance(instance), "remove-selected-instance danger"));
  } else {
    settingsActions.appendChild(actionButton("Aus App entfernen", () => removeUnmanagedInstance(instance), "remove-selected-instance danger"));
  }
  renderSettingsFields(instance);
}

function renderSettingsFields(instance) {
  settingsFields.replaceChildren();
  const fields = [
    ["Quelle", sourceLabel(instance.source)],
    ["Status", instance.status || "available"],
    ["Rolle", instance.role || ""],
    ["Domain", instance.domain || ""],
    ["Host", instance.connection?.host || ""],
    ["Instanz-ID", instance.instanceId || instance.tenantId || instance.id],
    ["Session", instance.sessionPartition || ""],
    ["Datenpfad", instance.healthSummary?.dataPlane || "rxdb-webrtc"],
    ["SSH Secret", state.sshPasswordRefs[instance.id] || ""],
    ["Sudo Secret", state.sudoPasswordRefs[instance.id] || ""],
  ].filter(([, value]) => value);
  for (const [label, value] of fields) {
    const term = document.createElement("dt");
    term.textContent = label;
    const description = document.createElement("dd");
    description.textContent = value;
    settingsFields.append(term, description);
  }
}

function actionButton(label, handler, className) {
  const button = document.createElement("button");
  button.className = `settings-action ${className || ""}`.trim();
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", handler);
  return button;
}

async function activateInstance(instance) {
  await window.ctoxDesktop.activateInstance(instance);
  state.activeInstanceId = instance.id;
  state.selectedDetailsId = "";
  await closeSwitcher();
  render();
}

async function showDetails(instance) {
  await closeSwitcher();
  await window.ctoxDesktop.showAppShell?.();
  state.selectedDetailsId = instance.id;
  state.activeInstanceId = "";
  render();
}

async function importManualPairing() {
  const displayName = window.prompt("Anzeigename");
  if (!displayName) return;
  const syncRoom = window.prompt("Sync Room");
  if (!syncRoom) return;
  const signalingUrl = window.prompt("Signaling URL", "wss://signaling.ctox.dev");
  if (!signalingUrl) return;
  const roomSecret = window.prompt("Room Secret");
  if (!roomSecret) return;
  await window.ctoxDesktop.importManualPairing({ displayName, syncRoom, signalingUrl, roomSecret });
  await refresh();
}

async function loginCtoxDev() {
  const result = await window.ctoxDesktop.loginCtoxDev();
  if (Array.isArray(result?.instances)) {
    setInstances(result.instances);
    render();
    if (state.instances.length > 0) await openSwitcher();
    return;
  }
  await refresh();
  if (state.instances.length > 0) await openSwitcher();
}

async function logoutCtoxDev() {
  const result = await window.ctoxDesktop.logoutCtoxDev();
  if (Array.isArray(result?.instances)) {
    setInstances(result.instances);
    render();
    return;
  }
  await refresh();
}

async function openSwitcher(options = {}) {
  state.switcherOpen = true;
  render();
  if (options.focus !== false) {
    search.focus();
    search.select();
  }
  await window.ctoxDesktop.setChromeOverlayVisible?.(true);
}

async function closeSwitcher() {
  if (!state.switcherOpen) return;
  state.switcherOpen = false;
  search.value = "";
  state.query = "";
  render();
  await window.ctoxDesktop.setChromeOverlayVisible?.(false);
  openSwitcherButton.focus();
}

async function openManagedInstance(instance) {
  await window.ctoxDesktop.openCtoxDevManagedInstance(instance);
}

async function removeUnmanagedInstance(instance) {
  if (instance.source === "ctox_dev") return;
  if (!window.confirm(`Instanz "${instance.displayName}" aus dieser App entfernen?`)) return;
  await window.ctoxDesktop.removeInstance(instance);
  state.selectedDetailsId = "";
  await refresh();
}

async function rotatePairingInstance(instance) {
  if (instance.source !== "pairing_invite") return;
  const rawInvite = window.prompt("Neues Pairing Invite");
  if (!rawInvite) return;
  await window.ctoxDesktop.rotatePairing(instance, rawInvite);
  await refresh();
}

async function revokePairingInstance(instance) {
  if (instance.source !== "pairing_invite") return;
  if (!window.confirm(`Pairing "${instance.displayName}" widerrufen?`)) return;
  await window.ctoxDesktop.revokePairing(instance);
  state.selectedDetailsId = "";
  await refresh();
}

async function storeSshSudoPassword(instance) {
  if (instance.source !== "ssh_managed") return;
  const password = await promptForPassword(`Sudo-Passwort fuer ${instance.displayName}`);
  if (!password) return;
  const result = await window.ctoxDesktop.storeSshSudoPassword({
    host: instance.connection?.host || "",
    user: instance.connection?.user || "",
    port: instance.connection?.port || 22,
    sudoPassword: password,
  });
  if (result?.sudoPasswordRef) {
    state.sudoPasswordRefs[instance.id] = result.sudoPasswordRef;
    render();
  }
}

async function storeSshLoginPassword(instance) {
  if (instance.source !== "ssh_managed") return;
  const password = await promptForPassword(`SSH-Passwort fuer ${instance.displayName}`);
  if (!password) return;
  const result = await window.ctoxDesktop.storeSshLoginPassword({
    host: instance.connection?.host || "",
    user: instance.connection?.user || "",
    port: instance.connection?.port || 22,
    sshPassword: password,
  });
  if (result?.sshPasswordRef) {
    state.sshPasswordRefs[instance.id] = result.sshPasswordRef;
    render();
  }
}

function promptForPassword(title) {
  return new Promise((resolve) => {
    passwordDialogTitle.textContent = title;
    passwordDialogInput.value = "";
    let settled = false;
    function settle(value) {
      if (settled) return;
      settled = true;
      cleanup();
      if (passwordDialog.open) passwordDialog.close();
      resolve(value);
    }
    function cleanup() {
      passwordForm.removeEventListener("submit", onSubmit);
      passwordDialogCancel.removeEventListener("click", onCancel);
      passwordDialog.removeEventListener("cancel", onCancel);
    }
    function onSubmit(event) {
      event.preventDefault();
      settle(passwordDialogInput.value);
    }
    function onCancel(event) {
      event.preventDefault();
      settle("");
    }
    passwordForm.addEventListener("submit", onSubmit);
    passwordDialogCancel.addEventListener("click", onCancel);
    passwordDialog.addEventListener("cancel", onCancel);
    passwordDialog.showModal();
    passwordDialogInput.focus();
  });
}

function renderBadges(container, badges) {
  container.replaceChildren();
  for (const badge of badges) {
    const element = document.createElement("span");
    element.className = `badge badge-${badge.kind} badge-${badge.tone}`;
    element.textContent = badge.label;
    element.title = badge.title;
    container.appendChild(element);
  }
}

function instanceMeta(instance) {
  return [
    instance.domain,
    instance.instanceId,
    instance.connection?.host,
  ].filter(Boolean).join(" · ");
}

function sourceLabel(source) {
  return {
    ctox_dev: "ctox.dev",
    local_daemon: "lokal",
    ssh_managed: "ssh",
    pairing_invite: "pairing",
  }[source] || source || "";
}

refresh().catch((error) => {
  list.textContent = error instanceof Error ? error.message : String(error);
});
