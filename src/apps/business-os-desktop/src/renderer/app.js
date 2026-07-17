"use strict";

const state = {
  appInfo: null,
  instances: [],
  query: "",
  activeInstanceId: "",
  selectedDetailsId: "",
  switcherOpen: false,
  connectionOpen: false,
  connectionTab: "ctox_dev",
  connectionBusy: false,
  ctoxDevLoginBusy: false,
  sshHostKey: null,
  pairingRotationTarget: null,
  sudoPasswordRefs: {},
  sshPasswordRefs: {},
};

const list = document.getElementById("instances");
const search = document.getElementById("search");
const openSwitcherButton = document.getElementById("open-switcher");
const connectInstanceButton = document.getElementById("connect-instance");
const emptyConnectInstanceButton = document.getElementById("empty-connect-instance");
const closeSwitcherButton = document.getElementById("close-switcher");
const switcherBackdrop = document.getElementById("switcher-backdrop");
const switcherCount = document.getElementById("switcher-count");
const currentInstanceName = document.getElementById("current-instance-name");
const currentInstanceMeta = document.getElementById("current-instance-meta");
const loginButton = document.getElementById("login-ctox-dev");
const logoutButton = document.getElementById("logout-ctox-dev");
const choosePeerToPeerButton = document.getElementById("choose-peer-to-peer");
const emptyState = document.getElementById("empty-state");
const connectionPanel = document.getElementById("connection-panel");
const connectionTitle = document.getElementById("connection-title");
const connectionSubtitle = document.getElementById("connection-subtitle");
const closeConnectionButton = document.getElementById("close-connection");
const connectionStatus = document.getElementById("connection-status");
const connectionTabs = Array.from(document.querySelectorAll("[data-connection-tab]"));
const connectionPanes = Array.from(document.querySelectorAll("[data-connection-pane]"));
const localForm = document.getElementById("connection-local");
const localOperation = document.getElementById("local-operation");
const localFormCopy = document.getElementById("local-form-copy");
const localInspection = document.getElementById("local-inspection");
const inspectLocalButton = document.getElementById("inspect-local");
const attachLocalButton = document.getElementById("attach-local");
const localRootField = document.getElementById("local-root-field");
const localBinaryField = document.getElementById("local-binary-field");
const sshForm = document.getElementById("connection-ssh");
const sshOperation = document.getElementById("ssh-operation");
const sshFormCopy = document.getElementById("ssh-form-copy");
const inspectSshButton = document.getElementById("inspect-ssh");
const attachSshButton = document.getElementById("attach-ssh");
const sshHostKeyResult = document.getElementById("ssh-host-key");
const sshHostKeyConfirmation = document.getElementById("ssh-host-key-confirmation");
const sshHostKeyTrusted = document.getElementById("ssh-host-key-trusted");
const inviteForm = document.getElementById("connection-invite");
const inviteFormTitle = document.getElementById("invite-form-title");
const invitePayload = document.getElementById("invite-payload");
const manualPairingFields = document.getElementById("manual-pairing-fields");
const importInviteButton = document.getElementById("import-invite");
const cancelInviteRotationButton = document.getElementById("cancel-invite-rotation");
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
const appVersion = document.getElementById("app-version");

