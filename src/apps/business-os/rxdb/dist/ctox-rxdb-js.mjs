// src/apps/business-os/rxdb/src/protocol-contract.generated.mjs
var CTOX_RXDB_PROTOCOL = "ctox-rxdb-protocol-v1";
var CTOX_PROTOCOL_PHASE = "rxdb-protocol-handshake";
var CTOX_REQUIRED_PROTOCOL_CAPABILITIES = Object.freeze([
  "ctox-schema-hash-v1",
  "ctox-peer-session-v1",
  "ctox-checkpoint-epoch-v1"
]);
var CTOX_PROTOCOL_ERROR_CODES = Object.freeze({
  protocolMissing: "ctox_rxdb_protocol_missing",
  protocolMismatch: "ctox_rxdb_protocol_mismatch",
  capabilityMissing: "ctox_rxdb_capability_missing",
  collectionMismatch: "ctox_rxdb_collection_mismatch",
  schemaVersionMismatch: "ctox_rxdb_schema_version_mismatch",
  schemaHashMismatch: "ctox_rxdb_schema_hash_mismatch"
});
var CTOX_SCHEMA_HASH_SOURCES = Object.freeze({
  businessOsRegistry: "business-os-schema-hash-registry-v1",
  canonicalJson: "canonical-json-schema-sha256-v1",
  rxdbRs: "rxdb-rs-schema-hash-v1"
});

// src/apps/business-os/rxdb/src/schema.mjs
var CTOX_SCHEMA_HASH_CAPABILITY = "ctox-schema-hash-v1";
var CTOX_PEER_SESSION_CAPABILITY = "ctox-peer-session-v1";
var CTOX_CHECKPOINT_EPOCH_CAPABILITY = "ctox-checkpoint-epoch-v1";
var CTOX_BUSINESS_OS_SCHEMA_HASHES = Object.freeze({
  browser_frames: "89e1c1392d90d9f0ec826ced384f883092ce525f846e4d1c4383047a96673519",
  browser_input_events: "dc79706396f8c59865dc4187947fe925f4b1a1fae6669c4fd7d7d0e507a4dff7",
  browser_sessions: "8f9d925480b6fa11755bb0800e47da9d4b8dca59f510fb5c6bfb3d84cec212d3",
  browser_tabs: "3387a8373cad98f4651b15173cf920568970ad2afa7f14758bbfffe9d77d5004",
  business_chats: "4f7fc2d29ea54ef9cabef037caa01f0ef2567fc2fa156835c952bef2dd2fd456",
  business_commands: "4c273d32175717566fdc42c6f7b5d32e144f9d2ed1c7f5db15d1b9ef04c89d5e",
  business_module_acl: "7f2c6c44ffadefb0c9be30dba9f3067fc48e0847424e3f2709638c5ebcd8bedf",
  business_module_catalog: "332763869d93c2bb55fa6b217c36521d1c1f17be4701d8538d686cda89f5cea0",
  business_module_releases: "8d9ff79eec5eccc04353a885002a8982deb169dbbf3a348998b88fafb7e219f7",
  business_module_reports: "440b04e33e1040e556c62741d7c4289422b6d0d01203c74e5aee391d5f050ed1",
  business_module_source_files: "fa9cdeda3530f04bd84b926cb8ffae650c8f5886efac079daee0d01315737551",
  business_users: "da6d1a192bc21ad59baf2680d8b80faa471a4883457a8d0ad5a533a1afefba42",
  channel_pairing_state: "d93ceef99b772bc57939143bc6ef0044bf816801700d2dbc8f88def356aa246a",
  communication_accounts: "d40ca549e2f112071b6eb39bf0999a743643073279af4471a477cef259275653",
  communication_messages: "10d120234ec23bbe98124d255599f44d2ef68ecb5ff29787b9b647aaf6537b6f",
  communication_threads: "2111d907ee8cc8c7c2c4e9f10a43bc56f217071dbee0610a96b0457ef6473a8d",
  ctox_bug_reports: "f7329368ad5144b8ea740600265f06c6ac19ad049de751cec92818d9d9de94b5",
  ctox_queue_tasks: "2a5c7c35f65a2ad0e35d19902bbb0c45456137c30f046e9d322406872fbd0824",
  ctox_runs: "73df37bddc2e511b0567496f6199089aef436dd598a3e0bf85f462d38b4f3fff",
  ctox_runtime_settings: "3958bb6580e9705f3688fcf453a80ec33c486b43ac6988f015ffc16cb5ac918d",
  desktop_file_chunks: "e59672c6f729c100b9076f88be0abb695f8e780f5cd03c2fabc7abc770ae44d9",
  desktop_files: "5c8ea6eddecd37233ef1b99ad10280afe9ae5654bc77819d85d56236257be627",
  desktop_icons: "b3fc7cde6c2df59469255353b9ce91e5213ad091b86e8b3f2372e63db8c5ecd9",
  desktop_layout: "d741aa98029c7e0c38fb2ef53e32319ee4c7891b808c875802c540d60bdf5c3c",
  desktop_notifications: "5c312d2c291bf2b36fdbda8aacc1b2de7873c6ee7058c9960897bbb5b0797d0d",
  desktop_windows: "bcd10d8462083460b5025160f88f0abe6c7118d583aa4d1fd97433942617627f",
  document_blob_chunks: "9b4e27b2f795c697b67747b55e388b8d42afb3d5b8f66e6f9ec36f9564028b16",
  document_runbooks: "50b126b168c2fbf148da6b8693bbf455f6124c1b798a19e48aaaf5174acc9b7b",
  document_versions: "fca6df9bfa1d0d27f93d41cb7685fd08dacbf9f4843b7c1d95142b4cbe157738",
  documents: "600e0a73160dfaa480dd0ff8b833c85cec8aa60d41a9982a1ecd971e8a291ec1",
  knowledge_items: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  knowledge_runbooks: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  knowledge_tables: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  matching_objects: "31ff9b1fce039239cf0684e1cf246b9e5d3a222abd8ca4b0c9f3c837dfeb55e1",
  matching_requirements: "7a57a57784d58c9898d135a519a8789380742cb5a0de055f19e8f6a279035b50",
  matching_results: "a5260077a1b4e9d5881ff3b265daf8651b8c6be3158cb5eff0d4f78bed21137c",
  notes: "9c02d9c9f4362f7cb9739b5b401eb59528254534fdfd807050a941041304854d",
  outbound_account_limits: "35d7a40e3e485447e234f72ec898ce57b7f2b7ebc4f01bb748a7e9ea5a3fc68e",
  outbound_approvals: "f7be2c8526ffc3df85e92a56c8e808adebbcd8944be95bd05658bc6f9d7b143a",
  outbound_campaigns: "194e3748c589a9cfc50ed63dccab525028e9bdbd006f20b73c10e29aa865e58d",
  outbound_companies: "1d79eb4b67d84826ed2016b0385224600d51c334d5b91d4adb77e62e916d0bbf",
  outbound_engagements: "f310db7ac3c7abdc78b40b227866ce673f5871601d594b00853000f7c4e088c2",
  outbound_meeting_requests: "f04c3249c3a3d8cf7ca6c2a4b51fbb15729035bca707668fbef3988242e69aa2",
  outbound_messages: "93b8e2cea0670112b6499a86a774dafef3cbd289d11725bf57d4e0941ad13006",
  outbound_pipeline_items: "d128a88597977a96b0b2572c0eaeb7c2e5da7d21ae691ff0b0a18e4824fd378c",
  outbound_research_runs: "46573b72d1bd75daf105265b179af2b0b5d9fae5a61e15cf1198e0dc2604a372",
  outbound_sender_assignments: "d57aeee6946976bd082044147591d648583a6493c6c1c320359b0949c3405c78",
  outbound_sequences: "9368f8c42dc026c94549485d230d01ea511358313b64de0100b5f7706bae251b",
  outbound_sources: "241a2673630fb51c06a4e3155465855f299cb56ceeb8ce09ab1ba0d4c460c29a",
  outbound_suppression_entries: "2a894fbfc598d41b81ad7c76466e531d6771c7a9f6e5aa34389dba0e5f2cb329",
  planning_absences: "20263440e5b0fa1d7a3a8c0d95f0753f6f5a30da517dcc208fafe5467ef1870b",
  planning_employees: "36852db8c0acb2b48b653592aeefa1af483843e22a2f400cf411178d7e8377c7",
  planning_projects: "fc558898d1dfe2d9f8cfb925b5fbd304133fcfad7b2e63069770d5f8325e9b6f",
  planning_shifts: "3e5a629a3dd83035c59f23ece1074478bc37afbdea14a7c02dc262cb47813804",
  planning_time_records: "2674badebb2a9b2133f5053b651ec7723b197869c6e32db59153cf0c227c4829",
  research_notes: "d078cd9b657f5eeb66281eb33e8b912c772fac447a5e60b580901fd4ef82c6dd",
  research_runs: "ba19ca3daec5cd92154b75faa056bbfab95383769dd69b77ce663656d18c282c",
  research_tasks: "502aa089a7498cf17db0bad1bba2d4bda864261b99488a07e783f6c107dc0dd0",
  spreadsheet_blob_chunks: "dc97cfb4feca43442477d88da04528ecda56ab7cb52b38a19306270eddf26168",
  spreadsheet_runbooks: "08bf33d949370df78a4598cc97208212df6944c4feefe291787dad75e8b0d985",
  spreadsheet_versions: "5c569a9152b65e943b047a0419afea200a7c43e83e6c07eb0a0c667282e45842",
  spreadsheets: "1dfe54101a8efe6ad4d127bc9ac102c74d6b211cda716b1fa5411fc473c24367"
});
function canonicalJson(value) {
  return JSON.stringify(sortCanonical(value));
}
async function sha256Hex(text) {
  if (!globalThis.crypto?.subtle) {
    throw new Error("WebCrypto crypto.subtle is required for CTOX schema hashes");
  }
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
async function schemaHash(schema, collectionName = "") {
  const registryHash = CTOX_BUSINESS_OS_SCHEMA_HASHES[String(collectionName || "")];
  if (registryHash) return registryHash;
  return sha256Hex(canonicalJson(normalizeSchema(schema)));
}
function schemaHashSource(collectionName = "") {
  return CTOX_BUSINESS_OS_SCHEMA_HASHES[String(collectionName || "")] ? CTOX_SCHEMA_HASH_SOURCES.businessOsRegistry : CTOX_SCHEMA_HASH_SOURCES.canonicalJson;
}
function normalizeSchema(schema) {
  if (!schema || typeof schema !== "object") {
    throw new TypeError("schema must be an object");
  }
  const normalized = structuredCloneSafe(schema);
  delete normalized.hash;
  delete normalized.encrypted;
  return normalized;
}
function buildProtocolPayload({
  collectionName,
  schemaVersion,
  schemaHash: hash,
  schemaHashSource: source,
  peerSessionId,
  peerGeneration,
  checkpoint,
  role = "browser",
  capabilities = []
} = {}) {
  const checkpointEvidence = checkpoint || null;
  return {
    protocol: CTOX_RXDB_PROTOCOL,
    checkpoint: checkpointEvidence,
    collection: collectionName ? {
      name: collectionName,
      schemaVersion: Number.isFinite(schemaVersion) ? schemaVersion : null,
      schemaHash: hash || null,
      schemaHashSource: source || schemaHashSource(collectionName),
      checkpoint: checkpointEvidence
    } : null,
    peerSession: {
      role,
      sessionId: peerSessionId || null,
      generation: Number.isFinite(peerGeneration) ? peerGeneration : null
    },
    capabilities: Array.from(/* @__PURE__ */ new Set([
      ...CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
      ...capabilities
    ])).sort()
  };
}
function assertCompatibleProtocol(local, remote, {
  requiredCapabilities = CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  validateSchema = true
} = {}) {
  if (!remote || typeof remote !== "object") {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMissing,
      message: "CTOX RxDB WebRTC protocol payload is missing.",
      expected: CTOX_RXDB_PROTOCOL,
      actual: null
    });
  }
  if (!remote.protocol) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMissing,
      message: "CTOX RxDB WebRTC protocol marker is missing.",
      expected: CTOX_RXDB_PROTOCOL,
      actual: null
    });
  }
  if (remote.protocol !== CTOX_RXDB_PROTOCOL) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMismatch,
      message: "Incompatible CTOX RxDB WebRTC protocol.",
      expected: CTOX_RXDB_PROTOCOL,
      actual: remote.protocol
    });
  }
  const remoteCapabilities = new Set(
    Array.isArray(remote.capabilities) ? remote.capabilities.filter((capability) => typeof capability === "string" && capability) : []
  );
  for (const capability of requiredCapabilities || []) {
    if (!remoteCapabilities.has(capability)) {
      throw createProtocolCompatibilityError({
        code: CTOX_PROTOCOL_ERROR_CODES.capabilityMissing,
        message: `Remote CTOX RxDB peer is missing required capability ${capability}.`,
        expected: capability,
        actual: Array.from(remoteCapabilities).sort()
      });
    }
  }
  const localCollection = normalizeProtocolCollection(local);
  const remoteCollection = normalizeProtocolCollection(remote);
  if (localCollection.name && remoteCollection.name && localCollection.name !== remoteCollection.name) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.collectionMismatch,
      message: `CTOX RxDB collection mismatch: ${localCollection.name} != ${remoteCollection.name}.`,
      expected: localCollection.name,
      actual: remoteCollection.name,
      collection: localCollection.name
    });
  }
  if (validateSchema && (Number.isFinite(localCollection.schemaVersion) && Number.isFinite(remoteCollection.schemaVersion) && localCollection.schemaVersion !== remoteCollection.schemaVersion)) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.schemaVersionMismatch,
      message: `CTOX RxDB schema version mismatch for ${localCollection.name || remoteCollection.name || "collection"}.`,
      expected: localCollection.schemaVersion,
      actual: remoteCollection.schemaVersion,
      collection: localCollection.name || remoteCollection.name || null
    });
  }
  if (validateSchema && localCollection.schemaHash && remoteCollection.schemaHash && localCollection.schemaHash !== remoteCollection.schemaHash) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.schemaHashMismatch,
      message: `CTOX RxDB schema hash mismatch for ${localCollection.name || remoteCollection.name || "collection"}.`,
      expected: localCollection.schemaHash,
      actual: remoteCollection.schemaHash,
      collection: localCollection.name || remoteCollection.name || null
    });
  }
  return true;
}
function normalizeProtocolCollection(payload) {
  const collection = payload?.collection && typeof payload.collection === "object" ? payload.collection : {};
  return {
    name: collection.name || payload?.collectionName || payload?.collection || null,
    schemaVersion: Number.isFinite(collection.schemaVersion) ? collection.schemaVersion : Number.isFinite(payload?.schemaVersion) ? payload.schemaVersion : null,
    schemaHash: collection.schemaHash || payload?.schemaHash || null
  };
}
function createProtocolCompatibilityError({
  code,
  message,
  expected = null,
  actual = null,
  collection = null
}) {
  const error = new Error(message);
  error.name = "CtoxRxdbProtocolError";
  error.code = code;
  error.phase = CTOX_PROTOCOL_PHASE;
  error.expected = expected;
  error.actual = actual;
  error.collection = collection;
  error.retryable = false;
  return error;
}
function sortCanonical(value) {
  if (Array.isArray(value)) {
    return value.map(sortCanonical);
  }
  if (!value || typeof value !== "object") {
    return value;
  }
  const sorted = {};
  for (const key of Object.keys(value).sort()) {
    const next = value[key];
    if (typeof next !== "undefined") {
      sorted[key] = sortCanonical(next);
    }
  }
  return sorted;
}
function structuredCloneSafe(value) {
  if (typeof structuredClone === "function") {
    return structuredClone(value);
  }
  return JSON.parse(JSON.stringify(value));
}

