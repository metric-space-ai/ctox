// Shell v4 – Workjet-Brücke, Aktionsgruppe D: Rechner (globalThis.workjetComputerControl). Eigentum Strang D.
// Rumpf wörtlich V2 app.js:13364-13821 (Pin e35a116ae) mit den Ersetzungen aus PORT-PLAN Regel 2
// (`state.` → `env.state.`, `actorContext` → `env.actorContext`, `newId` → `env.newId`, `Date.now()` → `env.now()`).
// Weitere, nötige Ersetzungen (Begründung in WJB-D.md):
//   - Hilfen mit V2-`state` bekommen `env` als ersten Parameter (requireWorkjetComputerDataPlane,
//     listProjectedWorkjetComputers, waitForProjectedWorkjetComputer).
//   - `waitForSyncBridgeReady(bridge, ms)` → `waitForSyncBridgeReady(env, bridge, ms)`: V2-Hilfe app.js:15458-15474
//     liegt außerhalb des Rechner-Abschnitts; lokale Kopie unten, env darf sie ersetzen (Orakel-Aufbau).
//   - `window.setTimeout` → `setTimeout`.
// v4-Zusatz: fehlt dem Kern die Route (v2-compat: unknown_command → {code:'unsupported'}), meldet die Brücke das
// einzige Merkmal, das Workjet für Rechner als „nicht unterstützt“ liest:
// `workjet_computers collection is not registered.` (CtoxGuestManager.ts buildGuestComputerControlExpression).
import { isUnsupported, unsupportedError } from '../v2-compat.js';