window.ctoxDesktop.onOpenSwitcher?.(() => {
  openSwitcher({ focus: true }).catch((error) => console.error("open switcher failed", error));
});
openSwitcherButton.addEventListener("click", () => openSwitcher());
connectInstanceButton.addEventListener("click", () => openConnection("ctox_dev"));
emptyConnectInstanceButton.addEventListener("click", () => openConnection("ctox_dev"));
closeSwitcherButton.addEventListener("click", () => closeSwitcher());
switcherBackdrop.addEventListener("click", (event) => {
  if (event.target === switcherBackdrop) closeSwitcher();
});
loginButton.addEventListener("click", loginCtoxDev);
logoutButton.addEventListener("click", logoutCtoxDev);
choosePeerToPeerButton.addEventListener("click", () => selectConnectionTab("invite"));
closeConnectionButton.addEventListener("click", closeConnection);
for (const tab of connectionTabs) {
  tab.addEventListener("click", () => selectConnectionTab(tab.dataset.connectionTab));
  tab.addEventListener("keydown", (event) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const current = connectionTabs.indexOf(tab);
    const next = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? connectionTabs.length - 1
        : (current + (event.key === 'ArrowRight' ? 1 : -1) + connectionTabs.length) % connectionTabs.length;
    selectConnectionTab(connectionTabs[next].dataset.connectionTab);
  });
}
inspectLocalButton.addEventListener("click", inspectLocalConnection);
localOperation.addEventListener("change", renderConnectionState);
localForm.addEventListener("submit", attachLocalConnection);
inspectSshButton.addEventListener("click", inspectSshConnection);
sshOperation.addEventListener("change", renderConnectionState);
sshForm.addEventListener("input", resetSshTrustIfEndpointChanged);
sshHostKeyTrusted.addEventListener("change", renderConnectionState);
sshForm.addEventListener("submit", attachSshConnection);
inviteForm.addEventListener("submit", importInviteConnection);
cancelInviteRotationButton.addEventListener("click", cancelInviteRotation);
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
  } else if (event.key === "Escape" && state.connectionOpen) {
    event.preventDefault();
    closeConnection();
  }
});

