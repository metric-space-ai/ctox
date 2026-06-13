"use strict";

const { spawn } = require("node:child_process");

const WINDOWS_CREDENTIAL_MANAGER_SCRIPT = String.raw`
$ErrorActionPreference = 'Stop'
$payloadText = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($payloadText)) {
  throw 'missing credential payload'
}
$payload = $payloadText | ConvertFrom-Json

$source = @"
using System;
using System.Runtime.InteropServices;

public static class CtoxCredentialManager
{
    public const UInt32 CRED_TYPE_GENERIC = 1;
    public const UInt32 CRED_PERSIST_LOCAL_MACHINE = 2;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct FILETIME
    {
        public UInt32 dwLowDateTime;
        public UInt32 dwHighDateTime;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct CREDENTIAL
    {
        public UInt32 Flags;
        public UInt32 Type;
        public string TargetName;
        public string Comment;
        public FILETIME LastWritten;
        public UInt32 CredentialBlobSize;
        public IntPtr CredentialBlob;
        public UInt32 Persist;
        public UInt32 AttributeCount;
        public IntPtr Attributes;
        public string TargetAlias;
        public string UserName;
    }

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool CredWriteW(ref CREDENTIAL credential, UInt32 flags);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool CredReadW(string target, UInt32 type, UInt32 reservedFlag, out IntPtr credentialPtr);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool CredDeleteW(string target, UInt32 type, UInt32 flags);

    [DllImport("advapi32.dll")]
    public static extern void CredFree(IntPtr credentialPtr);
}
"@

Add-Type -TypeDefinition $source

function ThrowLastWin32Error([string]$operation) {
  $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
  throw "$operation failed with Win32 error $code"
}

$target = [string]$payload.target
$action = [string]$payload.action

if ($action -eq 'set') {
  $secret = [string]$payload.secret
  $bytes = [Text.Encoding]::Unicode.GetBytes($secret)
  if ($bytes.Length -gt 5120) {
    throw 'credential secret exceeds Windows Credential Manager blob limit'
  }
  $blob = [Runtime.InteropServices.Marshal]::AllocHGlobal($bytes.Length)
  try {
    [Runtime.InteropServices.Marshal]::Copy($bytes, 0, $blob, $bytes.Length)
    $credential = New-Object CtoxCredentialManager+CREDENTIAL
    $credential.Type = [CtoxCredentialManager]::CRED_TYPE_GENERIC
    $credential.TargetName = $target
    $credential.CredentialBlobSize = [UInt32]$bytes.Length
    $credential.CredentialBlob = $blob
    $credential.Persist = [CtoxCredentialManager]::CRED_PERSIST_LOCAL_MACHINE
    $credential.UserName = [string]$payload.userName
    if (-not [CtoxCredentialManager]::CredWriteW([ref]$credential, 0)) {
      ThrowLastWin32Error 'CredWriteW'
    }
  } finally {
    if ($blob -ne [IntPtr]::Zero) {
      [Runtime.InteropServices.Marshal]::FreeHGlobal($blob)
    }
  }
} elseif ($action -eq 'get') {
  $credentialPtr = [IntPtr]::Zero
  if (-not [CtoxCredentialManager]::CredReadW($target, [CtoxCredentialManager]::CRED_TYPE_GENERIC, 0, [ref]$credentialPtr)) {
    $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($code -eq 1168) {
      exit 0
    }
    throw "CredReadW failed with Win32 error $code"
  }
  try {
    $credential = [Runtime.InteropServices.Marshal]::PtrToStructure($credentialPtr, [type][CtoxCredentialManager+CREDENTIAL])
    if ($credential.CredentialBlobSize -gt 0) {
      $chars = [int]($credential.CredentialBlobSize / 2)
      [Console]::Out.Write([Runtime.InteropServices.Marshal]::PtrToStringUni($credential.CredentialBlob, $chars))
    }
  } finally {
    [CtoxCredentialManager]::CredFree($credentialPtr)
  }
} elseif ($action -eq 'delete') {
  if (-not [CtoxCredentialManager]::CredDeleteW($target, [CtoxCredentialManager]::CRED_TYPE_GENERIC, 0)) {
    $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($code -ne 1168) {
      throw "CredDeleteW failed with Win32 error $code"
    }
  }
} else {
  throw "unsupported credential action: $action"
}
`;