// ref: app.js:15458-15474 (waitForSyncBridgeReady, wörtlich; `state` heißt hier `bridgeState`, weil die Hilfe
// sonst das V2-Global verdeckt). Gehört nach v2-helpers (WJB-D.md).
async function v2WaitForSyncBridgeReady(bridge, timeoutMs = 15000) {
  const bridgeState = bridge?.state;
  if (!bridgeState) return;
  let timer = null;
  try {
    await Promise.race([
      Promise.resolve()
        .then(() => bridgeState.awaitInSync?.() || bridgeState.awaitInitialReplication?.())
        .catch(() => {}),
      new Promise((resolve) => {
        timer = setTimeout(resolve, timeoutMs);
        timer?.unref?.();
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}
export function waitForSyncBridgeReady(env, bridge, timeoutMs = 15000) {
  return typeof env.waitForSyncBridgeReady === 'function'
    ? env.waitForSyncBridgeReady(bridge, timeoutMs)
    : v2WaitForSyncBridgeReady(bridge, timeoutMs);
}

// E32: Lesesicht einer exportierten Collection für eine lokale Kernel-Abfrage (statt Kern-Befehl).
// Bereit = Collection-Liveness `live` (§16.45, v2-compat peer.awaitQueryReady). Die erste Abfrage abonniert die
// Collection, damit ihr Liveness-Eintrag sofort erscheint (sonst gälte ein noch nicht abonnierter Name als bereit,
// v2-compat.js awaitQueryReady `!l`). Ohne awaitQueryReady (V2-Testattrappen) gilt die V2-Schranke
// waitForSyncBridgeReady. Nicht bereit (error, Frist, Abfrage abgelehnt, z. B. nicht exportiert) → {ready:false};
// der Aufrufer meldet daraus die „nicht unterstützt“-Form seiner Brücke, nie „fehlgeschlagen“.
export const WORKJET_LOCAL_READ_BUDGET_MS = 15_000;
export async function awaitLocalReadView(env, name, budgetMs = WORKJET_LOCAL_READ_BUDGET_MS) {
  const collection = env.state.db?.collection?.(name);
  if (!collection?.find) return { ready: false, detail: `${name}: keine lokale Sicht` };
  try {
    const bridge = await env.state.sync?.startCollection?.(name); // v4-allow: debt L-42
    const peer = bridge?.state;
    if (typeof peer?.awaitQueryReady === 'function') {
      await collection.find({ limit: 1 }).exec();
      await peer.awaitQueryReady(budgetMs);
    } else {
      await waitForSyncBridgeReady(env, bridge, budgetMs);
    }
    return { ready: true, collection };
  } catch (error) {
    return { ready: false, detail: `${name}: ${error?.code ? `${error.code} ` : ''}${error?.message || error}` };
  }
}

const WORKJET_COMPUTER_CONTROL_MAX_RESULTS = 100;
const WORKJET_COMPUTER_CONTROL_TIMEOUT_MS = 30_000;
const WORKJET_SELF_HOST_COLOCATION_CONFIRMATION = 'workjet-self-host-colocation.v1';

async function workjetComputerControl(env, request = {}) {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    throw new TypeError('Workjet computer control request must be an object.');
  }
  const action = boundedWorkjetComputerText(request.action, 'action', 64);
  const ownerUserId = boundedWorkjetComputerText(env.actorContext(env.state.session).id, 'owner_user_id', 256);

  // E32: computer.list ist eine lokale Kernel-Abfrage auf workjet_computers, kein Kern-Befehl mehr.
  // V2 (app.js:13376-13402 @ e35a116ae) sandte ctox.workjet.computer.list nur als Schranke und verwarf die Quittung;
  // der Kern antwortet ohnehin nur {ok, collection, count, truncated} (store_workjet_computers.rs:159-193), die
  // Workjet-Antwort entstand schon immer aus den projizierten Zeilen (listProjectedWorkjetComputers, unverändert V2).
  if (action === 'computer.list') {
    assertWorkjetComputerPayloadKeys(request, new Set(['action', 'includeOperationalDetails']));
    if (request.includeOperationalDetails !== undefined
      && typeof request.includeOperationalDetails !== 'boolean') {
      throw new Error('Invalid Workjet computer includeOperationalDetails.');
    }
    const view = await awaitLocalReadView(env, 'workjet_computers', WORKJET_LOCAL_READ_BUDGET_MS);
    if (!view.ready) throw unsupportedError('computer', 'computer.list', view.detail);
    return {
      action: 'computer.list',
      computers: await listProjectedWorkjetComputers(env, ownerUserId, {
        includeOperationalDetails: request.includeOperationalDetails === true,
      }),
    };
  }

  const computerBridge = await requireWorkjetComputerDataPlane(env);

  if (action === 'computer.assign') {
    assertWorkjetComputerPayloadKeys(request, new Set([
      'action',
      'commandId',
      'computerId',
      'displayName',
      'hostingMode',
      'capabilities',
      'capabilityConfig',
      'agentless',
      'selfHostedColocation',
      'colocationConfirmation',
    ]));
    const commandId = boundedWorkjetComputerText(request.commandId, 'commandId', 128);
    const computerId = boundedWorkjetComputerText(request.computerId, 'computerId', 160);
    const displayName = boundedWorkjetComputerText(request.displayName, 'displayName', 256);
    const hostingMode = boundedWorkjetComputerHostingMode(request.hostingMode);
    const capabilities = boundedWorkjetComputerCapabilities(request.capabilities);
    const selfHostedColocation = request.selfHostedColocation === true;
    if (request.selfHostedColocation !== undefined
      && typeof request.selfHostedColocation !== 'boolean') {
      throw new Error('Invalid Workjet computer selfHostedColocation.');
    }
    let colocationConfirmation;
    if (request.colocationConfirmation !== undefined) {
      colocationConfirmation = boundedWorkjetComputerText(
        request.colocationConfirmation,
        'colocationConfirmation',
        64,
      );
    }
    if (selfHostedColocation
      && colocationConfirmation !== WORKJET_SELF_HOST_COLOCATION_CONFIRMATION) {
      throw new Error('Self-hosted co-location requires explicit workjet-self-host-colocation.v1 confirmation.');
    }
    const payload = {
      computer_id: computerId,
      display_name: displayName,
      hosting_mode: hostingMode,
      capabilities,
      self_hosted_colocation: selfHostedColocation,
    };
    if (colocationConfirmation !== undefined) {
      payload.colocation_confirmation = colocationConfirmation;
    }
    if (request.capabilityConfig !== undefined) {
      payload.capability_config = boundedWorkjetOperationalCapabilities(request.capabilityConfig);
    }
    if (request.agentless !== undefined) {
      if (typeof request.agentless !== 'boolean') throw new Error('Invalid Workjet agentless flag.');
      payload.agentless = request.agentless;
    }
    const receipt = await env.state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: 'ctox.workjet.computer.assign',
      record_id: computerId,
      payload,
      client_context: {
        source: 'workjet-computer-control',
        actor: env.actorContext(env.state.session),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_COMPUTER_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    // Typed grants are confirmed by their correlated native receipt, never by an old projection.
    if (payload.capability_config !== undefined || payload.agentless !== undefined) {
      const native = completedWorkjetComputerResult(receipt, commandId)?.computer;
      if (!native || native.id !== computerId || native.owner_user_id !== ownerUserId
        || native.status !== 'assigned'
        || (payload.agentless !== undefined && native.agentless !== payload.agentless)
        || (payload.capability_config !== undefined
          && JSON.stringify(boundedWorkjetOperationalCapabilities(native.capability_config))
            !== JSON.stringify(payload.capability_config))) {
        throw new Error('Workjet access grant was not confirmed.');
      }
      const confirmed = boundedWorkjetComputerResult(native);
      if (!confirmed) throw new Error('Invalid Workjet access grant result.');
      return { action: 'computer.assign', computer: confirmed };
    }
    const computer = await waitForProjectedWorkjetComputer(
      env,
      computerId,
      ownerUserId,
      'assigned',
      computerBridge,
      WORKJET_COMPUTER_CONTROL_TIMEOUT_MS,
    );
    return { action: 'computer.assign', computer };
  }

  if (action === 'computer.endpoint.upsert' || action === 'computer.endpoint.disable') {
    const upsert = action === 'computer.endpoint.upsert';
    assertWorkjetComputerPayloadKeys(request, new Set(upsert
      ? ['action', 'commandId', 'computerId', 'endpointRef', 'connection']
      : ['action', 'commandId', 'endpointRef']));
    const commandId = boundedWorkjetComputerText(request.commandId, 'commandId', 128);
    const endpointRef = boundedWorkjetComputerText(request.endpointRef, 'endpointRef', 128);
    const payload = { endpoint_ref: endpointRef };
    if (upsert) {
      payload.computer_id = boundedWorkjetComputerText(request.computerId, 'computerId', 160);
      payload.connection = boundedWorkjetComputerEndpoint(request.connection);
    }
    const receipt = await env.state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: upsert
        ? 'ctox.workjet.computer.endpoint.upsert'
        : 'ctox.workjet.computer.endpoint.disable',
      record_id: endpointRef,
      payload,
      client_context: {
        source: 'workjet-computer-control',
        actor: env.actorContext(env.state.session),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_COMPUTER_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    const endpoint = completedWorkjetComputerResult(receipt, commandId)?.endpoint;
    if (!endpoint || endpoint.id !== endpointRef || endpoint.owner_user_id !== ownerUserId
      || endpoint.enabled !== upsert
      || (upsert && endpoint.computer_id !== payload.computer_id)) {
      throw new Error('Workjet connection change was not confirmed.');
    }
    return { action, endpointRef, computerId: endpoint.computer_id, enabled: endpoint.enabled };
  }

  if (action === 'computer.ssh_key.ensure') {
    assertWorkjetComputerPayloadKeys(request, new Set(['action', 'commandId', 'computerId']));
    const commandId = boundedWorkjetComputerText(request.commandId, 'commandId', 128);
    const computerId = boundedWorkjetComputerText(request.computerId, 'computerId', 160);
    const receipt = await env.state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: 'ctox.workjet.computer.ssh_key.ensure',
      record_id: computerId,
      payload: { computer_id: computerId },
      client_context: {
        source: 'workjet-computer-control',
        actor: env.actorContext(env.state.session),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_COMPUTER_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    const result = completedWorkjetComputerResult(receipt, commandId);
    assertWorkjetComputerPayloadKeys(result, new Set([
      'ok', 'contract', 'computer_id', 'private_key', 'public_key', 'public_key_sha256',
    ]));
    if (result.contract !== 'ctox.workjet.computer-ssh-key.v1' || result.computer_id !== computerId) {
      throw new Error('Workjet computer SSH key was not confirmed.');
    }
    const reference = result.private_key;
    if (!reference || typeof reference !== 'object' || Array.isArray(reference)) {
      throw new Error('Workjet computer SSH key reference was not confirmed.');
    }
    assertWorkjetComputerPayloadKeys(reference, new Set(['scope', 'name']));
    const scope = boundedWorkjetComputerText(reference.scope, 'credential scope', 128);
    const name = boundedWorkjetComputerText(reference.name, 'credential name', 128);
    const publicKey = boundedWorkjetComputerText(result.public_key, 'public key', 1024);
    const publicKeySha256 = boundedWorkjetComputerText(result.public_key_sha256, 'public key fingerprint', 80);
    if (scope !== 'computer-access' || !/^workjet-ssh-[a-f0-9]{64}$/.test(name)
      || !/^ssh-ed25519 [A-Za-z0-9+/]+={0,2}(?: [^\r\n]*)?$/.test(publicKey)
      || !/^SHA256:[A-Za-z0-9+/]{43}$/.test(publicKeySha256)) {
      throw new Error('Workjet computer SSH key was not confirmed.');
    }
    return { action, contract: result.contract, computerId,
      privateKey: { scope, name }, publicKey, publicKeySha256 };
  }

  if (action === 'computer.unassign') {
    assertWorkjetComputerPayloadKeys(request, new Set(['action', 'commandId', 'computerId']));
    const commandId = boundedWorkjetComputerText(request.commandId, 'commandId', 128);
    const computerId = boundedWorkjetComputerText(request.computerId, 'computerId', 160);
    await env.state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: 'ctox.workjet.computer.unassign',
      record_id: computerId,
      payload: { computer_id: computerId },
      client_context: {
        source: 'workjet-computer-control',
        actor: env.actorContext(env.state.session),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_COMPUTER_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    const computer = await waitForProjectedWorkjetComputer(
      env,
      computerId,
      ownerUserId,
      'unassigned',
      computerBridge,
      WORKJET_COMPUTER_CONTROL_TIMEOUT_MS,
    );
    return { action: 'computer.unassign', computer };
  }

  throw new Error(`Unsupported Workjet computer control action: ${action}`);
}

function completedWorkjetComputerResult(receipt, commandId) {
  if (receipt?.ok !== true || receipt.status !== 'completed' || receipt.command_id !== commandId
    || receipt.result?.ok !== true) {
    throw new Error('Workjet computer command did not complete successfully.');
  }
  return receipt.result;
}

function workjetComputerInteger(value, label, max) {
  if (!Number.isSafeInteger(value) || value < 1 || value > max) {
    throw new Error(`Invalid Workjet computer ${label}.`);
  }
  return value;
}

function boundedWorkjetOperationalCapabilities(value) {
  if (!Array.isArray(value) || value.length > 3) {
    throw new Error('Invalid Workjet operational capabilities.');
  }
  const kinds = new Set();
  const capabilities = value.map((descriptor) => {
    if (!descriptor || typeof descriptor !== 'object' || Array.isArray(descriptor)) {
      throw new Error('Invalid Workjet operational capability.');
    }
    const kind = descriptor.kind;
    if (kinds.has(kind)) throw new Error('Duplicate Workjet operational capability.');
    kinds.add(kind);
    const text = (key, max = 128) => boundedWorkjetComputerText(descriptor[key], key, max);
    const integer = (key, max) => workjetComputerInteger(descriptor[key], key, max);
    if (kind === 'build') {
      assertWorkjetComputerPayloadKeys(descriptor, new Set([
        'kind', 'ssh_endpoint_ref', 'slots', 'jobs', 'lane_root', 'disk_floor_gib', 'toolchains',
      ]));
      if (!Array.isArray(descriptor.toolchains) || descriptor.toolchains.length < 1
        || descriptor.toolchains.length > 16) throw new Error('Invalid Workjet build toolchains.');
      return { kind, ssh_endpoint_ref: text('ssh_endpoint_ref'),
        slots: integer('slots', 32), jobs: integer('jobs', 64), lane_root: text('lane_root', 4096),
        disk_floor_gib: integer('disk_floor_gib', 4294967295),
        toolchains: [...new Set(descriptor.toolchains.map((tool) =>
          boundedWorkjetComputerText(tool, 'toolchain', 128)))].sort() };
    }
    if (kind === 'storage') {
      assertWorkjetComputerPayloadKeys(descriptor, new Set([
        'kind', 'endpoint_ref', 'protocol', 'root', 'quota_gib', 'purposes',
      ]));
      if (!['ssh', 'smb', 'nfs'].includes(descriptor.protocol)
        || !Array.isArray(descriptor.purposes) || descriptor.purposes.length < 1
        || descriptor.purposes.length > 3
        || descriptor.purposes.some((purpose) => !['artifacts', 'backups', 'exchange'].includes(purpose))) {
        throw new Error('Invalid Workjet storage protocol or purposes.');
      }
      return { kind, endpoint_ref: text('endpoint_ref'), protocol: descriptor.protocol,
        root: text('root', 4096), quota_gib: descriptor.quota_gib === null ? null
          : integer('quota_gib', Number.MAX_SAFE_INTEGER),
        purposes: [...new Set(descriptor.purposes)].sort() };
    }
    if (kind === 'gpu') {
      assertWorkjetComputerPayloadKeys(descriptor, new Set(['kind', 'model', 'vram_gib']));
      return { kind, model: text('model', 256), vram_gib: integer('vram_gib', 4294967295) };
    }
    throw new Error('Unsupported Workjet operational capability.');
  });
  return capabilities.sort((left, right) => left.kind.localeCompare(right.kind));
}

function boundedWorkjetComputerEndpoint(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Invalid Workjet computer endpoint.');
  }
  const protocol = value.protocol;
  const ssh = protocol === 'ssh';
  if (!ssh && protocol !== 'smb') throw new Error('Unsupported Workjet endpoint protocol.');
  assertWorkjetComputerPayloadKeys(value, new Set(ssh
    ? ['protocol', 'host', 'port', 'username', 'root', 'host_key_sha256',
      'host_key_algorithm', 'private_key', 'passphrase']
    : ['protocol', 'host', 'port', 'username', 'root', 'share', 'password']));
  const reference = (candidate) => {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
      throw new Error('Workjet credentials must reference the Secret Store.');
    }
    assertWorkjetComputerPayloadKeys(candidate, new Set(['scope', 'name']));
    return {
      scope: boundedWorkjetComputerText(candidate.scope, 'credential scope', 128),
      name: boundedWorkjetComputerText(candidate.name, 'credential name', 128),
    };
  };
  const connection = { protocol,
    host: boundedWorkjetComputerText(value.host, 'host', 253),
    port: workjetComputerInteger(value.port, 'port', 65535),
    username: boundedWorkjetComputerText(value.username, 'username', 256),
    root: boundedWorkjetComputerText(value.root, 'root', 4096) };
  if (ssh) {
    connection.host_key_sha256 = boundedWorkjetComputerText(value.host_key_sha256, 'host key', 80);
    connection.private_key = reference(value.private_key);
    connection.passphrase = value.passphrase == null ? null : reference(value.passphrase);
    if (value.host_key_algorithm != null) {
      if (!['ssh-ed25519', 'ecdsa-sha2-nistp256', 'ecdsa-sha2-nistp384',
        'ecdsa-sha2-nistp521', 'rsa-sha2-256', 'rsa-sha2-512'].includes(value.host_key_algorithm)) {
        throw new Error('Unsupported Workjet SSH host-key algorithm.');
      }
      connection.host_key_algorithm = value.host_key_algorithm;
    }
  } else {
    connection.share = boundedWorkjetComputerText(value.share, 'share', 128);
    connection.password = reference(value.password);
  }
  return connection;
}

async function requireWorkjetComputerDataPlane(env) {
  if (!env.state.commandBus?.dispatch || !env.state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet computer control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  const collection = env.state.db?.collection?.('workjet_computers');
  if (!collection) throw new Error('workjet_computers collection is not registered.');
  const commandBridge = await env.state.sync?.startCollection?.('business_commands'); // v4-allow: debt L-42
  await waitForSyncBridgeReady(env, commandBridge, 15_000);
  const computerBridge = await env.state.sync?.startCollection?.('workjet_computers'); // v4-allow: debt L-42
  await waitForSyncBridgeReady(env, computerBridge, 15_000);
  return computerBridge;
}

async function listProjectedWorkjetComputers(env, ownerUserId, options = {}) {
  const collection = env.state.db?.collection?.('workjet_computers');
  const docs = await collection.find({
    selector: {
      owner_user_id: { $eq: ownerUserId },
      status: { $eq: 'assigned' },
    },
    limit: WORKJET_COMPUTER_CONTROL_MAX_RESULTS,
  }).exec();
  return docs
    .map((doc) => boundedWorkjetComputerResult(doc?.toJSON?.() || doc, options))
    .filter(Boolean)
    .sort((left, right) => left.displayName.localeCompare(right.displayName)
      || left.id.localeCompare(right.id));
}

async function waitForProjectedWorkjetComputer(
  env,
  computerId,
  ownerUserId,
  status,
  bridge,
  timeoutMs,
) {
  const collection = env.state.db?.collection?.('workjet_computers');
  const deadline = env.now() + timeoutMs;
  let lastError = null;
  while (env.now() < deadline) {
    try {
      await waitForSyncBridgeReady(env, bridge, Math.max(1, deadline - env.now()));
      const doc = await collection.findOne(computerId).exec();
      const rawComputer = doc?.toJSON?.() || doc;
      if (rawComputer?.owner_user_id === ownerUserId && rawComputer?.status === status) {
        const computer = boundedWorkjetComputerResult(rawComputer, { includeUnassigned: true });
        if (computer) return computer;
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const error = new Error(`Workjet computer projection did not arrive for ${computerId}.`);
  error.code = 'workjet_computer_projection_timeout';
  if (lastError) error.cause = lastError;
  throw error;
}

function boundedWorkjetComputerHostingMode(value) {
  const hostingMode = boundedWorkjetComputerText(value, 'hostingMode', 64);
  if (hostingMode === 'managed_backend') {
    throw new Error('Managed backend hosts are backend-only and cannot be assigned as Workjet computers.');
  }
  if (!['workstation', 'self_hosted'].includes(hostingMode)) {
    throw new Error('Unsupported Workjet computer hostingMode.');
  }
  return hostingMode;
}

function boundedWorkjetComputerCapabilities(value) {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.length > 32) {
    throw new Error('Invalid Workjet computer capabilities.');
  }
  return Array.from(new Set(value.map((capability) => (
    boundedWorkjetComputerText(capability, 'capability', 80)
  )))).sort();
}

function boundedWorkjetComputerResult(value, options = {}) {
  if (!value || typeof value !== 'object' || value._deleted === true
    || value.is_deleted === true
    || (!options.includeUnassigned && value.status !== 'assigned')
    || !['assigned', 'unassigned'].includes(value.status)) {
    return null;
  }
  return Object.freeze({
    id: boundedWorkjetComputerText(value.id, 'computer.id', 160),
    displayName: boundedWorkjetComputerText(value.display_name, 'computer.displayName', 256),
    hostingMode: boundedWorkjetComputerHostingMode(value.hosting_mode),
    status: value.status,
    capabilities: Object.freeze(boundedWorkjetComputerCapabilities(value.capabilities)),
    ...(options.includeOperationalDetails && value.capability_config !== undefined
      ? { capabilityConfig: Object.freeze(boundedWorkjetOperationalCapabilities(value.capability_config)) } : {}),
    ...(options.includeOperationalDetails && typeof value.agentless === 'boolean'
      ? { agentless: value.agentless } : {}),
    selfHostedColocation: value.self_hosted_colocation === true,
  });
}

function boundedWorkjetComputerText(value, field, maxLength) {
  if (typeof value !== 'string') throw new Error(`Invalid Workjet computer ${field}.`);
  const normalized = value.trim();
  if (!normalized || [...normalized].length > maxLength || /[\u0000-\u001f\u007f]/.test(normalized)) {
    throw new Error(`Invalid Workjet computer ${field}.`);
  }
  return normalized;
}

function assertWorkjetComputerPayloadKeys(value, allowed) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw new Error(`Unsupported Workjet computer payload field: ${key}`);
  }
}

// Kern-Lücke → Rechner-Merkmal „nicht unterstützt“ (siehe Kopf).
async function computerControl(env, request) {
  try {
    return await workjetComputerControl(env, request);
  } catch (error) {
    if (isUnsupported(error) && error.bridge !== 'computer') {
      throw unsupportedError('computer', request?.action, error.detail || error.message);
    }
    throw error;
  }
}

// Freigeschaltete Aktionen (Orakel: workjet-computer-control.test.mjs).
export const COMPUTER_ACTIONS = Object.freeze(Object.fromEntries([
  'computer.list', 'computer.assign', 'computer.unassign',
  'computer.endpoint.upsert', 'computer.endpoint.disable', 'computer.ssh_key.ensure',
].map((action) => [action, computerControl])));