async function refresh() {
  if (!state.appInfo && window.ctoxDesktop.getAppInfo) {
    state.appInfo = await window.ctoxDesktop.getAppInfo();
    const version = String(state.appInfo?.version || "").trim();
    appVersion.textContent = version ? `v${version}` : "Version unbekannt";
    document.title = version
      ? `CTOX Business OS Desktop Beta v${version}`
      : "CTOX Business OS Desktop Beta";
  }
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
  const hasManagedAccount = state.instances.some((instance) => instance.source === "ctox_dev");
  loginButton.hidden = hasManagedAccount;
  logoutButton.hidden = !hasManagedAccount;
  emptyState.hidden = state.instances.length > 0 || Boolean(state.selectedDetailsId) || state.connectionOpen;
  connectionPanel.hidden = !state.connectionOpen;
  renderConnectionState();
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
  if (!instance || state.connectionOpen) {
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
    settingsActions.appendChild(actionButton("Pairing rotieren", () => beginPairingRotation(instance), "rotate-pairing-instance"));
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
  state.connectionOpen = false;
  render();
}

async function loginCtoxDev() {
  if (state.ctoxDevLoginBusy) return;
  state.ctoxDevLoginBusy = true;
  renderConnectionState();
  setConnectionStatus("ctox.dev-Anmeldung ist in einem separaten Fenster geöffnet. Peer2Peer bleibt hier auswählbar.", "warning");
  let result;
  try {
    result = await window.ctoxDesktop.loginCtoxDev();
  } catch (error) {
    setConnectionStatus(error instanceof Error ? error.message : String(error), "error");
    return;
  } finally {
    state.ctoxDevLoginBusy = false;
    renderConnectionState();
  }
  if (Array.isArray(result?.instances)) {
    setInstances(result.instances);
    render();
    if (result.completed && state.instances.length > 0) await openSwitcher();
    else if (!result.completed) setConnectionStatus("Anmeldung beendet. Du kannst ctox.dev erneut öffnen oder Peer2Peer wählen.", "warning");
    return;
  }
  await refresh();
  if (result?.completed && state.instances.length > 0) await openSwitcher();
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
  closeConnection({ renderAfter: false });
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

async function openConnection(tab = "local", options = {}) {
  if (state.switcherOpen) await closeSwitcher();
  await window.ctoxDesktop.showAppShell?.();
  state.connectionOpen = true;
  state.selectedDetailsId = "";
  state.activeInstanceId = "";
  state.pairingRotationTarget = options.pairingRotationTarget || null;
  clearConnectionStatus();
  selectConnectionTab(tab, { focus: false });
  render();
  document.querySelector(".content").scrollTop = 0;
  const firstInput = connectionPanel.querySelector(`[data-connection-pane="${state.connectionTab}"] input, [data-connection-pane="${state.connectionTab}"] textarea`);
  firstInput?.focus();
}

function closeConnection(options = {}) {
  state.connectionOpen = false;
  state.connectionBusy = false;
  state.sshHostKey = null;
  state.pairingRotationTarget = null;
  sshHostKeyTrusted.checked = false;
  invitePayload.value = "";
  clearConnectionStatus();
  if (options.renderAfter !== false) render();
}

function selectConnectionTab(tab, options = {}) {
  if (!['ctox_dev', 'invite', 'local', 'ssh'].includes(tab)) return;
  state.connectionTab = tab;
  clearConnectionStatus();
  renderConnectionState();
  if (options.focus !== false) {
    connectionTabs.find((entry) => entry.dataset.connectionTab === tab)?.focus();
  }
}

function renderConnectionState() {
  for (const tab of connectionTabs) {
    const selected = tab.dataset.connectionTab === state.connectionTab;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  }
  for (const pane of connectionPanes) {
    pane.hidden = pane.dataset.connectionPane !== state.connectionTab;
  }
  connectionTitle.textContent = state.pairingRotationTarget ? "Pairing rotieren" : "Instanz verbinden";
  connectionSubtitle.textContent = state.pairingRotationTarget
    ? `Neue Einladung für ${state.pairingRotationTarget.displayName} importieren.`
    : "Wähle den Zugang zu deiner CTOX-Instanz.";
  inviteFormTitle.textContent = state.pairingRotationTarget ? "Neue Einladung importieren" : "Peer2Peer verbinden";
  importInviteButton.textContent = state.pairingRotationTarget ? "Pairing rotieren" : "Verbinden";
  cancelInviteRotationButton.hidden = !state.pairingRotationTarget;
  manualPairingFields.hidden = Boolean(state.pairingRotationTarget);
  sshHostKeyResult.hidden = !state.sshHostKey;
  sshHostKeyConfirmation.hidden = !state.sshHostKey;
  attachSshButton.disabled = state.connectionBusy || !state.sshHostKey || !sshHostKeyTrusted.checked;
  const installingLocal = localOperation.value === "install";
  inspectLocalButton.hidden = installingLocal;
  localRootField.hidden = installingLocal;
  localBinaryField.hidden = installingLocal;
  attachLocalButton.textContent = installingLocal ? "CTOX installieren" : "Lokal verbinden";
  localFormCopy.textContent = installingLocal
    ? "Führt den offiziellen CTOX-Installer aus, startet den lokalen Peer und fügt die Instanz anschließend hinzu."
    : "Prüft eine vorhandene lokale CTOX-Installation und fügt sie der Desktop-App hinzu.";
  const sshAction = sshOperation.value;
  attachSshButton.textContent = sshAction === "install"
    ? "CTOX per SSH installieren"
    : sshAction === "upgrade"
      ? "CTOX per SSH aktualisieren"
      : "Vorhandenes CTOX verbinden";
  sshFormCopy.textContent = sshAction === "install"
    ? "Installiert CTOX stabil auf dem SSH-Rechner, startet den Peer und fügt die Instanz hinzu."
    : sshAction === "upgrade"
      ? "Aktualisiert die vorhandene CTOX-Installation per SSH und verbindet sie anschließend."
      : "Verbindet eine vorhandene CTOX-Installation. Der Hostschlüssel wird vorher geprüft.";
  for (const control of connectionPanel.querySelectorAll("button, input, select, textarea")) {
    if (control === closeConnectionButton) continue;
    if (control === attachSshButton) continue;
    if (control === loginButton) {
      control.disabled = state.connectionBusy || state.ctoxDevLoginBusy;
      continue;
    }
    control.disabled = state.connectionBusy;
  }
}

function localOptions() {
  return compactOptions({
    displayName: document.getElementById("local-display-name").value,
    ctoxRoot: document.getElementById("local-root").value,
    ctoxBinary: document.getElementById("local-binary").value,
  });
}

async function inspectLocalConnection() {
  await runConnectionTask("Lokale Verbindung wird geprüft …", async () => {
    const result = await window.ctoxDesktop.inspectLocalDaemon(localOptions());
    localInspection.hidden = false;
    localInspection.dataset.tone = result.status === "available" ? "success" : "warning";
    localInspection.textContent = result.status === "available"
      ? `Bereit: ${result.instanceId || "CTOX"} über ${result.dataPlane || "rxdb-webrtc"}`
      : result.message || `Status: ${result.status || "unbekannt"}`;
    if (result.status !== "available") throw new Error(result.message || "Der lokale CTOX-Daemon ist nicht bereit.");
    clearConnectionStatus();
  });
}

async function attachLocalConnection(event) {
  event.preventDefault();
  const installing = localOperation.value === "install";
  await runConnectionTask(installing ? "CTOX wird auf diesem Rechner installiert …" : "Lokale Instanz wird verbunden …", async () => {
    if (installing) {
      await window.ctoxDesktop.installLocalCtox(localOptions());
    } else {
      await window.ctoxDesktop.attachLocalDaemon(localOptions());
    }
    await finishConnection(installing ? "CTOX wurde installiert und verbunden." : "Lokale Instanz verbunden.");
  });
}

function sshOptions() {
  return compactOptions({
    displayName: document.getElementById("ssh-display-name").value,
    host: document.getElementById("ssh-host").value,
    user: document.getElementById("ssh-user").value,
    port: Number(document.getElementById("ssh-port").value || 22),
  });
}

async function inspectSshConnection() {
  if (!sshForm.reportValidity()) return;
  await runConnectionTask("SSH-Hostschlüssel wird gelesen …", async () => {
    const options = sshOptions();
    const hostKey = await window.ctoxDesktop.inspectSshHostKey(options);
    state.sshHostKey = { ...hostKey, host: options.host, user: options.user, port: options.port };
    sshHostKeyTrusted.checked = false;
    sshHostKeyResult.hidden = false;
    sshHostKeyResult.dataset.tone = "warning";
    sshHostKeyResult.replaceChildren();
    const label = document.createElement("strong");
    label.textContent = `${hostKey.algorithm || hostKey.keyType || "SSH"} Fingerprint`;
    const value = document.createElement("code");
    value.textContent = hostKey.fingerprint || "Fingerprint fehlt";
    sshHostKeyResult.append(label, value);
    setConnectionStatus("Vergleiche den Fingerprint mit einer vertrauenswürdigen Quelle.", "warning");
  });
}

function resetSshTrustIfEndpointChanged() {
  if (!state.sshHostKey) return;
  const options = sshOptions();
  if (options.host === state.sshHostKey.host && options.user === state.sshHostKey.user && options.port === state.sshHostKey.port) return;
  state.sshHostKey = null;
  sshHostKeyTrusted.checked = false;
  clearConnectionStatus();
  renderConnectionState();
}

async function attachSshConnection(event) {
  event.preventDefault();
  if (!sshForm.reportValidity()) return;
  const options = sshOptions();
  if (!state.sshHostKey || !sshHostKeyTrusted.checked) {
    setConnectionStatus("Prüfe und bestätige zuerst den Host-Fingerprint.", "error");
    return;
  }
  await runConnectionTask("SSH-Verbindung wird vorbereitet …", async () => {
    const sshPassword = document.getElementById("ssh-password").value;
    const sudoPassword = document.getElementById("ssh-sudo-password").value;
    if (sshPassword) {
      const stored = await window.ctoxDesktop.storeSshLoginPassword({ ...options, sshPassword });
      options.sshPasswordRef = stored.sshPasswordRef;
    }
    if (sudoPassword) {
      const stored = await window.ctoxDesktop.storeSshSudoPassword({ ...options, sudoPassword });
      options.sudoPasswordRef = stored.sudoPasswordRef;
    }
    options.trustedHostKeyFingerprint = state.sshHostKey.fingerprint;
    const preflight = await window.ctoxDesktop.preflightSshManaged(options);
    if (!preflight?.sshReachable) throw new Error("Der SSH-Host ist nicht erreichbar.");
    const operation = sshOperation.value;
    if (operation === "attach") {
      await window.ctoxDesktop.attachSshManaged(options);
    } else {
      await window.ctoxDesktop.installSshManaged({
        ...options,
        freshInstall: operation === "install",
        releaseChannel: "stable",
      });
    }
    document.getElementById("ssh-password").value = "";
    document.getElementById("ssh-sudo-password").value = "";
    await finishConnection("SSH-Instanz verbunden.");
  });
}

async function importInviteConnection(event) {
  event.preventDefault();
  const rawInvite = invitePayload.value.trim();
  const manual = compactOptions({
    displayName: document.getElementById("pairing-display-name").value,
    syncRoom: document.getElementById("pairing-sync-room").value,
    signalingUrl: document.getElementById("pairing-signaling-url").value,
    roomSecret: document.getElementById("pairing-room-secret").value,
    capabilityToken: document.getElementById("pairing-capability-token").value,
  });
  if (!rawInvite && state.pairingRotationTarget) {
    setConnectionStatus("Füge für die Rotation eine vollständige Desktop-Einladung ein.", "error");
    return;
  }
  if (!rawInvite && (!manual.displayName || !manual.syncRoom || !manual.signalingUrl || !manual.roomSecret || !manual.capabilityToken)) {
    manualPairingFields.open = true;
    setConnectionStatus("Gib Einladung oder Anzeigename, Signaling-URL, Sync Room, Room-Passwort und Zugangs-Token vollständig an.", "error");
    return;
  }
  await runConnectionTask(state.pairingRotationTarget ? "Pairing wird rotiert …" : "Einladung wird importiert …", async () => {
    if (state.pairingRotationTarget) {
      await window.ctoxDesktop.rotatePairing(state.pairingRotationTarget, rawInvite);
    } else if (rawInvite) {
      await window.ctoxDesktop.importInvite(rawInvite);
    } else {
      await window.ctoxDesktop.importManualPairing(manual);
    }
    document.getElementById("pairing-room-secret").value = "";
    document.getElementById("pairing-capability-token").value = "";
    await finishConnection(state.pairingRotationTarget ? "Pairing rotiert." : "Einladung importiert.");
  });
}

function beginPairingRotation(instance) {
  openConnection("invite", { pairingRotationTarget: instance });
}

function cancelInviteRotation() {
  state.pairingRotationTarget = null;
  invitePayload.value = "";
  renderConnectionState();
}

async function finishConnection(message) {
  await refresh();
  state.connectionBusy = false;
  state.connectionOpen = false;
  state.pairingRotationTarget = null;
  state.sshHostKey = null;
  setConnectionStatus(message, "success");
  render();
  if (state.instances.length > 0) await openSwitcher();
}

async function runConnectionTask(pendingMessage, operation) {
  if (state.connectionBusy) return;
  state.connectionBusy = true;
  setConnectionStatus(pendingMessage, "progress");
  renderConnectionState();
  try {
    await operation();
  } catch (error) {
    setConnectionStatus(error instanceof Error ? error.message : String(error), "error");
  } finally {
    state.connectionBusy = false;
    renderConnectionState();
  }
}

function setConnectionStatus(message, tone) {
  connectionStatus.hidden = false;
  connectionStatus.dataset.tone = tone || "neutral";
  connectionStatus.textContent = message;
}

function clearConnectionStatus() {
  connectionStatus.hidden = true;
  connectionStatus.textContent = "";
  delete connectionStatus.dataset.tone;
}

function compactOptions(values) {
  return Object.fromEntries(Object.entries(values).filter(([, value]) => value !== "" && value !== undefined && value !== null));
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