function runCommand(program, args, options = {}) {
  return new Promise((resolve, reject) => {
    const timeoutMs = Number.isFinite(options.timeoutMs) ? options.timeoutMs : 30000;
    let timedOut = false;
    const child = spawn(program, args, {
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, timeoutMs);
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (timedOut) {
        reject(new Error(`${program} timed out after ${timeoutMs}ms`));
        return;
      }
      if (code === 0) {
        resolve({ stdout, stderr });
        return;
      }
      const error = new Error(`${program} exited with code ${code}${stderr ? `: ${stderr.trim()}` : ""}`);
      error.code = code;
      error.stdout = stdout;
      error.stderr = stderr;
      reject(error);
    });
    if (Object.prototype.hasOwnProperty.call(options, "input")) {
      child.stdin.end(String(options.input));
    } else {
      child.stdin.end();
    }
  });
}

class MemorySecretStore {
  constructor() {
    this.values = new Map();
  }

  async get(ref) {
    return this.values.get(String(ref)) || "";
  }

  async set(ref, value) {
    this.values.set(String(ref), String(value));
  }

  async delete(ref) {
    this.values.delete(String(ref));
  }
}

class MacOsKeychainSecretStore {
  constructor({ service = "CTOX Business OS Desktop", runner = runCommand } = {}) {
    this.service = service;
    this.runner = runner;
  }

  async get(ref) {
    const { stdout } = await this.runner("security", [
      "find-generic-password",
      "-a",
      String(ref),
      "-s",
      this.service,
      "-w",
    ]);
    return String(stdout || "").trim();
  }

  async set(ref, value) {
    await this.runner("security", [
      "add-generic-password",
      "-a",
      String(ref),
      "-s",
      this.service,
      "-U",
      "-w",
    ], {
      input: `${String(value)}\n${String(value)}\n`,
      timeoutMs: 120000,
    });
  }

  async delete(ref) {
    await this.runner("security", [
      "delete-generic-password",
      "-a",
      String(ref),
      "-s",
      this.service,
    ]);
  }
}

class LinuxSecretServiceStore {
  constructor({ service = "CTOX Business OS Desktop", appId = "ctox-business-os-desktop", runner = runCommand } = {}) {
    this.service = service;
    this.appId = appId;
    this.runner = runner;
  }

  async get(ref) {
    const { stdout } = await this.runner("secret-tool", [
      "lookup",
      "application",
      this.appId,
      "ref",
      String(ref),
    ]);
    return String(stdout || "").trim();
  }

  async set(ref, value) {
    await this.runner("secret-tool", [
      "store",
      "--label",
      this.service,
      "application",
      this.appId,
      "ref",
      String(ref),
    ], {
      input: `${String(value)}\n`,
    });
  }

  async delete(ref) {
    await this.runner("secret-tool", [
      "clear",
      "application",
      this.appId,
      "ref",
      String(ref),
    ]);
  }
}

class WindowsCredentialManagerStore {
  constructor({ service = "CTOX Business OS Desktop", runner = runCommand } = {}) {
    this.service = service;
    this.runner = runner;
  }

  async get(ref) {
    const { stdout } = await this.run("get", ref);
    return String(stdout || "").trim();
  }

  async set(ref, value) {
    await this.run("set", ref, value);
  }

  async delete(ref) {
    await this.run("delete", ref);
  }

  run(action, ref, secret = "") {
    return this.runner("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      WINDOWS_CREDENTIAL_MANAGER_SCRIPT,
    ], {
      input: JSON.stringify({
        action,
        target: `${this.service}:${String(ref)}`,
        userName: this.service,
        ...(action === "set" ? { secret: String(secret) } : {}),
      }),
    });
  }
}

class UnsupportedSecretStore {
  async get() {
    return "";
  }

  async set() {
    throw new Error("secret store is not implemented for this platform");
  }

  async delete() {
    throw new Error("secret store is not implemented for this platform");
  }
}

function createSecretStore({ platform = process.platform } = {}) {
  if (platform === "darwin") return new MacOsKeychainSecretStore();
  if (platform === "linux") return new LinuxSecretServiceStore();
  if (platform === "win32") return new WindowsCredentialManagerStore();
  return new UnsupportedSecretStore();
}

module.exports = {
  LinuxSecretServiceStore,
  MacOsKeychainSecretStore,
  MemorySecretStore,
  UnsupportedSecretStore,
  WINDOWS_CREDENTIAL_MANAGER_SCRIPT,
  WindowsCredentialManagerStore,
  createSecretStore,
};
