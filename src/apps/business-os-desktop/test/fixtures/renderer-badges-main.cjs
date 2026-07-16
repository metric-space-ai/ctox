"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { app, BrowserWindow } = require("electron");

const outputPath = process.argv[2];
const userDataPath = process.argv[3];
const screenshotPath = outputPath ? path.join(path.dirname(outputPath), "connection-flow.png") : "";

if (!outputPath || !userDataPath) {
  throw new Error("usage: electron renderer-badges-main.cjs <outputPath> <userDataPath>");
}

fs.mkdirSync(userDataPath, { recursive: true });
app.setPath("userData", userDataPath);
app.commandLine.appendSwitch("disable-gpu");

app.whenReady().then(async () => {
  const consoleMessages = [];
  const window = new BrowserWindow({
    show: false,
    width: 900,
    height: 720,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, "renderer-badges-preload.cjs"),
    },
  });
  window.webContents.on("console-message", (details) => {
    consoleMessages.push({
      level: details.level,
      message: details.message,
      line: details.lineNumber,
      sourceId: details.sourceId,
    });
  });
  try {
    await window.loadFile(path.join(__dirname, "../../src/renderer/index.html"));
    await waitForDom(window, "document.querySelectorAll('.instance').length === 3");
    const initial = await readSidebar(window);
    await window.webContents.executeJavaScript(`
      document.getElementById("connect-instance").click();
    `, true);
    await waitForDom(window, "!document.getElementById('connection-panel').hidden && !document.getElementById('connection-ctox-dev').hidden");
    await window.webContents.executeJavaScript(`document.getElementById("login-ctox-dev").click()`, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.ctoxDevLoginRequests().length === 1");
    await window.webContents.executeJavaScript(`document.getElementById("choose-peer-to-peer").click()`, true);
    await waitForDom(window, "!document.getElementById('connection-invite').hidden");
    const connectionChoice = await window.webContents.executeJavaScript(`({
      peerToPeerVisible: !document.getElementById("connection-invite").hidden,
      ctoxDevLoginRequests: window.ctoxDesktopSmoke.ctoxDevLoginRequests().length,
      peerTabLabel: document.getElementById("connection-tab-invite").textContent
    })`, true);
    await window.webContents.executeJavaScript(`document.getElementById("connection-tab-local").click()`, true);
    await waitForDom(window, "!document.getElementById('connection-local').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("local-operation").value = "attach";
        document.getElementById("local-operation").dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("local-display-name").value = "Local Smoke";
        document.getElementById("local-root").value = "/tmp/ctox-local-smoke";
        document.getElementById("inspect-local").click();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.localInspectRequests().length === 1 && !document.getElementById('local-inspection').hidden");
    const localInspectionText = await window.webContents.executeJavaScript(`document.getElementById("local-inspection").textContent`, true);
    await window.webContents.executeJavaScript(`document.getElementById("connection-local").requestSubmit()`, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.localAttachRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`document.getElementById("close-switcher").click()`, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");

    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("connect-instance").click();
        document.getElementById("connection-tab-local").click();
        document.getElementById("local-operation").value = "install";
        document.getElementById("local-operation").dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("local-display-name").value = "Installed Local";
      })();
    `, true);
    const localInstallFieldsHidden = await window.webContents.executeJavaScript(`
      document.getElementById("local-root-field").hidden
        && document.getElementById("local-binary-field").hidden
        && getComputedStyle(document.getElementById("local-root-field")).display === "none"
        && getComputedStyle(document.getElementById("local-binary-field")).display === "none"
    `, true);
    await window.webContents.executeJavaScript(`document.getElementById("connection-local").requestSubmit()`, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.localInstallRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    const localInstallButtonText = await window.webContents.executeJavaScript(`document.getElementById("attach-local").textContent`, true);
    await window.webContents.executeJavaScript(`document.getElementById("close-switcher").click()`, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");

    await window.webContents.executeJavaScript(`document.getElementById("connect-instance").click()`, true);
    await waitForDom(window, "!document.getElementById('connection-panel').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("connection-tab-ssh").click();
        document.getElementById("ssh-operation").value = "attach";
        document.getElementById("ssh-operation").dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("ssh-display-name").value = "SSH Smoke";
        document.getElementById("ssh-host").value = "ssh-smoke.example.com";
        document.getElementById("ssh-user").value = "ubuntu";
      })();
    `, true);
    const sshConfirmationInitiallyHidden = await window.webContents.executeJavaScript(`
      document.getElementById("ssh-host-key-confirmation").hidden
        && getComputedStyle(document.getElementById("ssh-host-key-confirmation")).display === "none"
    `, true);
    await window.webContents.executeJavaScript(`document.getElementById("inspect-ssh").click()`, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sshHostKeyRequests().length === 1 && !document.getElementById('ssh-host-key').hidden");
    const sshFingerprintText = await window.webContents.executeJavaScript(`document.getElementById("ssh-host-key").textContent`, true);
    await window.webContents.executeJavaScript(`new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`, true);
    fs.writeFileSync(screenshotPath, (await window.webContents.capturePage()).toPNG());
    await window.webContents.executeJavaScript(`
      (() => {
        const trusted = document.getElementById("ssh-host-key-trusted");
        trusted.checked = true;
        trusted.dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("connection-ssh").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sshAttachRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`document.getElementById("close-switcher").click()`, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");

    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("connect-instance").click();
        document.getElementById("connection-tab-ssh").click();
        document.getElementById("ssh-operation").value = "install";
        document.getElementById("ssh-operation").dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("ssh-display-name").value = "Installed Remote";
        document.getElementById("ssh-host").value = "install-smoke.example.com";
        document.getElementById("ssh-user").value = "ubuntu";
        document.getElementById("inspect-ssh").click();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sshHostKeyRequests().length === 2 && !document.getElementById('ssh-host-key').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        const trusted = document.getElementById("ssh-host-key-trusted");
        trusted.checked = true;
        trusted.dispatchEvent(new Event("change", { bubbles: true }));
        document.getElementById("connection-ssh").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sshInstallRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    const sshInstallButtonText = await window.webContents.executeJavaScript(`document.getElementById("attach-ssh").textContent`, true);
    await window.webContents.executeJavaScript(`document.getElementById("close-switcher").click()`, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");

    await window.webContents.executeJavaScript(`document.getElementById("connect-instance").click()`, true);
    await waitForDom(window, "!document.getElementById('connection-panel').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("connection-tab-invite").click();
        document.getElementById("invite-payload").value = "ctox-business-os-desktop://pair?payload=desktop-smoke";
        document.getElementById("connection-invite").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.inviteImportRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`document.getElementById("close-switcher").click()`, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("connect-instance").click();
        document.getElementById("connection-tab-invite").click();
        document.getElementById("manual-pairing-fields").open = true;
        document.getElementById("pairing-display-name").value = "Manual P2P";
        document.getElementById("pairing-signaling-url").value = "wss://signaling.ctox.dev";
        document.getElementById("pairing-sync-room").value = "ctox-business-os:manual:room";
        document.getElementById("pairing-room-secret").value = "manual-room-secret";
        document.getElementById("pairing-capability-token").value = "manual-capability-token";
        document.getElementById("connection-invite").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.manualPairingRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`
      document.querySelector(".instance-main").click();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.activateRequests().length === 1");
    const quickSwitchFocused = await window.webContents.executeJavaScript(`
      (() => {
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
        return document.activeElement === document.getElementById("search")
          && document.getElementById("switcher-backdrop").hidden === false;
      })()
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        const searchInput = document.getElementById("search");
        searchInput.value = "remote";
        searchInput.dispatchEvent(new Event("input", { bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 1");
    await window.webContents.executeJavaScript(`
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.activateRequests().length === 2");
    await window.webContents.executeJavaScript(`
      (() => {
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
        const searchInput = document.getElementById("search");
        searchInput.value = "paired";
        searchInput.dispatchEvent(new Event("input", { bubbles: true }));
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.activateRequests().length === 3");
    const activateRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.activateRequests()
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        const searchInput = document.getElementById("search");
        searchInput.value = "";
        searchInput.dispatchEvent(new Event("input", { bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 3");
    await window.webContents.executeJavaScript(`
      document.querySelector(".details-instance").click();
    `, true);
    await waitForDom(window, "!document.getElementById('instance-settings').hidden");
    const managedSettings = await readSettings(window);
    await window.webContents.executeJavaScript(`
      document.querySelector(".manage-selected-instance").click();
    `, true);
    const manageRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.manageRequests()
    `, true);
    await window.webContents.executeJavaScript(`
      document.getElementById("logout-ctox-dev").click();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 2");
    const afterLogout = await readSidebar(window);
    await window.webContents.executeJavaScript(`
      (() => {
        const searchInput = document.getElementById("search");
        searchInput.value = "sync pending";
        searchInput.dispatchEvent(new Event("input", { bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 1");
    const filtered = await readSidebar(window);
    await window.webContents.executeJavaScript(`
      document.querySelector(".details-instance").click();
    `, true);
    await waitForDom(window, "!document.getElementById('instance-settings').hidden");
    const unmanagedSettings = await readSettings(window);
    await window.webContents.executeJavaScript(`
      document.querySelector(".store-ssh-password").click();
    `, true);
    await waitForDom(window, "document.getElementById('password-dialog').open");
    const sshPasswordInputType = await window.webContents.executeJavaScript(`
      document.getElementById("password-dialog-input").type
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("password-dialog-input").value = "login-secret";
        document.getElementById("password-form").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sshPasswordRequests().length === 1 && !document.getElementById('password-dialog').open");
    const sshPasswordRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.sshPasswordRequests()
    `, true);
    const unmanagedSettingsAfterSsh = await readSettings(window);
    await window.webContents.executeJavaScript(`
      document.querySelector(".store-sudo-password").click();
    `, true);
    await waitForDom(window, "document.getElementById('password-dialog').open");
    const passwordInputType = await window.webContents.executeJavaScript(`
      document.getElementById("password-dialog-input").type
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("password-dialog-input").value = "sudo-secret";
        document.getElementById("password-form").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.sudoPasswordRequests().length === 1 && !document.getElementById('password-dialog').open");
    const sudoPasswordRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.sudoPasswordRequests()
    `, true);
    const unmanagedSettingsAfterSudo = await readSettings(window);
    await window.webContents.executeJavaScript(`
      window.confirm = () => true;
      document.querySelector(".remove-selected-instance").click();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 0");
    const removeRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.removeRequests()
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        const searchInput = document.getElementById("search");
        searchInput.value = "";
        searchInput.dispatchEvent(new Event("input", { bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 1");
    await window.webContents.executeJavaScript(`
      document.querySelector(".details-instance").click();
    `, true);
    await waitForDom(window, "!document.getElementById('instance-settings').hidden");
    const pairingSettings = await readSettings(window);
    await window.webContents.executeJavaScript(`
      document.querySelector(".rotate-pairing-instance").click();
    `, true);
    await waitForDom(window, "!document.getElementById('connection-panel').hidden && !document.getElementById('connection-invite').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("invite-payload").value = JSON.stringify({
          type: "ctox-business-os-invite",
          version: 1,
          display_name: "Paired Lab",
          instance_id: "paired_lab",
          sync_room: "ctox-business-os:paired_lab:room",
          signaling_urls: ["wss://signaling.ctox.dev"],
          signaling_room_password: "rotated-room-secret",
          transport: "webrtc",
          expires_at: "2099-01-01T00:00:00.000Z"
        });
        document.getElementById("connection-invite").requestSubmit();
      })();
    `, true);
    await waitForDom(window, "window.ctoxDesktopSmoke.rotateRequests().length === 1 && !document.getElementById('switcher-backdrop').hidden");
    const rotateRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.rotateRequests()
    `, true);
    await window.webContents.executeJavaScript(`
      (() => {
        document.getElementById("close-switcher").click();
      })();
    `, true);
    await waitForDom(window, "document.getElementById('switcher-backdrop').hidden");
    await window.webContents.executeJavaScript(`
      (() => {
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
      })();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 1");
    await window.webContents.executeJavaScript(`document.querySelector(".details-instance").click()`, true);
    await waitForDom(window, "!document.getElementById('instance-settings').hidden");
    await window.webContents.executeJavaScript(`
      window.confirm = () => true;
      document.querySelector(".revoke-pairing-instance").click();
    `, true);
    await waitForDom(window, "document.querySelectorAll('.instance').length === 0");
    const revokeRequests = await window.webContents.executeJavaScript(`
      window.ctoxDesktopSmoke.revokeRequests()
    `, true);
    const connectionRequests = await window.webContents.executeJavaScript(`
      ({
        localInspect: window.ctoxDesktopSmoke.localInspectRequests(),
        localAttach: window.ctoxDesktopSmoke.localAttachRequests(),
        localInstall: window.ctoxDesktopSmoke.localInstallRequests(),
        sshHostKey: window.ctoxDesktopSmoke.sshHostKeyRequests(),
        sshPreflight: window.ctoxDesktopSmoke.sshPreflightRequests(),
        sshAttach: window.ctoxDesktopSmoke.sshAttachRequests(),
        sshInstall: window.ctoxDesktopSmoke.sshInstallRequests(),
        inviteImport: window.ctoxDesktopSmoke.inviteImportRequests(),
        manualPairing: window.ctoxDesktopSmoke.manualPairingRequests(),
      })
    `, true);
    const result = {
      ok: initial.length === 3
        && connectionChoice.peerToPeerVisible === true
        && connectionChoice.ctoxDevLoginRequests === 1
        && connectionChoice.peerTabLabel === "Peer2Peer"
        && localInspectionText.includes("rxdb-webrtc")
        && sshFingerprintText.includes("SHA256:desktop-smoke-host-key")
        && connectionRequests.localInspect.length === 1
        && connectionRequests.localAttach.length === 1
        && connectionRequests.localInstall.length === 1
        && localInstallButtonText === "CTOX installieren"
        && localInstallFieldsHidden === true
        && connectionRequests.localAttach[0].ctoxRoot === "/tmp/ctox-local-smoke"
        && connectionRequests.sshHostKey.length === 2
        && connectionRequests.sshPreflight.length === 2
        && connectionRequests.sshAttach.length === 1
        && connectionRequests.sshInstall.length === 1
        && connectionRequests.sshInstall[0].freshInstall === true
        && sshInstallButtonText === "CTOX per SSH installieren"
        && sshConfirmationInitiallyHidden === true
        && connectionRequests.sshAttach[0].trustedHostKeyFingerprint === "SHA256:desktop-smoke-host-key"
        && connectionRequests.inviteImport.length === 1
        && connectionRequests.manualPairing.length === 1
        && connectionRequests.manualPairing[0].syncRoom === "ctox-business-os:manual:room"
        && connectionRequests.manualPairing[0].roomSecretLength === 18
        && !JSON.stringify(connectionRequests).includes("login-secret")
        && !JSON.stringify(connectionRequests).includes("sudo-secret")
        && !JSON.stringify(connectionRequests).includes("manual-room-secret")
        && quickSwitchFocused === true
        && activateRequests.map((request) => request.source).join("|") === "ctox_dev|ssh_managed|pairing_invite"
        && activateRequests.every((request) => request.dataPlane === "rxdb-webrtc")
        && activateRequests.every((request) => request.httpDataProxy === false)
        && initial[0].badges.join("|") === "ctox.dev|owner|online|rxdb"
        && initial[0].actions.join("|") === "Details|Verwalten"
        && initial[1].actions.join("|") === "Details"
        && initial[1].badges.join("|") === "ssh|offline|sync pending"
        && initial[2].actions.join("|") === "Details"
        && initial[2].badges.join("|") === "paired|online|rxdb"
        && managedSettings.name === "SKF"
        && managedSettings.actions.join("|") === "In ctox.dev verwalten"
        && managedSettings.fields["Quelle"] === "ctox.dev"
        && !managedSettings.actions.includes("Aus App entfernen")
        && manageRequests.length === 1
        && manageRequests[0].id === "managed:tenant_skf"
        && afterLogout.length === 2
        && afterLogout[0].name === "Remote VPS"
        && afterLogout[1].name === "Paired Lab"
        && filtered.length === 1
        && filtered[0].name === "Remote VPS"
        && unmanagedSettings.name === "Remote VPS"
        && unmanagedSettings.actions.join("|") === "SSH-Passwort speichern|Sudo-Passwort speichern|Aus App entfernen"
        && unmanagedSettings.fields["Quelle"] === "ssh"
        && unmanagedSettings.fields["Host"] === "203.0.113.11"
        && sshPasswordInputType === "password"
        && sshPasswordRequests.length === 1
        && sshPasswordRequests[0].host === "203.0.113.11"
        && sshPasswordRequests[0].user === "ubuntu"
        && sshPasswordRequests[0].passwordLength === 12
        && unmanagedSettingsAfterSsh.fields["SSH Secret"] === "keychain://ctox-business-os-desktop/ssh-login/203.0.113.11"
        && !JSON.stringify(unmanagedSettingsAfterSsh).includes("login-secret")
        && passwordInputType === "password"
        && sudoPasswordRequests.length === 1
        && sudoPasswordRequests[0].host === "203.0.113.11"
        && sudoPasswordRequests[0].user === "ubuntu"
        && sudoPasswordRequests[0].passwordLength === 11
        && unmanagedSettingsAfterSudo.fields["Sudo Secret"] === "keychain://ctox-business-os-desktop/ssh-sudo/203.0.113.11"
        && !JSON.stringify(unmanagedSettingsAfterSudo).includes("sudo-secret")
        && removeRequests.length === 1
        && removeRequests[0].id === "ssh:test"
        && pairingSettings.name === "Paired Lab"
        && pairingSettings.actions.join("|") === "Pairing rotieren|Pairing widerrufen"
        && pairingSettings.fields["Quelle"] === "pairing"
        && rotateRequests.length === 1
        && rotateRequests[0].id === "paired:lab"
        && rotateRequests[0].payloadLength > 0
        && revokeRequests.length === 1
        && revokeRequests[0].id === "paired:lab",
      initial,
      connectionChoice,
      localInspectionText,
      localInstallButtonText,
      localInstallFieldsHidden,
      sshFingerprintText,
      sshInstallButtonText,
      sshConfirmationInitiallyHidden,
      connectionRequests,
      screenshotPath,
      activateRequests,
      quickSwitchFocused,
      afterLogout,
      filtered,
      managedSettings,
      unmanagedSettings,
      unmanagedSettingsAfterSsh,
      unmanagedSettingsAfterSudo,
      pairingSettings,
      manageRequests,
      removeRequests,
      rotateRequests,
      revokeRequests,
      sudoPasswordRequests,
      sshPasswordRequests,
      passwordInputType,
      sshPasswordInputType,
      consoleMessages,
    };
    writeResult(result);
    process.exit(result.ok ? 0 : 2);
  } catch (error) {
    writeResult({
      ok: false,
      error: error instanceof Error ? error.stack || error.message : String(error),
      consoleMessages,
    });
    process.exit(1);
  }
});

function waitForDom(window, conditionScript, timeoutMs = 5000) {
  const startedAt = Date.now();
  return new Promise((resolve, reject) => {
    async function check() {
      try {
        if (await window.webContents.executeJavaScript(`Boolean(${conditionScript})`, true)) {
          resolve();
          return;
        }
      } catch (error) {
        reject(error);
        return;
      }
      if (Date.now() - startedAt > timeoutMs) {
        reject(new Error(`renderer condition timed out: ${conditionScript}`));
        return;
      }
      setTimeout(check, 50);
    }
    check();
  });
}

function readSidebar(window) {
  return window.webContents.executeJavaScript(`
    Array.from(document.querySelectorAll(".instance")).map((button) => ({
      name: button.querySelector(".name").textContent,
      badges: Array.from(button.querySelectorAll(".badge")).map((badge) => badge.textContent),
      actions: Array.from(button.querySelectorAll(".instance-action")).map((action) => action.textContent),
      meta: button.querySelector(".meta").textContent,
    }))
  `, true);
}

function readSettings(window) {
  return window.webContents.executeJavaScript(`
    (() => {
      const fields = {};
      const terms = Array.from(document.querySelectorAll("#settings-fields dt"));
      for (const term of terms) {
        fields[term.textContent] = term.nextElementSibling?.textContent || "";
      }
      return {
        name: document.getElementById("settings-name").textContent,
        meta: document.getElementById("settings-meta").textContent,
        badges: Array.from(document.querySelectorAll("#settings-badges .badge")).map((badge) => badge.textContent),
        actions: Array.from(document.querySelectorAll("#settings-actions button")).map((button) => button.textContent),
        fields,
      };
    })()
  `, true);
}

function writeResult(result) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
}