// src/apps/business-os/rxdb/src/event-target.mjs
var CtoxEventEmitter = class {
  constructor() {
    this.target = new EventTarget();
  }
  on(type, listener) {
    this.target.addEventListener(type, listener);
    return () => this.target.removeEventListener(type, listener);
  }
  once(type, listener) {
    const unsubscribe = this.on(type, (event) => {
      unsubscribe();
      listener(event);
    });
    return unsubscribe;
  }
  emit(type, detail = {}) {
    this.target.dispatchEvent(new CustomEvent(type, { detail }));
  }
};
function waitForEvent(emitter, type, timeoutMs = 1e4) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timed out waiting for ${type}`));
    }, timeoutMs);
    const unsubscribe = emitter.once(type, (event) => {
      clearTimeout(timeout);
      resolve(event.detail);
    });
  });
}

// src/apps/business-os/rxdb/src/storage-indexeddb.mjs
var DB_VERSION = 1;
var DOCUMENT_STORE = "documents";
var OPEN_DATABASE_TIMEOUT_MS = 4e3;
async function openCtoxIndexedDbStorage({ databaseName = "ctox_business_os_js_v1" } = {}) {
  if (!globalThis.indexedDB) {
    throw new Error("indexedDB is required for ctox-rxdb-js storage");
  }
  const db = await openDatabase(databaseName);
  return new CtoxIndexedDbStorage(db);
}
var CtoxIndexedDbStorage = class {
  constructor(db) {
    this.db = db;
  }
  collection(name, { schema = null } = {}) {
    if (!name || typeof name !== "string") {
      throw new TypeError("collection name must be a non-empty string");
    }
    return new CtoxIndexedDbCollection(this.db, name, { schema });
  }
  close() {
    this.db.close();
  }
};
var CtoxIndexedDbCollection = class {
  constructor(db, name, { schema = null } = {}) {
    this.db = db;
    this.name = name;
    this.schema = schema || {};
    this.indexes = normalizeSchemaIndexes(schema);
    this.events = new CtoxEventEmitter();
  }
  observe(listener) {
    return this.events.on("change", listener);
  }
  async upsert(doc) {
    const id = documentId(doc);
    if (!id) {
      throw new Error(`Cannot upsert ${this.name} document without primary key`);
    }
    const previous = await this.findOne(id);
    await this.bulkWrite([{ previous, document: { ...previous || {}, ...doc } }]);
    return this.findOne(id, { withDeleted: true });
  }
  async bulkWrite(rows, { now = Date.now(), replicationOrigin = null } = {}) {
    if (!Array.isArray(rows)) {
      throw new TypeError("bulkWrite rows must be an array");
    }
    const tx = this.db.transaction(DOCUMENT_STORE, "readwrite");
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];
    for (const row of rows) {
      const doc = row?.document || row;
      const id = documentId(doc);
      if (!id) {
        error.push({ row, error: "missing primary key" });
        continue;
      }
      const lwt = Number(doc._meta?.lwt || doc.updated_at_ms || doc.updatedAtMs || now);
      const stored = {
        collection: this.name,
        id,
        lwt,
        deleted: Boolean(doc._deleted),
        indexValues: indexValuesFor(this.indexes, doc),
        doc: normalizeDocument(doc, lwt, replicationOrigin)
      };
      await idbRequest(store.put(stored));
      success[id] = stored.doc;
    }
    await idbTransactionDone(tx);
    if (Object.keys(success).length) {
      this.events.emit("change", {
        collection: this.name,
        success,
        at: now
      });
    }
    return { success, error };
  }
  async findDocumentsById(ids, { withDeleted = false } = {}) {
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const store = tx.objectStore(DOCUMENT_STORE);
    const result = {};
    for (const id of ids) {
      const record = await idbRequest(store.get([this.name, String(id)]));
      if (record && (withDeleted || !record.deleted)) {
        result[String(id)] = record.doc;
      }
    }
    await idbTransactionDone(tx);
    return result;
  }
  async findOne(id, { withDeleted = false } = {}) {
    const docs = await this.findDocumentsById([id], { withDeleted });
    return docs[String(id)] || null;
  }
  async allDocuments({ withDeleted = false } = {}) {
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collection");
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (withDeleted || !record.deleted) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    return documents;
  }
  async getChangedDocumentsSince(checkpoint = null, limit = 100, options = {}) {
    const fromLwt = Number(checkpoint?.lwt || 0);
    const fromId = String(checkpoint?.id || "");
    const excludedOriginRole = String(options?.excludeReplicationOriginRole || "").trim();
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collectionLwtId");
    const range = IDBKeyRange.bound([this.name, fromLwt, fromId], [this.name, Number.MAX_SAFE_INTEGER, "\uFFFF"], true, false);
    const documents = [];
    let nextCheckpoint = checkpoint || null;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || documents.length >= limit) {
        return false;
      }
      const record = cursor.value;
      nextCheckpoint = { lwt: record.lwt, id: record.id };
      if (!documentMatchesReplicationOrigin(record.doc, excludedOriginRole)) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    return { documents, checkpoint: nextCheckpoint };
  }
  async replicationCheckpointStatus(schemaHash2 = null) {
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collectionLwtId");
    const range = IDBKeyRange.bound([this.name, 0, ""], [this.name, Number.MAX_SAFE_INTEGER, "\uFFFF"], false, false);
    const record = await firstCursorValue(index.openCursor(range, "prev"));
    await idbTransactionDone(tx);
    if (!record) {
      return {
        source: "browser",
        state: "advertised",
        collection: this.name,
        schemaHash: schemaHash2,
        latestLwt: null,
        latestIdHash: null,
        epoch: `browser:${this.name}:empty`
      };
    }
    const latestIdHash = await sha256Hex(record.id);
    return {
      source: "browser",
      state: "advertised",
      collection: this.name,
      schemaHash: schemaHash2,
      latestLwt: record.lwt,
      latestIdHash,
      epoch: `browser:${this.name}:${record.lwt}:${latestIdHash.slice(0, 16)}`
    };
  }
  schemaIndexes() {
    return this.indexes.map((index) => ({ ...index, fields: [...index.fields] }));
  }
  queryPlanFor(query = {}) {
    const selectorFields = Object.keys(query?.selector || {}).filter((field) => !field.startsWith("$"));
    const sortFields = normalizeSortFields(query?.sort);
    const selectedIndex = selectBestIndex(this.indexes, selectorFields, sortFields);
    return {
      collection: this.name,
      selectorFields,
      sortFields,
      selectedIndex,
      indexed: Boolean(selectedIndex)
    };
  }
};
function openDatabase(databaseName) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (fn, value) => {
      if (settled) return false;
      settled = true;
      clearTimeout(timer);
      fn(value);
      return true;
    };
    const timer = setTimeout(() => {
      finish(reject, new Error(`IndexedDB open timed out after ${OPEN_DATABASE_TIMEOUT_MS}ms for ${databaseName}`));
    }, OPEN_DATABASE_TIMEOUT_MS);
    const request = indexedDB.open(databaseName, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(DOCUMENT_STORE)) {
        const store = db.createObjectStore(DOCUMENT_STORE, { keyPath: ["collection", "id"] });
        store.createIndex("collection", "collection", { unique: false });
        store.createIndex("collectionLwtId", ["collection", "lwt", "id"], { unique: false });
      }
    };
    request.onsuccess = () => {
      if (!finish(resolve, request.result)) {
        try {
          request.result?.close?.();
        } catch {
        }
      }
    };
    request.onerror = () => finish(reject, request.error || new Error(`Failed to open IndexedDB ${databaseName}`));
    request.onblocked = () => finish(reject, new Error(`IndexedDB open blocked for ${databaseName}`));
  });
}
function documentId(doc) {
  if (!doc || typeof doc !== "object") {
    return "";
  }
  return String(doc.id || doc._id || doc.document_id || doc.documentId || "");
}
function normalizeDocument(doc, lwt, replicationOrigin = null) {
  const normalized = { ...doc };
  const id = documentId(doc);
  if (!normalized.id) {
    normalized.id = id;
  }
  normalized._meta = { ...normalized._meta || {}, lwt };
  if (replicationOrigin?.role) {
    normalized._meta.ctoxReplicationOrigin = sanitizeReplicationOrigin(replicationOrigin);
  } else {
    delete normalized._meta.ctoxReplicationOrigin;
  }
  normalized._deleted = Boolean(normalized._deleted);
  return normalized;
}
function sanitizeReplicationOrigin(origin) {
  return {
    role: String(origin.role || "").slice(0, 64),
    peerId: String(origin.peerId || "").slice(0, 160),
    sessionId: String(origin.sessionId || "").slice(0, 160),
    collection: String(origin.collection || "").slice(0, 160)
  };
}
function documentMatchesReplicationOrigin(doc, excludedOriginRole) {
  if (!excludedOriginRole) return false;
  const origin = doc?._meta?.ctoxReplicationOrigin;
  return origin?.role === excludedOriginRole;
}
function normalizeSchemaIndexes(schema = {}) {
  const indexes = Array.isArray(schema?.indexes) ? schema.indexes : [];
  return indexes.map((index, position) => {
    const fields = Array.isArray(index) ? index : [index];
    const normalizedFields = fields.map((field) => String(field || "").trim()).filter(Boolean);
    return normalizedFields.length ? { name: `idx_${position}_${normalizedFields.join("_")}`, fields: normalizedFields } : null;
  }).filter(Boolean);
}
function indexValuesFor(indexes, doc) {
  const values = {};
  for (const index of indexes || []) {
    values[index.name] = index.fields.map((field) => valueAtPath(doc, field));
  }
  return values;
}
function selectBestIndex(indexes, selectorFields = [], sortFields = []) {
  const wanted = [...selectorFields, ...sortFields].filter(Boolean);
  if (!wanted.length) return null;
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    let score = 0;
    for (const field of index.fields) {
      if (wanted.includes(field)) score += 1;
      else break;
    }
    if (score > bestScore) {
      best = index;
      bestScore = score;
    }
  }
  return best ? { ...best, fields: [...best.fields], matchedFields: bestScore } : null;
}
function normalizeSortFields(sort = []) {
  if (!Array.isArray(sort)) return typeof sort === "string" ? [sort] : [];
  return sort.map((entry) => {
    if (typeof entry === "string") return entry;
    return Object.keys(entry || {})[0] || "";
  }).filter(Boolean);
}
function valueAtPath(doc, path) {
  return String(path || "").split(".").reduce((value, key) => value?.[key], doc);
}
function idbRequest(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
function idbTransactionDone(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onabort = () => reject(tx.error || new Error("IndexedDB transaction aborted"));
    tx.onerror = () => reject(tx.error || new Error("IndexedDB transaction failed"));
  });
}
function iterateCursor(request, visitor) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => {
      const cursor = request.result;
      if (!cursor) {
        resolve();
        return;
      }
      const shouldContinue = visitor(cursor);
      if (shouldContinue === false) {
        resolve();
        return;
      }
      cursor.continue();
    };
    request.onerror = () => reject(request.error);
  });
}
function firstCursorValue(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result?.value || null);
    request.onerror = () => reject(request.error);
  });
}
var ctoxIndexedDbStorageTestInternals = {
  documentMatchesReplicationOrigin,
  indexValuesFor,
  normalizeDocument,
  normalizeSchemaIndexes,
  selectBestIndex
};

// src/apps/business-os/rxdb/src/frame-contract.generated.mjs
var CTOX_FRAME_PROTOCOL = "ctox-rxdb-frame-v1";
var MAX_INLINE_FRAME_BYTES = 14336;
var MAX_CHUNK_CHARS = 10240;
var MAX_TRANSFER_BYTES = 8388608;
var FRAME_ACK_WINDOW = 4;
var MAX_FRAME_RETRIES = 2;

// src/apps/business-os/rxdb/src/webrtc-native.mjs
var SEND_BUFFER_HIGH_WATER = 512 * 1024;
var SEND_BUFFER_LOW_WATER = 128 * 1024;
var FRAME_ACK_TIMEOUT_MS = 3e4;
var FRAME_RESUME_TIMEOUT_MS = 1e3;
var COMPLETED_FRAME_ACK_TTL_MS = 6e4;
var SEND_PRIORITIES = ["high", "normal", "low"];
function createCtoxWebRtcNativePeer(options = {}) {
  return new CtoxWebRtcNativePeer(options);
}
var CtoxWebRtcNativePeer = class {
  constructor({
    signalingUrl,
    room,
    roomPassword = "",
    token = "",
    tokenIssuedAt = null,
    tokenExpiresAt = null,
    clientId = randomId("browser"),
    role = "browser",
    instanceId = "",
    capabilities = [],
    iceServers = [],
    storageToken = randomId("storage"),
    protocolPayload = null,
    requestHandlers = {}
  } = {}) {
    if (!signalingUrl) {
      throw new Error("signalingUrl is required");
    }
    if (!room) {
      throw new Error("room is required");
    }
    this.options = {
      signalingUrl,
      room,
      roomPassword,
      token,
      tokenIssuedAt,
      tokenExpiresAt,
      clientId,
      role,
      instanceId,
      capabilities,
      iceServers,
      storageToken,
      protocolPayload,
      requestHandlers
    };
    this.events = new CtoxEventEmitter();
    this.socket = null;
    this.connections = /* @__PURE__ */ new Map();
    this.peerMetadata = /* @__PURE__ */ new Map();
    this.pending = /* @__PURE__ */ new Map();
    this.pendingFrameAcks = /* @__PURE__ */ new Map();
    this.incomingFrames = /* @__PURE__ */ new Map();
    this.completedFrameAcks = /* @__PURE__ */ new Map();
    this.observedRequests = /* @__PURE__ */ new Map();
    this.requestWaiters = /* @__PURE__ */ new Map();
    this.requestCounter = 0;
    this.frameCounter = 0;
    this.transportStats = {
      protocol: CTOX_FRAME_PROTOCOL,
      maxInlineFrameBytes: MAX_INLINE_FRAME_BYTES,
      maxChunkChars: MAX_CHUNK_CHARS,
      maxTransferBytes: MAX_TRANSFER_BYTES,
      ackWindow: FRAME_ACK_WINDOW,
      sendBufferHighWater: SEND_BUFFER_HIGH_WATER,
      sendBufferLowWater: SEND_BUFFER_LOW_WATER,
      activeTransfers: 0,
      pendingAcks: 0,
      incomingTransfers: 0,
      completedAckCacheSize: 0,
      sentFrames: 0,
      sentBytes: 0,
      receivedFrames: 0,
      receivedBytes: 0,
      retryCount: 0,
      resumeRequestCount: 0,
      resumeAckCount: 0,
      backpressureWaitCount: 0,
      queuedFrames: 0,
      sentScheduledFrames: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
      lastSendPriority: "normal",
      lastAckLagMs: 0,
      lastBufferedAmount: 0,
      updatedAtMs: Date.now()
    };
    this.lastControlPlaneError = null;
    this.closed = false;
  }
  on(type, listener) {
    return this.events.on(type, listener);
  }
  connect() {
    this.closed = false;
    const url = buildSignalingUrl(this.options);
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onopen = () => {
      socket.send(JSON.stringify({ type: "join", room: this.options.room }));
      this.events.emit("signaling-open", { url: redactUrl(url) });
    };
    socket.onmessage = (event) => this.handleSignalingMessage(event.data);
    socket.onerror = () => this.events.emit("error", this.lastControlPlaneError || { code: "ctox_signaling_socket_error" });
    socket.onclose = () => this.events.emit("signaling-close", {});
    return this;
  }
  close() {
    this.closed = true;
    for (const peerId of [...this.connections.keys()]) {
      this.removeConnection(peerId, "peer-close");
    }
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) {
      this.socket.close();
    }
    this.rejectAllPending(createPeerClosedError(this.options.clientId, "peer-close"));
    this.incomingFrames.clear();
  }
  send(remotePeerId, payload) {
    const connection = this.connections.get(remotePeerId);
    if (!connection?.channel || connection.channel.readyState !== "open") {
      return false;
    }
    const text = JSON.stringify(payload);
    this.enqueueSendFrame(connection, {
      payload,
      text,
      inline: encodedSize(text) <= MAX_INLINE_FRAME_BYTES,
      priority: classifySendPriority(payload, text)
    });
    return true;
  }
  enqueueSendFrame(connection, item) {
    if (!connection.sendQueue) {
      connection.sendQueue = createSendQueue();
    }
    connection.sendQueue[item.priority].push({
      ...item,
      queuedAtMs: Date.now(),
      sequence: connection.sendQueue.nextSequence++
    });
    this.recordTransportStatus({
      queuedFrames: this.transportStats.queuedFrames + 1,
      lastSendPriority: item.priority
    });
    this.refreshSendQueueStatus(connection);
    this.drainSendQueue(connection).catch((error) => {
      this.events.emit("error", {
        code: "ctox_webrtc_send_queue_failed",
        peerId: connection.remotePeerId,
        message: error?.message || String(error)
      });
    });
  }
  async drainSendQueue(connection) {
    if (connection.sendQueue?.draining) return;
    connection.sendQueue.draining = true;
    try {
      await Promise.resolve();
      while (!this.closed && connection.channel?.readyState === "open") {
        const item = nextQueuedSend(connection.sendQueue);
        if (!item) break;
        this.refreshSendQueueStatus(connection);
        this.recordTransportStatus({
          sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
          lastSendPriority: item.priority
        });
        if (item.inline) {
          await this.waitForSendBuffer(connection.channel);
          connection.channel.send(item.text);
          continue;
        }
        try {
          await this.sendFramed(connection, item.text);
        } catch (error) {
          this.events.emit("error", {
            code: "ctox_webrtc_frame_send_failed",
            peerId: connection.remotePeerId,
            priority: item.priority,
            message: error?.message || String(error)
          });
        }
      }
    } finally {
      connection.sendQueue.draining = false;
      this.refreshSendQueueStatus(connection);
    }
  }
  async sendFramed(connection, text) {
    const channel = connection.channel;
    const transferId = `${this.options.clientId}|frame|${Date.now()}|${this.frameCounter++}`;
    const totalFrames = Math.ceil(text.length / MAX_CHUNK_CHARS);
    const totalBytes = encodedSize(text);
    if (totalBytes > MAX_TRANSFER_BYTES) {
      throw new Error(`WebRTC frame transfer exceeds ${MAX_TRANSFER_BYTES} bytes`);
    }
    this.recordTransportStatus({ activeTransfers: this.transportStats.activeTransfers + 1 });
    let lastError = null;
    for (let attempt = 0; attempt <= MAX_FRAME_RETRIES; attempt += 1) {
      const startFrame = {
        ctoxFrame: CTOX_FRAME_PROTOCOL,
        kind: "start",
        transferId,
        windowSize: FRAME_ACK_WINDOW,
        attempt,
        totalFrames,
        totalBytes
      };
      channel.send(JSON.stringify(startFrame));
      this.recordSentTransportFrame(startFrame, channel);
      try {
        for (let windowStart = 0; windowStart < totalFrames; windowStart += FRAME_ACK_WINDOW) {
          await this.drainHighPriorityInlineFrames(connection);
          const windowEnd = Math.min(windowStart + FRAME_ACK_WINDOW, totalFrames) - 1;
          const ack = this.awaitFrameAck(transferId, connection.remotePeerId, windowEnd);
          for (let seq = windowStart; seq <= windowEnd; seq += 1) {
            await this.waitForSendBuffer(channel);
            const chunkFrame = {
              ctoxFrame: CTOX_FRAME_PROTOCOL,
              kind: "chunk",
              transferId,
              attempt,
              seq,
              data: text.slice(seq * MAX_CHUNK_CHARS, (seq + 1) * MAX_CHUNK_CHARS)
            };
            channel.send(JSON.stringify(chunkFrame));
            this.recordSentTransportFrame(chunkFrame, channel);
          }
          try {
            await ack;
          } catch (error) {
            const resumed = await this.requestFrameResume(connection, transferId, attempt, windowEnd);
            if (!resumed) throw error;
          }
        }
        this.recordTransportStatus({ activeTransfers: Math.max(0, this.transportStats.activeTransfers - 1) });
        return;
      } catch (error) {
        lastError = error;
        if (attempt >= MAX_FRAME_RETRIES) break;
        this.recordTransportStatus({ retryCount: this.transportStats.retryCount + 1 });
        this.events.emit("transport-retry", {
          peerId: connection.remotePeerId,
          transferId,
          attempt: attempt + 1
        });
        await delay(Math.min(250 * (attempt + 1), 1e3));
      }
    }
    this.recordTransportStatus({ activeTransfers: Math.max(0, this.transportStats.activeTransfers - 1) });
    throw lastError || new Error(`WebRTC frame transfer failed ${transferId}`);
  }
  async drainHighPriorityInlineFrames(connection) {
    const queue = connection.sendQueue;
    if (!queue) return;
    while (queue.high.length && queue.high[0]?.inline && connection.channel?.readyState === "open") {
      const item = queue.high.shift();
      this.refreshSendQueueStatus(connection);
      await this.waitForSendBuffer(connection.channel);
      connection.channel.send(item.text);
      this.recordTransportStatus({
        sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
        lastSendPriority: item.priority
      });
    }
  }
  awaitFrameAck(transferId, peerId, ackSeq = null) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingFrameAcks.delete(frameAckKey(transferId, ackSeq));
        reject(new Error(`Timed out waiting for WebRTC frame ack ${transferId}:${ackSeq ?? "final"}`));
      }, FRAME_ACK_TIMEOUT_MS);
      this.pendingFrameAcks.set(frameAckKey(transferId, ackSeq), { resolve, reject, timer, peerId, transferId, ackSeq, sentAtMs: Date.now() });
      this.recordTransportStatus({ pendingAcks: this.pendingFrameAcks.size });
    });
  }
  requestFrameResume(connection, transferId, attempt, ackSeq) {
    const channel = connection.channel;
    return new Promise((resolve, reject) => {
      const key = frameAckKey(transferId, ackSeq);
      const timer = setTimeout(() => {
        this.pendingFrameAcks.delete(key);
        this.recordTransportStatus({ pendingAcks: this.pendingFrameAcks.size });
        resolve(false);
      }, FRAME_RESUME_TIMEOUT_MS);
      this.pendingFrameAcks.set(key, {
        resolve: (payload) => resolve(payload || true),
        reject,
        timer,
        peerId: connection.remotePeerId,
        transferId,
        ackSeq,
        sentAtMs: Date.now()
      });
      const resumeFrame = {
        ctoxFrame: CTOX_FRAME_PROTOCOL,
        kind: "resume",
        transferId,
        attempt,
        ackSeq
      };
      channel.send(JSON.stringify(resumeFrame));
      this.recordSentTransportFrame(resumeFrame, channel);
      this.recordTransportStatus({ resumeRequestCount: this.transportStats.resumeRequestCount + 1 });
    });
  }
  waitForSendBuffer(channel) {
    if (Number(channel.bufferedAmount || 0) <= SEND_BUFFER_HIGH_WATER) {
      return Promise.resolve();
    }
    this.recordTransportStatus({
      backpressureWaitCount: this.transportStats.backpressureWaitCount + 1,
      lastBufferedAmount: Number(channel.bufferedAmount || 0)
    });
    return new Promise((resolve) => {
      const previousThreshold = channel.bufferedAmountLowThreshold;
      channel.bufferedAmountLowThreshold = SEND_BUFFER_LOW_WATER;
      const done = () => {
        channel.removeEventListener?.("bufferedamountlow", done);
        channel.bufferedAmountLowThreshold = previousThreshold || 0;
        resolve();
      };
      channel.addEventListener?.("bufferedamountlow", done, { once: true });
      setTimeout(done, 250);
    });
  }
  request(remotePeerId, method, params = [], timeoutMs = 15e3) {
    const id = `${this.options.clientId}|${Date.now()}|${this.requestCounter++}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Timed out waiting for WebRTC response ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method, peerId: remotePeerId });
      const sent = this.send(remotePeerId, { id, method, params });
      if (!sent) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(new Error(`WebRTC peer ${remotePeerId} is not open`));
      }
    });
  }
  handleSignalingMessage(raw) {
    let message;
    try {
      message = JSON.parse(raw);
    } catch (error) {
      this.events.emit("error", { code: "ctox_signaling_invalid_json", message: error.message });
      return;
    }
    if (message.type === "init" || message.type === "joined") {
      const ownPeerId = message.yourPeerId || message.peerId || this.options.clientId;
      if (ownPeerId && ownPeerId !== this.options.clientId) {
        this.options.clientId = String(ownPeerId);
      }
      for (const descriptor of signalingPeerDescriptors(message)) {
        const remotePeerId = descriptor.peerId;
        if (!remotePeerId) continue;
        this.rememberPeerMetadata(remotePeerId, descriptor);
        if (message.type === "joined" && remotePeerId !== this.options.clientId && this.connections.has(remotePeerId)) {
          this.removeConnection(remotePeerId, "signaling-peer-rejoined");
        }
        if (!this.shouldConnectToRemotePeer(remotePeerId)) {
          this.removeConnection(remotePeerId, "signaling-non-native-peer");
          continue;
        }
        this.ensureConnection(remotePeerId);
      }
      this.events.emit("joined", message);
      return;
    }
    if (message.type === "ctoxError") {
      const error = normalizeSignalingControlPlaneError(message);
      if (error.name === "CtoxSignalingControlPlaneError") {
        this.lastControlPlaneError = error;
      }
      this.events.emit("error", error);
      return;
    }
    if (message.type === "signal" || message.signal || message.data) {
      const remotePeerId = String(message.senderPeerId || message.sender || message.from || message.peerId || "");
      if (!remotePeerId) {
        this.events.emit("error", { code: "ctox_signaling_missing_sender" });
        return;
      }
      if (!this.shouldConnectToRemotePeer(remotePeerId)) {
        return;
      }
      this.handlePeerSignal(remotePeerId, message.signal || message.data).catch((error) => {
        const normalized = normalizePeerSignalError(error, remotePeerId);
        if (normalized?.ignored) return;
        this.events.emit("error", normalized);
      });
    }
  }
  ensureConnection(remotePeerId) {
    if (remotePeerId === this.options.clientId) {
      return this.connections.get(remotePeerId);
    }
    if (!this.shouldConnectToRemotePeer(remotePeerId)) {
      return void 0;
    }
    let connection = this.connections.get(remotePeerId);
    if (connection) {
      return connection;
    }
    const peer = new RTCPeerConnection({ iceServers: this.options.iceServers });
    connection = { peer, channel: null, remotePeerId, pendingCandidates: [] };
    this.connections.set(remotePeerId, connection);
    peer.onicecandidate = (event) => {
      if (event.candidate) {
        this.sendSignal(remotePeerId, { type: "candidate", candidate: event.candidate.toJSON() });
      }
    };
    peer.onconnectionstatechange = () => {
      const state = peer.connectionState;
      this.events.emit("peer-state", { peerId: remotePeerId, state });
      if (["closed", "failed", "disconnected"].includes(state)) {
        this.removeConnection(remotePeerId, `peer-${state}`);
      }
    };
    peer.ondatachannel = (event) => this.attachChannel(connection, event.channel);
    if (this.shouldInitiate(remotePeerId)) {
      this.attachChannel(connection, peer.createDataChannel("ctox-rxdb"));
      this.createOffer(remotePeerId, peer).catch((error) => {
        this.events.emit("error", normalizePeerSignalError(error, remotePeerId));
      });
    }
    return connection;
  }
  shouldInitiate(remotePeerId) {
    return String(this.options.clientId) < String(remotePeerId);
  }
  async createOffer(remotePeerId, peer) {
    if (this.closed || peer.signalingState === "closed") return;
    const offer = await peer.createOffer();
    if (this.closed || peer.signalingState === "closed") return;
    await peer.setLocalDescription(offer);
    this.sendSignal(remotePeerId, { type: offer.type, sdp: offer.sdp });
  }
  async handlePeerSignal(remotePeerId, signal) {
    const connection = this.ensureConnection(remotePeerId);
    if (!connection) return;
    const peer = connection.peer;
    const data = typeof signal === "string" ? JSON.parse(signal) : signal;
    if (data.type === "candidate") {
      await this.addIceCandidateWhenReady(connection, data.candidate);
      return;
    }
    if (data.type === "offer") {
      if (peer.signalingState !== "stable") {
        if (this.shouldInitiate(remotePeerId)) {
          return;
        }
        await rollbackLocalDescription(peer);
      }
      await peer.setRemoteDescription(data);
      await this.flushPendingIceCandidates(connection);
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      this.sendSignal(remotePeerId, { type: answer.type, sdp: answer.sdp });
      return;
    }
    if (data.type === "answer") {
      if (peer.signalingState !== "have-local-offer") {
        return;
      }
      await peer.setRemoteDescription(data);
      await this.flushPendingIceCandidates(connection);
    }
  }
  async addIceCandidateWhenReady(connection, candidate) {
    if (!candidate) return;
    const peer = connection?.peer;
    if (!peer || peer.signalingState === "closed") return;
    if (!peer.remoteDescription) {
      connection.pendingCandidates.push(candidate);
      return;
    }
    try {
      await peer.addIceCandidate(candidate);
    } catch (error) {
      if (!peer.remoteDescription && isMissingRemoteDescriptionIceError(error)) {
        connection.pendingCandidates.push(candidate);
        return;
      }
      throw error;
    }
  }
  async flushPendingIceCandidates(connection) {
    const peer = connection?.peer;
    if (!peer || peer.signalingState === "closed" || !peer.remoteDescription) return;
    const candidates = connection.pendingCandidates.splice(0);
    for (const candidate of candidates) {
      try {
        await peer.addIceCandidate(candidate);
      } catch (error) {
        this.events.emit("error", normalizePeerSignalError(error, connection.remotePeerId));
      }
    }
  }
  attachChannel(connection, channel) {
    connection.channel = channel;
    channel.onopen = () => {
      this.events.emit("peer-open", { peerId: connection.remotePeerId });
    };
    channel.onmessage = (event) => {
      let payload = event.data;
      try {
        payload = JSON.parse(event.data);
      } catch {
      }
      this.handleDataChannelFrame(connection.remotePeerId, payload);
    };
    channel.onerror = () => this.events.emit("error", { code: "ctox_data_channel_error", peerId: connection.remotePeerId });
    channel.onclose = () => this.removeConnection(connection.remotePeerId, "channel-close");
  }
  async handleDataChannelFrame(peerId, payload) {
    if (this.closed) return;
    if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
      await this.handleTransportFrame(peerId, payload);
      return;
    }
    this.events.emit("message", { peerId, payload });
    if (payload?.id === "masterChangeStream$") {
      this.events.emit("master-change", { peerId, result: payload.result });
      return;
    }
    if (payload?.id && (Object.prototype.hasOwnProperty.call(payload, "result") || Object.prototype.hasOwnProperty.call(payload, "error"))) {
      const pending = this.pending.get(payload.id);
      if (!pending) return;
      this.pending.delete(payload.id);
      clearTimeout(pending.timer);
      if (payload.error) {
        pending.reject(payload.error);
      } else {
        pending.resolve(payload.result);
      }
      return;
    }
    if (payload?.id && payload.method) {
      try {
        const result = await this.handleRequest(peerId, payload.method, payload.params || []);
        this.send(peerId, { id: payload.id, result, error: null });
      } catch (error) {
        const normalized = serializeFrameError(error, payload.method);
        this.events.emit("error", normalized);
        this.send(peerId, { id: payload.id, result: null, error: normalized });
      }
    }
  }
  async handleTransportFrame(peerId, payload) {
    this.recordReceivedTransportFrame(payload);
    if (payload.kind === "ack") {
      const transferId2 = String(payload.transferId || "");
      const ackSeq = Number(payload.ackSeq ?? -1);
      for (const [key, pending] of [...this.pendingFrameAcks.entries()]) {
        if (pending.transferId !== transferId2 || pending.peerId !== peerId) continue;
        if (!(payload.final || pending.ackSeq == null || ackSeq >= pending.ackSeq)) continue;
        this.pendingFrameAcks.delete(key);
        clearTimeout(pending.timer);
        this.recordTransportStatus({
          pendingAcks: this.pendingFrameAcks.size,
          lastAckLagMs: pending.sentAtMs ? Date.now() - pending.sentAtMs : this.transportStats.lastAckLagMs,
          resumeAckCount: payload.resume ? this.transportStats.resumeAckCount + 1 : this.transportStats.resumeAckCount
        });
        pending.resolve(payload);
      }
      return;
    }
    if (payload.kind === "start") {
      const transferId2 = String(payload.transferId || "");
      const totalFrames = Number(payload.totalFrames || 0);
      const totalBytes = Number(payload.totalBytes || 0);
      if (!transferId2 || totalFrames < 1 || totalFrames > 1e5 || totalBytes > MAX_TRANSFER_BYTES) {
        this.events.emit("error", {
          code: "ctox_webrtc_frame_start_invalid",
          peerId,
          transferId: transferId2,
          totalBytes
        });
        return;
      }
      this.incomingFrames.set(transferId2, {
        peerId,
        totalFrames,
        totalBytes,
        received: /* @__PURE__ */ new Map(),
        createdAt: Date.now(),
        attempt: Number(payload.attempt || 0),
        nextAckSeq: Math.min(FRAME_ACK_WINDOW - 1, totalFrames - 1)
      });
      this.completedFrameAcks.delete(transferId2);
      this.cleanupCompletedFrameAcks();
      this.recordTransportStatus({
        incomingTransfers: this.incomingFrames.size,
        completedAckCacheSize: this.completedFrameAcks.size
      });
      return;
    }
    if (payload.kind === "resume") {
      const transferId2 = String(payload.transferId || "");
      const completed = this.completedFrameAcks.get(transferId2);
      if (completed && completed.peerId === peerId) {
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: "ack",
          transferId: transferId2,
          ackSeq: completed.ackSeq,
          receivedFrames: completed.receivedFrames,
          final: true,
          resume: true
        });
        return;
      }
      const entry2 = this.incomingFrames.get(transferId2);
      if (entry2 && entry2.peerId === peerId) {
        const ackSeq = highestContiguousSeq(entry2.received, entry2.totalFrames);
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: "ack",
          transferId: transferId2,
          ackSeq,
          receivedFrames: entry2.received.size,
          final: false,
          resume: true
        });
      }
      return;
    }
    if (payload.kind !== "chunk") return;
    const transferId = String(payload.transferId || "");
    const entry = this.incomingFrames.get(transferId);
    if (!entry || entry.peerId !== peerId) {
      this.events.emit("error", {
        code: "ctox_webrtc_frame_chunk_without_start",
        peerId,
        transferId
      });
      return;
    }
    const seq = Number(payload.seq);
    if (!Number.isInteger(seq) || seq < 0 || seq >= entry.totalFrames) {
      this.events.emit("error", {
        code: "ctox_webrtc_frame_chunk_invalid",
        peerId,
        transferId,
        seq
      });
      return;
    }
    const attempt = Number(payload.attempt || 0);
    if (attempt !== Number(entry.attempt || 0)) {
      this.events.emit("error", {
        code: "ctox_webrtc_frame_chunk_stale_attempt",
        peerId,
        transferId,
        seq,
        attempt,
        expectedAttempt: entry.attempt
      });
      return;
    }
    entry.received.set(seq, String(payload.data || ""));
    const contiguousSeq = highestContiguousSeq(entry.received, entry.totalFrames);
    if (entry.received.size !== entry.totalFrames) {
      if (contiguousSeq >= entry.nextAckSeq && contiguousSeq < entry.totalFrames - 1) {
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: "ack",
          transferId,
          ackSeq: contiguousSeq,
          receivedFrames: entry.received.size,
          final: false
        });
        entry.nextAckSeq = Math.min(contiguousSeq + FRAME_ACK_WINDOW, entry.totalFrames - 1);
      }
      return;
    }
    this.incomingFrames.delete(transferId);
    let text = "";
    for (let index = 0; index < entry.totalFrames; index += 1) {
      text += entry.received.get(index) || "";
    }
    if (entry.totalBytes && encodedSize(text) !== entry.totalBytes) {
      this.events.emit("error", {
        code: "ctox_webrtc_frame_size_mismatch",
        peerId,
        transferId,
        expectedBytes: entry.totalBytes,
        actualBytes: encodedSize(text)
      });
      return;
    }
    this.send(peerId, {
      ctoxFrame: CTOX_FRAME_PROTOCOL,
      kind: "ack",
      transferId,
      ackSeq: entry.totalFrames - 1,
      receivedFrames: entry.received.size,
      final: true
    });
    this.completedFrameAcks.set(transferId, {
      peerId,
      ackSeq: entry.totalFrames - 1,
      receivedFrames: entry.received.size,
      expiresAt: Date.now() + COMPLETED_FRAME_ACK_TTL_MS
    });
    this.cleanupCompletedFrameAcks();
    this.recordTransportStatus({
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size
    });
    try {
      await this.handleDataChannelFrame(peerId, JSON.parse(text));
    } catch (error) {
      this.events.emit("error", {
        code: "ctox_webrtc_frame_decode_failed",
        peerId,
        transferId,
        message: error?.message || String(error)
      });
    }
  }
  async handleRequest(peerId, method, params) {
    this.recordObservedRequest(peerId, method);
    if (method === "token") {
      return this.options.storageToken;
    }
    if (method === "ctoxProtocol") {
      return this.protocolPayload(peerId, params);
    }
    const handler = this.options.requestHandlers?.[method];
    if (typeof handler === "function") {
      return handler({ peerId, params, peer: this });
    }
    return {
      code: "ctox_unknown_webrtc_method",
      phase: "replication-io",
      direction: "unknown",
      method
    };
  }
  recordObservedRequest(peerId, method) {
    const key = requestObservationKey(peerId, method);
    this.observedRequests.set(key, Date.now());
    const waiters = this.requestWaiters.get(key) || [];
    this.requestWaiters.delete(key);
    for (const waiter of waiters) {
      clearTimeout(waiter.timer);
      waiter.resolve();
    }
    this.events.emit("request-observed", { peerId, method });
  }
  hasObservedRequest(peerId, method) {
    return this.observedRequests.has(requestObservationKey(peerId, method));
  }
  waitForRequest(peerId, method, timeoutMs = 2e3) {
    if (this.hasObservedRequest(peerId, method)) {
      return Promise.resolve();
    }
    const key = requestObservationKey(peerId, method);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const waiters2 = (this.requestWaiters.get(key) || []).filter((item) => item.resolve !== resolve);
        if (waiters2.length) this.requestWaiters.set(key, waiters2);
        else this.requestWaiters.delete(key);
        reject(new Error(`Timed out waiting for remote WebRTC request ${method}`));
      }, timeoutMs);
      const waiters = this.requestWaiters.get(key) || [];
      waiters.push({ resolve, reject, timer });
      this.requestWaiters.set(key, waiters);
    });
  }
  async protocolPayload(peerId, params = []) {
    if (typeof this.options.protocolPayload === "function") {
      return this.options.protocolPayload({ peerId, params, peer: this });
    }
    return buildProtocolPayload({
      role: this.options.role,
      peerSessionId: `${this.options.role}:${this.options.clientId}`,
      peerGeneration: 1,
      capabilities: this.options.capabilities
    });
  }
  sendSignal(remotePeerId, signal) {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      this.events.emit("error", { code: "ctox_signaling_socket_not_open", peerId: remotePeerId });
      return false;
    }
    this.socket.send(JSON.stringify({
      type: "signal",
      room: this.options.room,
      senderPeerId: this.options.clientId,
      receiverPeerId: remotePeerId,
      receiver: remotePeerId,
      target: remotePeerId,
      data: signal
    }));
    return true;
  }
  removeConnection(remotePeerId, reason = "closed") {
    const peerId = String(remotePeerId || "");
    const connection = this.connections.get(peerId);
    if (!connection) return;
    this.connections.delete(peerId);
    try {
      connection.channel?.close?.();
    } catch {
    }
    try {
      connection.peer?.close?.();
    } catch {
    }
    this.rejectPendingForPeer(peerId, createPeerClosedError(peerId, reason));
    this.events.emit("peer-close", { peerId, reason });
  }
  rememberPeerMetadata(peerId, metadata = {}) {
    const normalized = normalizePeerMetadata({ ...metadata, peerId });
    if (!normalized.peerId || normalized.peerId === this.options.clientId) return;
    this.peerMetadata.set(normalized.peerId, {
      ...this.peerMetadata.get(normalized.peerId) || {},
      ...normalized
    });
  }
  shouldConnectToRemotePeer(remotePeerId) {
    const peerId = String(remotePeerId || "");
    if (!peerId || peerId === this.options.clientId) return false;
    const metadata = this.peerMetadata.get(peerId);
    if (!metadata?.role) return true;
    return metadata.role === "ctox_instance";
  }
  rejectPendingForPeer(peerId, error) {
    for (const [id, pending] of [...this.pending.entries()]) {
      if (pending.peerId !== peerId) continue;
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [transferId, pending] of [...this.pendingFrameAcks.entries()]) {
      if (pending.peerId !== peerId) continue;
      this.pendingFrameAcks.delete(transferId);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [transferId, entry] of [...this.incomingFrames.entries()]) {
      if (entry.peerId === peerId) this.incomingFrames.delete(transferId);
    }
  }
  rejectAllPending(error) {
    for (const [id, pending] of [...this.pending.entries()]) {
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    for (const [key, waiters] of [...this.requestWaiters.entries()]) {
      this.requestWaiters.delete(key);
      for (const waiter of waiters) {
        clearTimeout(waiter.timer);
        waiter.reject(error);
      }
    }
    for (const [transferId, pending] of [...this.pendingFrameAcks.entries()]) {
      this.pendingFrameAcks.delete(transferId);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.incomingFrames.clear();
    this.completedFrameAcks.clear();
    for (const connection of this.connections.values()) {
      if (connection.sendQueue) {
        connection.sendQueue.high = [];
        connection.sendQueue.normal = [];
        connection.sendQueue.low = [];
      }
    }
    this.recordTransportStatus({
      pendingAcks: 0,
      incomingTransfers: 0,
      completedAckCacheSize: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0
    });
  }
  getTransportStatus() {
    return {
      ...this.transportStats,
      pendingAcks: this.pendingFrameAcks.size,
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size
    };
  }
  recordSentTransportFrame(payload, channel) {
    this.recordTransportStatus({
      sentFrames: this.transportStats.sentFrames + 1,
      sentBytes: this.transportStats.sentBytes + encodedSize(JSON.stringify(payload)),
      lastBufferedAmount: Number(channel?.bufferedAmount || 0)
    });
  }
  recordReceivedTransportFrame(payload) {
    this.recordTransportStatus({
      receivedFrames: this.transportStats.receivedFrames + 1,
      receivedBytes: this.transportStats.receivedBytes + encodedSize(JSON.stringify(payload))
    });
  }
  recordTransportStatus(patch = {}) {
    Object.assign(this.transportStats, patch, { updatedAtMs: Date.now() });
    this.events.emit("transport-status", this.getTransportStatus());
  }
  refreshSendQueueStatus(connection = null) {
    let high = 0;
    let normal = 0;
    let low = 0;
    const connections = connection ? [connection] : this.connections.values();
    for (const entry of connections) {
      const queue = entry?.sendQueue;
      if (!queue) continue;
      high += queue.high.length;
      normal += queue.normal.length;
      low += queue.low.length;
    }
    this.recordTransportStatus({
      priorityQueueDepth: high + normal + low,
      highPriorityQueueDepth: high,
      normalPriorityQueueDepth: normal,
      lowPriorityQueueDepth: low
    });
  }
  cleanupCompletedFrameAcks() {
    const now = Date.now();
    for (const [transferId, completed] of [...this.completedFrameAcks.entries()]) {
      if (completed.expiresAt <= now || this.completedFrameAcks.size > 512) {
        this.completedFrameAcks.delete(transferId);
      }
    }
  }
};
function normalizeSignalingControlPlaneError(payload = {}) {
  if (!payload || typeof payload !== "object") {
    return {
      name: "Error",
      code: "ctox_signaling_unknown_error",
      message: "Unknown WebRTC signaling error."
    };
  }
  const code = typeof payload.code === "string" && payload.code.trim() ? payload.code.trim() : "control_plane_rejected";
  const reason = typeof payload.reason === "string" && payload.reason.trim() ? payload.reason.trim() : typeof payload.message === "string" && payload.message.trim() ? payload.message.trim() : code;
  if (payload.type === "ctoxError" && payload.scope === "control-plane") {
    return {
      name: "CtoxSignalingControlPlaneError",
      type: payload.type,
      scope: payload.scope,
      code,
      phase: "signaling-control-plane",
      severity: "error",
      retryable: false,
      message: reason
    };
  }
  return {
    ...payload,
    code,
    message: reason
  };
}
function createPeerClosedError(peerId, reason) {
  const error = new Error(`WebRTC peer ${peerId} closed: ${reason}`);
  error.code = "ERR_CONNECTION_FAILURE";
  error.peerId = peerId;
  error.reason = reason;
  error.lifecycle = true;
  return error;
}
async function rollbackLocalDescription(peer) {
  if (!peer || peer.signalingState === "stable" || peer.signalingState === "closed") return;
  try {
    await peer.setLocalDescription({ type: "rollback" });
  } catch {
  }
}
function normalizePeerSignalError(error, peerId) {
  const message = String(error?.message || error || "");
  const name = typeof error?.name === "string" ? error.name : "Error";
  if (message.includes("Called in wrong state: stable") || message.includes("remote description was null") || message.includes("The remote description was null")) {
    return {
      name: "CtoxWebRtcPeerLifecycleEvent",
      code: "peer_signal_stale",
      phase: "peer-reconnect",
      severity: "recoverable",
      retryable: true,
      lifecycle: true,
      peerId,
      message: "Stale WebRTC signaling arrived after peer state changed; reconnect repair will keep the RxDB data channel authoritative."
    };
  }
  return {
    name,
    code: error?.code || (isMissingRemoteDescriptionIceError(error) ? "ERR_ADD_ICE_CANDIDATE" : "ERR_SET_REMOTE_DESCRIPTION"),
    phase: "peer-signaling",
    severity: "error",
    retryable: true,
    peerId,
    message
  };
}
function isMissingRemoteDescriptionIceError(error) {
  const message = String(error?.message || error || "");
  return message.includes("remote description was null") || message.includes("The remote description was null");
}
function serializeFrameError(error, method = "") {
  if (error && typeof error === "object") {
    return {
      name: error.name || "Error",
      code: error.code || "ctox_webrtc_request_failed",
      method,
      message: error.message || String(error),
      retryable: Boolean(error.retryable),
      lifecycle: Boolean(error.lifecycle)
    };
  }
  return {
    name: "Error",
    code: "ctox_webrtc_request_failed",
    method,
    message: String(error || "Unknown WebRTC request failure"),
    retryable: false,
    lifecycle: false
  };
}
function signalingPeerDescriptors(message = {}) {
  const descriptors = [];
  const append = (entry) => {
    if (typeof entry === "string") {
      descriptors.push({ peerId: entry });
      return;
    }
    if (!entry || typeof entry !== "object") return;
    const peerId = entry.peerId || entry.id || entry.clientId || entry.client;
    if (!peerId) return;
    descriptors.push(normalizePeerMetadata({ ...entry, peerId }));
  };
  for (const entry of Array.isArray(message.peers) ? message.peers : []) append(entry);
  for (const entry of Array.isArray(message.otherPeerIds) ? message.otherPeerIds : []) append(entry);
  const seen = /* @__PURE__ */ new Set();
  return descriptors.filter((descriptor) => {
    if (!descriptor.peerId || seen.has(descriptor.peerId)) return false;
    seen.add(descriptor.peerId);
    return true;
  });
}
function normalizePeerMetadata(entry = {}) {
  const capabilities = Array.isArray(entry.capabilities) ? entry.capabilities.filter((capability) => typeof capability === "string" && capability.trim()).map((capability) => capability.trim()) : [];
  return {
    peerId: typeof entry.peerId === "string" ? entry.peerId : String(entry.peerId || ""),
    role: typeof entry.role === "string" ? entry.role.trim() : "",
    protocol: typeof entry.protocol === "string" ? entry.protocol.trim() : "",
    instanceId: typeof entry.instanceId === "string" ? entry.instanceId.trim() : "",
    client: typeof entry.client === "string" ? entry.client.trim() : "",
    capabilities
  };
}
function buildSignalingUrl(options) {
  const url = new URL(options.signalingUrl);
  url.searchParams.set("room", options.room);
  url.searchParams.set("peerId", options.clientId);
  url.searchParams.set("client", options.clientId);
  url.searchParams.set("role", options.role);
  url.searchParams.set("protocol", CTOX_RXDB_PROTOCOL);
  if (options.instanceId) url.searchParams.set("instance_id", options.instanceId);
  if (options.roomPassword) url.searchParams.set("room_password", options.roomPassword);
  if (options.token) url.searchParams.set("token", options.token);
  if (options.tokenIssuedAt) url.searchParams.set("token_iat", String(options.tokenIssuedAt));
  if (options.tokenExpiresAt) url.searchParams.set("token_exp", String(options.tokenExpiresAt));
  for (const capability of options.capabilities || []) {
    url.searchParams.append("cap", capability);
  }
  return url.toString();
}
function redactUrl(value) {
  const url = new URL(value);
  for (const key of ["room_password", "token"]) {
    if (url.searchParams.has(key)) {
      url.searchParams.set(key, "[redacted]");
    }
  }
  return url.toString();
}
function randomId(prefix) {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  const suffix = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${prefix}-${suffix}`;
}
function requestObservationKey(peerId, method) {
  return `${peerId || ""}|${method || ""}`;
}
function encodedSize(value) {
  return new TextEncoder().encode(String(value || "")).byteLength;
}
function highestContiguousSeq(received, totalFrames) {
  let seq = -1;
  for (let index = 0; index < totalFrames; index += 1) {
    if (!received.has(index)) break;
    seq = index;
  }
  return seq;
}
function createSendQueue() {
  return {
    high: [],
    normal: [],
    low: [],
    draining: false,
    nextSequence: 0
  };
}
function nextQueuedSend(queue) {
  for (const priority of SEND_PRIORITIES) {
    if (queue[priority].length) {
      return queue[priority].shift();
    }
  }
  return null;
}
function classifySendPriority(payload = {}, text = "") {
  if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
    return ["ack", "resume", "start"].includes(payload.kind) ? "high" : "normal";
  }
  const method = String(payload?.method || "");
  if (["ctoxProtocol", "token"].includes(method)) return "high";
  if (method === "masterWrite" && encodedSize(text) > MAX_INLINE_FRAME_BYTES) return "low";
  if (method === "masterChangesSince") return "normal";
  if (payload?.id && (Object.prototype.hasOwnProperty.call(payload, "result") || Object.prototype.hasOwnProperty.call(payload, "error"))) {
    return "high";
  }
  return "normal";
}
function frameAckKey(transferId, ackSeq) {
  return `${transferId}|${ackSeq == null ? "final" : ackSeq}`;
}
function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// src/apps/business-os/rxdb/src/observable.mjs
var CtoxSubject = class {
  constructor(initialValue) {
    this.value = initialValue;
    this.listeners = /* @__PURE__ */ new Set();
  }
  next(value) {
    this.value = value;
    for (const listener of [...this.listeners]) {
      listener(value);
    }
  }
  subscribe(listener) {
    this.listeners.add(listener);
    if (this.value !== void 0) {
      listener(this.value);
    }
    return {
      unsubscribe: () => this.listeners.delete(listener)
    };
  }
  getValue() {
    return this.value;
  }
};

// src/apps/business-os/rxdb/src/replication-webrtc.mjs
var BROWSER_CAPABILITIES = [
  "ctox-rxdb-browser-v1",
  "ctox-file-chunks-v1",
  "ctox-schema-hash-v1",
  "ctox-peer-session-v1",
  "ctox-checkpoint-epoch-v1"
];
function getConnectionHandlerSimplePeer({ signalingServerUrl, config } = {}) {
  return {
    kind: "ctox-native-webrtc",
    signalingServerUrl,
    config: config || {}
  };
}
async function replicateWebRTC({
  collection,
  topic,
  connectionHandlerCreator,
  pull = { batchSize: 10 },
  push = { batchSize: 10 },
  retryTime = 5e3,
  ctox = {}
} = {}) {
  if (!collection) throw new Error("replicateWebRTC requires collection");
  if (!topic) throw new Error("replicateWebRTC requires topic");
  const state = new CtoxWebRtcReplicationState({ collection, topic, pull, push, retryTime, ctox });
  await state.start(connectionHandlerCreator);
  return state;
}
var CtoxWebRtcReplicationState = class {
  constructor({ collection, topic, pull, push, retryTime, ctox }) {
    this.collection = collection;
    this.topic = topic;
    this.pull = pull;
    this.push = push;
    this.retryTime = retryTime;
    this.ctox = ctox;
    this.error$ = new CtoxSubject();
    this.active$ = new CtoxSubject(false);
    this.canceled$ = new CtoxSubject(false);
    this.peerStates$ = new CtoxSubject(/* @__PURE__ */ new Map());
    this.transportStatus$ = new CtoxSubject({});
    this.peer = null;
    this.initialReplicationDeferred = createDeferred();
    this.initialReplication = this.initialReplicationDeferred.promise;
    this.cancelled = false;
    this.pullCheckpointsByPeer = /* @__PURE__ */ new Map();
    this.pushCheckpointsByPeer = /* @__PURE__ */ new Map();
    this.changeSubscription = null;
    this.periodicPullTimer = null;
    this.periodicPushTimer = null;
    this.pullInProgress = false;
    this.pushInProgress = false;
    this.peerOpenQueue = Promise.resolve();
  }
  async start(connectionHandlerCreator) {
    const schemaHashValue = await this.collection.schema.hash();
    const signalingUrl = connectionHandlerCreator?.signalingServerUrl;
    const iceServers = connectionHandlerCreator?.config?.iceServers || [];
    this.peer = createCtoxWebRtcNativePeer({
      signalingUrl,
      room: this.topic,
      role: "browser",
      capabilities: BROWSER_CAPABILITIES,
      iceServers,
      protocolPayload: async () => {
        const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(schemaHashValue);
        return buildProtocolPayload({
          collectionName: this.collection.name,
          schemaVersion: this.collection.schema.version,
          schemaHash: schemaHashValue,
          schemaHashSource: schemaHashSource(this.collection.name),
          peerSessionId: `browser:${this.topic}`,
          peerGeneration: 1,
          checkpoint,
          role: "browser",
          capabilities: BROWSER_CAPABILITIES
        });
      },
      requestHandlers: {
        masterChangesSince: async ({ peerId, params }) => this.masterChangesSince(params, peerId),
        masterWrite: async ({ peerId, params }) => this.masterWrite(params, peerId)
      }
    });
    this.peer.on("error", (event) => this.error$.next(event.detail || event));
    this.peer.on("transport-status", (event) => {
      this.transportStatus$.next(this.decorateTransportStatus(event.detail || event));
    });
    this.peer.on("peer-open", (event) => {
      const peerId = event.detail.peerId;
      this.peerOpenQueue = this.peerOpenQueue.then(() => this.handlePeerOpen(peerId)).catch((error) => this.error$.next(error));
    });
    this.peer.on("peer-close", (event) => {
      this.removePeer(event.detail?.peerId, event.detail?.reason || "peer-close");
    });
    this.peer.on("peer-state", (event) => {
      const state = event.detail?.state || "";
      if (["closed", "failed", "disconnected"].includes(state)) {
        this.removePeer(event.detail?.peerId, `peer-${state}`);
      }
    });
    this.peer.on("master-change", () => {
      this.pullFromRemotePeers().catch((error) => this.error$.next(error));
    });
    this.peer.connect();
    this.changeSubscription = this.collection.observe(() => {
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    });
    const periodicPullMs = this.periodicPullIntervalMs();
    if (periodicPullMs > 0) {
      this.periodicPullTimer = setInterval(() => {
        this.pullFromRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPullMs);
    }
    const periodicPushMs = this.periodicPushIntervalMs();
    if (periodicPushMs > 0) {
      this.periodicPushTimer = setInterval(() => {
        this.pushToRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPushMs);
    }
  }
  async handlePeerOpen(peerId) {
    const localProtocol = await this.peer.protocolPayload(peerId);
    const remoteProtocol = await this.peer.request(peerId, "ctoxProtocol", [
      localProtocol
    ]);
    const normalizedRemoteProtocol = normalizeRemoteProtocol(remoteProtocol);
    assertCompatibleProtocol(localProtocol, normalizedRemoteProtocol, {
      requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES
    });
    if (normalizedRemoteProtocol?.peerSession?.role !== "ctox_instance") {
      this.peer?.removeConnection?.(peerId, "non-native-peer-role");
      return;
    }
    this.ctox?.onPeerProtocol?.(normalizedRemoteProtocol);
    await this.peer.request(peerId, "token", []);
    await this.awaitRemoteMasterReady(peerId);
    this.pruneReplacedNativePeers(peerId, normalizedRemoteProtocol);
    const peerStates = new Map(this.peerStates$.getValue() || /* @__PURE__ */ new Map());
    peerStates.set(peerId, {
      peerId,
      replicationState: this,
      remoteProtocol: normalizedRemoteProtocol
    });
    this.peerStates$.next(this.retainOnlyNativePeer(peerId, normalizedRemoteProtocol, peerStates));
    this.active$.next(true);
    try {
      this.initialReplication = this.pullFromRemotePeers().then(() => this.pushToRemotePeers());
      await this.initialReplication;
      this.resolveInitialReplication();
    } catch (error) {
      this.rejectInitialReplication(error);
      throw error;
    }
  }
  async awaitRemoteMasterReady(peerId) {
    try {
      await this.peer.waitForRequest?.(peerId, "token", 2e3);
    } catch {
    }
    await delay2(100);
  }
  async pullFromRemotePeers() {
    if (this.pullInProgress) return;
    this.pullInProgress = true;
    const peerIds = this.openPeerIds();
    try {
      const results = await Promise.allSettled(peerIds.map((peerId) => this.pullFromPeer(peerId)));
      this.reportPeerResults(results, peerIds);
    } finally {
      this.pullInProgress = false;
    }
  }
  async pullFromPeer(peerId) {
    const batchSize = Number(this.pull?.batchSize || 10);
    let checkpoint = this.pullCheckpointsByPeer.get(peerId) || null;
    while (!this.cancelled) {
      const result = await this.requestMasterChangesSince(
        peerId,
        checkpoint,
        batchSize
      );
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (documents.length) {
        await this.collection.storageCollection.bulkWrite(documents, {
          replicationOrigin: this.replicationOriginForPeer(peerId)
        });
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pullCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }
  async requestMasterChangesSince(peerId, checkpoint, batchSize) {
    const timeoutMs = this.requestTimeoutMsFor("masterChangesSince");
    const maxAttempts = 2;
    let lastError = null;
    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      try {
        return await this.peer.request(
          peerId,
          "masterChangesSince",
          [checkpoint, batchSize],
          timeoutMs
        );
      } catch (error) {
        lastError = error;
        if (attempt >= maxAttempts || this.cancelled || !this.isPeerOpen(peerId) || !this.isTransientMasterChangesSinceError(error)) {
          throw error;
        }
        await delay2(250);
      }
    }
    throw lastError;
  }
  async pushToRemotePeers() {
    if (!this.push) return;
    if (this.pushInProgress) return;
    this.pushInProgress = true;
    const peerIds = this.openPeerIds();
    try {
      const results = await Promise.allSettled(peerIds.map((peerId) => this.pushToPeer(peerId)));
      this.reportPeerResults(results, peerIds);
    } finally {
      this.pushInProgress = false;
    }
  }
  async pushToPeer(peerId) {
    if (!this.push || this.cancelled) return;
    const batchSize = Number(this.push?.batchSize || 10);
    let checkpoint = this.pushCheckpointsByPeer.get(peerId) || null;
    while (!this.cancelled) {
      const result = await this.collection.storageCollection.getChangedDocumentsSince(
        checkpoint,
        batchSize,
        this.changedDocumentReadOptionsForPeer(peerId)
      );
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (!documents.length) {
        checkpoint = result?.checkpoint || checkpoint;
        this.pushCheckpointsByPeer.set(peerId, checkpoint);
        break;
      }
      const rows = documents.map((doc) => ({
        newDocumentState: doc,
        assumedMasterState: null
      }));
      await this.peer.request(peerId, "masterWrite", [rows], this.requestTimeoutMsFor("masterWrite"));
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }
  async masterChangesSince(params, peerId = "") {
    const checkpoint = params?.[0] || null;
    const batchSize = Number(params?.[1] || this.pull?.batchSize || 10);
    return this.collection.storageCollection.getChangedDocumentsSince(
      checkpoint,
      batchSize,
      this.changedDocumentReadOptionsForPeer(peerId)
    );
  }
  async masterWrite(params, peerId = "") {
    const rows = Array.isArray(params?.[0]) ? params[0] : [];
    const docs = rows.map((row) => row?.newDocumentState || row?.document || row).filter(Boolean);
    if (docs.length) {
      await this.collection.storageCollection.bulkWrite(docs, {
        replicationOrigin: this.replicationOriginForPeer(peerId)
      });
    }
    return [];
  }
  awaitInitialReplication() {
    return this.initialReplication;
  }
  awaitInSync() {
    return Promise.resolve().then(() => this.awaitInitialReplication()).then(() => this.pullFromRemotePeers()).then(() => this.pushToRemotePeers());
  }
  getTransportStatus() {
    return this.decorateTransportStatus(this.peer?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {});
  }
  async cancel() {
    this.cancelled = true;
    this.rejectInitialReplication(new Error("WebRTC replication cancelled"));
    this.active$.next(false);
    this.canceled$.next(true);
    this.changeSubscription?.unsubscribe?.();
    if (this.periodicPullTimer) {
      clearInterval(this.periodicPullTimer);
      this.periodicPullTimer = null;
    }
    if (this.periodicPushTimer) {
      clearInterval(this.periodicPushTimer);
      this.periodicPushTimer = null;
    }
    this.peer?.close?.();
  }
  resolveInitialReplication() {
    this.initialReplicationDeferred?.resolve?.(true);
  }
  rejectInitialReplication(error) {
    this.initialReplicationDeferred?.reject?.(error);
  }
  removePeer(peerId, reason = "closed") {
    if (!peerId) return;
    const peerStates = new Map(this.peerStates$.getValue() || /* @__PURE__ */ new Map());
    if (!peerStates.has(peerId)) return;
    peerStates.delete(peerId);
    this.pullCheckpointsByPeer.delete(peerId);
    this.pushCheckpointsByPeer.delete(peerId);
    this.peerStates$.next(peerStates);
    if (!peerStates.size) this.active$.next(false);
    this.ctox?.onPeerClose?.({ peerId, reason });
  }
  pruneReplacedNativePeers(activePeerId, remoteProtocol) {
    if (remoteProtocol?.peerSession?.role !== "ctox_instance") return;
    const peerStates = new Map(this.peerStates$.getValue() || /* @__PURE__ */ new Map());
    let changed = false;
    for (const [peerId, state] of peerStates.entries()) {
      if (peerId === activePeerId) continue;
      if (state?.remoteProtocol?.peerSession?.role !== "ctox_instance") continue;
      peerStates.delete(peerId);
      this.pullCheckpointsByPeer.delete(peerId);
      this.pushCheckpointsByPeer.delete(peerId);
      this.peer?.removeConnection?.(peerId, "native-peer-replaced");
      changed = true;
    }
    if (changed) this.peerStates$.next(peerStates);
  }
  retainOnlyNativePeer(activePeerId, remoteProtocol, peerStates) {
    if (remoteProtocol?.peerSession?.role !== "ctox_instance") return peerStates;
    const activeState = peerStates.get(activePeerId);
    const nextPeerStates = /* @__PURE__ */ new Map([[activePeerId, activeState]]);
    for (const peerId of peerStates.keys()) {
      if (peerId === activePeerId) continue;
      this.pullCheckpointsByPeer.delete(peerId);
      this.pushCheckpointsByPeer.delete(peerId);
      this.peer?.removeConnection?.(peerId, "native-peer-retained-singleton");
    }
    return nextPeerStates;
  }
  remoteProtocolForPeer(peerId) {
    return (this.peerStates$.getValue() || /* @__PURE__ */ new Map()).get(peerId)?.remoteProtocol || null;
  }
  replicationOriginForPeer(peerId) {
    const remoteProtocol = this.remoteProtocolForPeer(peerId);
    const peerSession = remoteProtocol?.peerSession || {};
    const role = typeof peerSession.role === "string" ? peerSession.role : "";
    if (!role) return null;
    return {
      role,
      peerId,
      sessionId: typeof peerSession.sessionId === "string" ? peerSession.sessionId : "",
      collection: this.collection.name
    };
  }
  changedDocumentReadOptionsForPeer(peerId) {
    const role = this.replicationOriginForPeer(peerId)?.role || "";
    return role ? { excludeReplicationOriginRole: role } : {};
  }
  requestTimeoutMsFor(method) {
    if (this.collection.name === "desktop_file_chunks") {
      return method === "masterChangesSince" ? 45e3 : 3e4;
    }
    return 15e3;
  }
  periodicPullIntervalMs() {
    return this.collection.name === "desktop_file_chunks" ? 250 : 0;
  }
  periodicPushIntervalMs() {
    return ["business_commands", "ctox_queue_tasks"].includes(this.collection.name) ? 1e3 : 0;
  }
  openPeerIds() {
    const peerStates = this.peerStates$.getValue() || /* @__PURE__ */ new Map();
    const open = [];
    for (const peerId of peerStates.keys()) {
      if (this.isPeerOpen(peerId)) {
        open.push(peerId);
      } else {
        this.removePeer(peerId, "peer-not-open");
      }
    }
    return open;
  }
  isPeerOpen(peerId) {
    const connection = this.peer?.connections?.get?.(peerId);
    if (!connection) return false;
    const channelState = connection.channel?.readyState || "";
    const pcState = connection.peer?.connectionState || "";
    return channelState === "open" && !["closed", "failed", "disconnected"].includes(pcState);
  }
  isTransientMasterChangesSinceError(error) {
    const message = typeof error?.message === "string" ? error.message : String(error || "");
    return message.includes("Timed out waiting for WebRTC response masterChangesSince");
  }
  decorateTransportStatus(status = {}) {
    return {
      ...status,
      collection: this.collection.name,
      topic: this.topic,
      activePeerCount: (this.peerStates$.getValue?.() || /* @__PURE__ */ new Map()).size,
      pullInProgress: this.pullInProgress,
      pushInProgress: this.pushInProgress,
      updatedAtMs: Date.now()
    };
  }
  reportPeerResults(results, peerIds) {
    results.forEach((result, index) => {
      if (result.status !== "rejected") return;
      const peerId = peerIds[index];
      if (this.shouldRetainPeerAfterError(peerId, result.reason)) {
        this.error$.next(result.reason);
        return;
      }
      this.removePeer(peerId, result.reason?.message || "request-failed");
      this.error$.next(result.reason);
    });
  }
  shouldRetainPeerAfterError(peerId, error) {
    return this.isPeerOpen(peerId) && this.isTransientMasterChangesSinceError(error);
  }
};
function delay2(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
function createDeferred() {
  let settled = false;
  let resolve;
  let reject;
  const promise = new Promise((promiseResolve, promiseReject) => {
    resolve = (value) => {
      if (settled) return;
      settled = true;
      promiseResolve(value);
    };
    reject = (error) => {
      if (settled) return;
      settled = true;
      promiseReject(error);
    };
  });
  return { promise, resolve, reject };
}
function normalizeRemoteProtocol(payload) {
  if (!payload || typeof payload !== "object") return payload;
  return {
    ...payload,
    checkpoint: payload.checkpoint || payload.collection?.checkpoint || null
  };
}

// src/apps/business-os/rxdb/src/rx-database.mjs
function getRxStorageDexie() {
  return { name: "ctox-indexeddb-native" };
}
async function createRxDatabase({
  name,
  storage = getRxStorageDexie(),
  multiInstance = false,
  closeDuplicates = true
} = {}) {
  if (!name) {
    throw new Error("createRxDatabase requires a name");
  }
  const nativeStorage = storage?.nativeStorage || await openCtoxIndexedDbStorage({ databaseName: name });
  return new CtoxRxDatabase({
    name,
    storage: nativeStorage,
    multiInstance,
    closeDuplicates
  });
}
async function removeRxDatabase(name) {
  if (!name || !globalThis.indexedDB?.deleteDatabase) return;
  await new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error || new Error(`Failed to delete IndexedDB ${name}`));
    request.onblocked = () => resolve();
  });
}
function addRxPlugin(_ignored = null) {
  return void 0;
}
var RxDBMigrationSchemaPlugin = {
  name: "ctox-JS-migration-schema-placeholder"
};
function rxdbCore() {
  return {
    CTOX_CHECKPOINT_EPOCH_CAPABILITY,
    CTOX_BUSINESS_OS_SCHEMA_HASHES,
    CTOX_PEER_SESSION_CAPABILITY,
    CTOX_PROTOCOL_ERROR_CODES,
    CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
    CTOX_RXDB_PROTOCOL,
    CTOX_SCHEMA_HASH_SOURCES,
    CTOX_SCHEMA_HASH_CAPABILITY,
    addRxPlugin,
    buildProtocolPayload,
    canonicalJson,
    createRxDatabase,
    getRxStorageDexie,
    getConnectionHandlerSimplePeer,
    replicateWebRTC,
    removeRxDatabase,
    RxDBMigrationSchemaPlugin,
    schemaHash,
    schemaHashSource,
    sha256Hex
  };
}
var CtoxRxDatabase = class {
  constructor({ name, storage, multiInstance, closeDuplicates }) {
    this.name = name;
    this.storage = storage;
    this.multiInstance = Boolean(multiInstance);
    this.closeDuplicates = Boolean(closeDuplicates);
    this.collections = {};
  }
  async addCollections(collections) {
    for (const [name, definition] of Object.entries(collections || {})) {
      if (this.collections[name]) continue;
      const schema = definition?.schema || definition;
      const collection = new CtoxRxCollection({
        name,
        schema,
        storageCollection: this.storage.collection(name, { schema })
      });
      this.collections[name] = collection;
      this[name] = collection;
    }
    return this.collections;
  }
  collection(name) {
    return this.collections[name] || this[name] || null;
  }
  async close() {
    this.storage.close();
  }
};
var CtoxRxCollection = class {
  constructor({ name, schema, storageCollection }) {
    this.name = name;
    this.schema = {
      jsonSchema: schema,
      version: schema?.version || 0,
      primaryPath: primaryPathFromSchema(schema),
      hash: () => schemaHash(schema, name)
    };
    this.storageCollection = storageCollection;
  }
  async insert(doc) {
    const normalized = normalizeDoc(doc, this.schema.primaryPath);
    await this.storageCollection.bulkWrite([normalized]);
    return new CtoxRxDocument(this, normalized);
  }
  async bulkInsert(docs = []) {
    if (!Array.isArray(docs)) {
      throw new TypeError("bulkInsert expects an array of documents");
    }
    const normalized = docs.map((doc) => normalizeDoc(doc, this.schema.primaryPath));
    await this.storageCollection.bulkWrite(normalized);
    return normalized.map((doc) => new CtoxRxDocument(this, doc));
  }
  async upsert(doc) {
    const normalized = normalizeDoc(doc, this.schema.primaryPath);
    const written = await this.storageCollection.upsert(normalized);
    return new CtoxRxDocument(this, written);
  }
  async atomicUpsert(doc) {
    return this.upsert(doc);
  }
  async bulkUpsert(docs = []) {
    if (!Array.isArray(docs)) {
      throw new TypeError("bulkUpsert expects an array of documents");
    }
    const written = [];
    for (const doc of docs) {
      written.push(await this.upsert(doc));
    }
    return written;
  }
  find(query = {}) {
    return new CtoxRxQuery(this, query, false);
  }
  findOne(idOrQuery) {
    return new CtoxRxQuery(this, idOrQuery, true);
  }
  count(query = {}) {
    return {
      exec: async () => (await this.find(query).exec()).length
    };
  }
  schemaIndexes() {
    return this.storageCollection.schemaIndexes?.() || [];
  }
  queryPlanFor(query = {}) {
    const normalized = normalizeQuery(query, this.schema.primaryPath);
    return this.storageCollection.queryPlanFor?.(normalized) || {
      collection: this.name,
      indexed: false,
      selectorFields: Object.keys(normalized.selector || {}),
      sortFields: normalizeSort(normalized.sort).map((entry) => Object.keys(entry)[0]).filter(Boolean),
      selectedIndex: null
    };
  }
  observe(listener) {
    return this.storageCollection.observe(listener);
  }
  get $() {
    return {
      subscribe: (listener) => {
        let active = true;
        const emit = async () => {
          if (!active) return;
          const documents = await this.find().exec();
          if (active) listener({ collectionName: this.name, documents });
        };
        emit();
        const unsubscribe = this.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            unsubscribe();
          }
        };
      }
    };
  }
};
var CtoxRxQuery = class _CtoxRxQuery {
  constructor(collection, query, single) {
    this.collection = collection;
    this.query = normalizeQuery(query, collection.schema.primaryPath);
    this.single = single;
    this.$ = {
      subscribe: (listener) => {
        let active = true;
        const emit = () => {
          this.exec().then((value) => {
            if (active) listener(value);
          }).catch(() => {
          });
        };
        emit();
        const unsubscribe = this.collection.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            unsubscribe();
          }
        };
      }
    };
  }
  selector(selector = {}) {
    return this._clone({ selector });
  }
  sort(sort = []) {
    return this._clone({ sort: normalizeSort(sort) });
  }
  limit(limit) {
    return this._clone({ limit: normalizePositiveInteger(limit, "limit") });
  }
  skip(skip) {
    return this._clone({ skip: normalizePositiveInteger(skip, "skip") });
  }
  where(field) {
    if (!field || typeof field !== "string") {
      throw new TypeError("where(field) requires a non-empty field path");
    }
    const withOperator = (operator, value) => {
      const current = this.query.selector?.[field];
      const nextValue = current && typeof current === "object" && !Array.isArray(current) ? { ...current, [operator]: value } : { [operator]: value };
      return this._withSelectorPatch({ [field]: nextValue });
    };
    return {
      eq: (value) => this._withSelectorPatch({ [field]: value }),
      ne: (value) => withOperator("$ne", value),
      gt: (value) => withOperator("$gt", value),
      gte: (value) => withOperator("$gte", value),
      lt: (value) => withOperator("$lt", value),
      lte: (value) => withOperator("$lte", value),
      in: (value) => withOperator("$in", value),
      nin: (value) => withOperator("$nin", value),
      exists: (value = true) => withOperator("$exists", value),
      regex: (value) => withOperator("$regex", value)
    };
  }
  async exec() {
    let docs = await this.collection.storageCollection.allDocuments();
    docs = docs.filter((doc) => matchesSelector(doc, this.query.selector));
    docs = sortDocuments(docs, this.query.sort);
    if (Number.isFinite(this.query.skip) && this.query.skip > 0) {
      docs = docs.slice(this.query.skip);
    }
    if (Number.isFinite(this.query.limit)) {
      docs = docs.slice(0, this.query.limit);
    }
    const wrapped = docs.map((doc) => new CtoxRxDocument(this.collection, doc));
    return this.single ? wrapped[0] || null : wrapped;
  }
  _clone(patch = {}) {
    return new _CtoxRxQuery(this.collection, {
      selector: patch.selector ?? this.query.selector,
      sort: patch.sort ?? this.query.sort,
      limit: patch.limit ?? this.query.limit,
      skip: patch.skip ?? this.query.skip
    }, this.single);
  }
  _withSelectorPatch(patch = {}) {
    return this._clone({
      selector: {
        ...this.query.selector || {},
        ...patch
      }
    });
  }
};
var CtoxRxDocument = class {
  constructor(collection, data) {
    this.collection = collection;
    this._data = { ...data };
    Object.assign(this, this._data);
  }
  toJSON() {
    return { ...this._data };
  }
  async patch(fields) {
    return this.incrementalPatch(fields);
  }
  async atomicPatch(fields) {
    return this.incrementalPatch(fields);
  }
  async update(operation) {
    if (operation?.$set && typeof operation.$set === "object") {
      return this.incrementalPatch(operation.$set);
    }
    return this.incrementalPatch(operation || {});
  }
  async incrementalModify(modifier) {
    const current = this.toJSON();
    const next = await modifier({ ...current });
    return this.incrementalPatch(next || current);
  }
  async atomicUpdate(modifier) {
    return this.incrementalModify(modifier);
  }
  async incrementalPatch(fields) {
    const next = {
      ...this._data,
      ...fields,
      updated_at_ms: fields?.updated_at_ms || this._data.updated_at_ms || Date.now()
    };
    await this.collection.storageCollection.upsert(next);
    this._data = next;
    Object.assign(this, next);
    return this;
  }
  async remove() {
    await this.incrementalPatch({ _deleted: true, is_deleted: true, updated_at_ms: Date.now() });
    return this;
  }
};
function normalizeQuery(query, primaryPath) {
  if (typeof query === "string") {
    return { selector: { [primaryPath]: query } };
  }
  if (query && typeof query === "object" && !query.selector && Object.keys(query).length && !query.sort && !query.limit && !query.skip) {
    return { selector: query };
  }
  return {
    selector: query?.selector || {},
    sort: normalizeSort(query?.sort),
    limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : void 0,
    skip: Number.isFinite(Number(query?.skip)) ? Math.max(0, Number(query.skip)) : void 0
  };
}
function matchesSelector(doc, selector = {}) {
  for (const [key, expected] of Object.entries(selector || {})) {
    if (key === "$and") {
      if (!Array.isArray(expected) || !expected.every((item) => matchesSelector(doc, item))) return false;
      continue;
    }
    if (key === "$or") {
      if (!Array.isArray(expected) || !expected.some((item) => matchesSelector(doc, item))) return false;
      continue;
    }
    if (key === "$not") {
      if (matchesSelector(doc, expected)) return false;
      continue;
    }
    const actual = valueAtPath2(doc, key);
    if (expected && typeof expected === "object" && !Array.isArray(expected)) {
      if ("$in" in expected && !isInOperatorMatch(actual, expected.$in)) return false;
      if ("$nin" in expected && isInOperatorMatch(actual, expected.$nin)) return false;
      if ("$eq" in expected && actual !== expected.$eq) return false;
      if ("$ne" in expected && actual === expected.$ne) return false;
      if ("$gt" in expected && !(actual > expected.$gt)) return false;
      if ("$gte" in expected && !(actual >= expected.$gte)) return false;
      if ("$lt" in expected && !(actual < expected.$lt)) return false;
      if ("$lte" in expected && !(actual <= expected.$lte)) return false;
      if ("$exists" in expected && actual !== void 0 !== Boolean(expected.$exists)) return false;
      if ("$regex" in expected && !matchesRegex(actual, expected.$regex)) return false;
      if ("$contains" in expected && !arrayContains(actual, expected.$contains)) return false;
      if ("$elemMatch" in expected && !elemMatch(actual, expected.$elemMatch)) return false;
      continue;
    }
    if (actual !== expected) return false;
  }
  return true;
}
function sortDocuments(docs, sort = []) {
  if (!sort.length) return docs;
  return docs.slice().sort((left, right) => {
    for (const entry of sort) {
      const [key, direction] = Object.entries(entry)[0] || [];
      const factor = direction === "desc" ? -1 : 1;
      const a = valueAtPath2(left, key);
      const b = valueAtPath2(right, key);
      if (a < b) return -1 * factor;
      if (a > b) return 1 * factor;
    }
    return 0;
  });
}
function normalizeSort(sort = []) {
  if (!sort) return [];
  if (typeof sort === "string") return [{ [sort]: "asc" }];
  if (!Array.isArray(sort)) return [];
  return sort.map((entry) => {
    if (typeof entry === "string") return { [entry]: "asc" };
    if (!entry || typeof entry !== "object") return {};
    const [key, direction] = Object.entries(entry)[0] || [];
    if (!key) return {};
    return { [key]: normalizeSortDirection(direction) };
  }).filter((entry) => Object.keys(entry).length);
}
function normalizeSortDirection(direction) {
  if (direction === -1 || direction === "desc" || direction === "DESC") return "desc";
  return "asc";
}
function normalizePositiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new TypeError(`${name} must be a positive number`);
  }
  return Math.floor(parsed);
}
function isInOperatorMatch(actual, candidates) {
  const values = Array.isArray(candidates) ? candidates : [candidates];
  if (Array.isArray(actual)) {
    return actual.some((value) => values.includes(value));
  }
  return values.includes(actual);
}
function matchesRegex(actual, pattern) {
  if (actual === void 0 || actual === null) return false;
  const source = pattern instanceof RegExp ? pattern : new RegExp(String(pattern));
  return source.test(String(actual));
}
function arrayContains(actual, expected) {
  return Array.isArray(actual) && actual.includes(expected);
}
function elemMatch(actual, selector) {
  return Array.isArray(actual) && actual.some((item) => item && typeof item === "object" ? matchesSelector(item, selector) : item === selector);
}
function valueAtPath2(doc, path) {
  return String(path || "").split(".").reduce((value, key) => value?.[key], doc);
}
function setValueAtPath(doc, path, value) {
  const parts = String(path || "").split(".").filter(Boolean);
  if (!parts.length) return;
  let target = doc;
  for (const part of parts.slice(0, -1)) {
    if (!target[part] || typeof target[part] !== "object") {
      target[part] = {};
    }
    target = target[part];
  }
  target[parts[parts.length - 1]] = value;
}
function primaryPathFromSchema(schema) {
  const primary = schema?.primaryKey;
  if (typeof primary === "string") return primary;
  if (primary?.key) return primary.key;
  return "id";
}
function normalizeDoc(doc, primaryPath) {
  if (!doc || typeof doc !== "object") {
    throw new TypeError("document must be an object");
  }
  const normalized = { ...doc };
  const id = normalized.id || normalized._id || valueAtPath2(normalized, primaryPath);
  if (!id) {
    throw new Error(`document is missing primary key ${primaryPath}`);
  }
  normalized.id = String(id);
  if (valueAtPath2(normalized, primaryPath) === void 0) {
    setValueAtPath(normalized, primaryPath, normalized.id);
  }
  normalized._deleted = Boolean(normalized._deleted);
  normalized._meta = {
    ...normalized._meta || {},
    lwt: Number(normalized._meta?.lwt || normalized.updated_at_ms || normalized.updatedAtMs || Date.now())
  };
  return normalized;
}
var ctoxRxdbTestInternals = {
  matchesSelector,
  normalizeDoc,
  normalizeQuery,
  normalizeSort,
  sortDocuments
};
export {
  CTOX_BUSINESS_OS_SCHEMA_HASHES,
  CTOX_CHECKPOINT_EPOCH_CAPABILITY,
  CTOX_PEER_SESSION_CAPABILITY,
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  CTOX_SCHEMA_HASH_CAPABILITY,
  CTOX_SCHEMA_HASH_SOURCES,
  CtoxEventEmitter,
  CtoxIndexedDbCollection,
  CtoxIndexedDbStorage,
  CtoxSubject,
  CtoxWebRtcNativePeer,
  RxDBMigrationSchemaPlugin,
  addRxPlugin,
  assertCompatibleProtocol,
  buildProtocolPayload,
  canonicalJson,
  createCtoxWebRtcNativePeer,
  createRxDatabase,
  ctoxIndexedDbStorageTestInternals,
  ctoxRxdbTestInternals,
  getConnectionHandlerSimplePeer,
  getRxStorageDexie,
  normalizeSignalingControlPlaneError,
  openCtoxIndexedDbStorage,
  removeRxDatabase,
  replicateWebRTC,
  rxdbCore,
  schemaHash,
  schemaHashSource,
  sha256Hex,
  waitForEvent
};
