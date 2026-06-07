// CTOX DB app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs.

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
var CTOX_QUERY_FETCH_CAPABILITY = "ctox-rxdb-query-fetch-v1";
var CTOX_QUERY_RPC = Object.freeze({
  fetch: "rxdb.query.fetch",
  chunk: "rxdb.query.chunk",
  error: "rxdb.query.error",
  cancel: "rxdb.query.cancel",
  maxDocumentsPerChunk: 200,
  maxBytesPerChunk: 262144,
  maxInFlightStreams: 8,
  maxQueryRuntimeMs: 3e4,
  defaultWindowLimit: 200
});
var CTOX_FILE_RPC = Object.freeze({
  fetch: "rxdb.file.fetch",
  chunk: "rxdb.file.chunk",
  error: "rxdb.file.error",
  cancel: "rxdb.file.cancel",
  maxBytesPerChunk: 262144
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
  business_onboarding_state: "264584aef35bc5efc4bbc8b4084155afe07ced859293be18cdd15cc65e8fad91",
  business_profile: "5ff538b175f7432f9d7e798c09777f4c974c2270ca320119131606adac0223bb",
  business_users: "da6d1a192bc21ad59baf2680d8b80faa471a4883457a8d0ad5a533a1afefba42",
  channel_pairing_state: "d93ceef99b772bc57939143bc6ef0044bf816801700d2dbc8f88def356aa246a",
  communication_accounts: "d40ca549e2f112071b6eb39bf0999a743643073279af4471a477cef259275653",
  communication_messages: "10d120234ec23bbe98124d255599f44d2ef68ecb5ff29787b9b647aaf6537b6f",
  communication_threads: "2111d907ee8cc8c7c2c4e9f10a43bc56f217071dbee0610a96b0457ef6473a8d",
  ctox_bug_reports: "f7329368ad5144b8ea740600265f06c6ac19ad049de751cec92818d9d9de94b5",
  ctox_queue_tasks: "2a5c7c35f65a2ad0e35d19902bbb0c45456137c30f046e9d322406872fbd0824",
  ctox_runs: "73df37bddc2e511b0567496f6199089aef436dd598a3e0bf85f462d38b4f3fff",
  ctox_runtime_settings: "3958bb6580e9705f3688fcf453a80ec33c486b43ac6988f015ffc16cb5ac918d",
  ctox_ticket_items: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_events: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_event_routing_state: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_cases: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_clarification_requests: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_self_work_items: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_self_work_notes: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_label_assignments: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_control_bundles: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_approvals: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_verifications: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_writebacks: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  customer_accounts: "9a98ca2106e699119cd958f5fe35baf31a1eaa90aaf2dced51a903e353aa5e47",
  customer_activities: "011f283b5b83c14faecff2f639af4db0d4f8ea97f8373bf513dbb50db251e5d8",
  customer_contacts: "5f7abb4b088c1ce30f12ae1438d75012de41bcd9d46c9b07d8f34478f506a093",
  customer_dedupe_candidates: "5b9503d8708014f6d7210ec37450611b77ee8127557d6dd5d901f1ad4e9c5097",
  customer_files: "2436166ea70232f2beeebc3d2a6841a61a06985805db2968db39d4506ab9277e",
  customer_import_batches: "59c02b9e9d7ea0449d407a9454550476457e2a2ec5af89090b7599eeee493f3b",
  customer_notes: "b0ac4af2ad41f552f589e8cd9c55511fc35efd55c8b0f87012aa52089d1ac0f9",
  customer_opportunities: "222c4446b849ad99db0210a36ebb1911b84a789952ff56489884675d3541cec5",
  customer_tasks: "b9de4bca1c54d10100a19c5453fc1803990d15f803ecb52e6075b61daf1109c4",
  customer_view_filters: "abd2880ebc48b29b72ef205a4b09085ee7acf1bdde3ffebcea08059ed7e99123",
  customer_view_sorts: "be8feb8ba887879e7c223d4883b9b7771a0b137e9523c732d4de3432b3f4dc51",
  customer_views: "c20ecce31428596148a2a4348061465eb8055aee14ef84ed9755d1a84503936e",
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
  iot_agent_status: "c719592fcc4274060d12567b09013cff8dc11b605b790b349e8efac88cfb6ccd",
  iot_agents: "0bf0fed6ea33be5d475e88b7b913fb1675bb1bf5d4361cc3c5eb6befec6480f8",
  iot_alarms: "978c527550ceb781393bba6e9e886714f7c66f60bc2f7b98be55896bb2ccb149",
  iot_asset_types: "5aebcc5fb39fe783d5364ce21c6f50dc929935ad1cef4964ad1ae996221064d3",
  iot_assets: "b56ee809bbf974a07d1a6423753bedc195e49f7ea4a9f0f4077afa54486ff93e",
  iot_attributes: "35a1c2494238fffedd2b6006ff5269bc7183a5ac60e2cd4a4c12ed17a9acabcb",
  iot_datapoints: "6313f3c8671e3406d789877aca842f8bf5b6a7fa2b63a8458dece314a2f55a80",
  iot_realms: "42ff4cfc74268c51602dd3873df95127f9070068aa5d7c1994e80f5275f78ada",
  iot_rulesets: "0232a7ef9501f87ff583848bf29489aff7105d79ea7a1740dbfc357476f799f1",
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
  outbound_letter_templates: "9839d58ede05148b48b2a7e494fc29d4aa94611034a11bc4c73b32de866a7466",
  outbound_meeting_requests: "f04c3249c3a3d8cf7ca6c2a4b51fbb15729035bca707668fbef3988242e69aa2",
  outbound_messages: "93b8e2cea0670112b6499a86a774dafef3cbd289d11725bf57d4e0941ad13006",
  outbound_pipeline_items: "d128a88597977a96b0b2572c0eaeb7c2e5da7d21ae691ff0b0a18e4824fd378c",
  outbound_research_runs: "46573b72d1bd75daf105265b179af2b0b5d9fae5a61e15cf1198e0dc2604a372",
  outbound_sender_assignments: "d57aeee6946976bd082044147591d648583a6493c6c1c320359b0949c3405c78",
  outbound_sequences: "9368f8c42dc026c94549485d230d01ea511358313b64de0100b5f7706bae251b",
  outbound_skillbooks: "a896fd1593614940aa223831a949fbda53e8714c9b5086a4f1949db1ace83c35",
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
  capabilities = [],
  // Phase 3 schema-validation hardening: the per-collection schema-hash map
  // for EVERY collection multiplexed on this one connection. Keyed by
  // collection name. The room handshake runs once off a single representative
  // collection, so this map is the only place the remote learns the schema
  // hash/version of the OTHER collections sharing the DataChannel. The remote
  // validates each entry individually (see `assertCollectionSchemasCompatible`)
  // instead of skipping schema validation wholesale under multiplex.
  collectionSchemas = null,
  // Multiplexed rooms also need per-collection checkpoint evidence. The
  // representative collection's checkpoint is not valid for sibling
  // collections when the room reconnects, especially for file chunk stores
  // where stale checkpoint epochs are a data-corruption signal.
  collectionCheckpoints = null
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
    // `{ collectionName: { schemaVersion, schemaHash, schemaHashSource } }`.
    // Omitted (null) for single-collection rooms so the legacy single-
    // collection handshake stays byte-identical.
    collectionSchemas: normalizeCollectionSchemas(collectionSchemas),
    collectionCheckpoints: normalizeCollectionCheckpoints(collectionCheckpoints),
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
function normalizeCollectionSchemas(map) {
  if (!map || typeof map !== "object") return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== "object") continue;
    out[name] = {
      schemaVersion: Number.isFinite(entry.schemaVersion) ? entry.schemaVersion : null,
      schemaHash: entry.schemaHash || null,
      schemaHashSource: entry.schemaHashSource || schemaHashSource(name)
    };
  }
  return Object.keys(out).length > 0 ? out : null;
}
function normalizeCollectionCheckpoints(map) {
  if (!map || typeof map !== "object") return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== "object") continue;
    out[name] = {
      ...entry,
      collection: typeof entry.collection === "string" && entry.collection ? entry.collection : name
    };
  }
  return Object.keys(out).length > 0 ? out : null;
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
function assertCollectionSchemasCompatible(localSchemas, remote) {
  const mismatches = /* @__PURE__ */ new Map();
  const remoteSchemas = remote && typeof remote.collectionSchemas === "object" && remote.collectionSchemas ? remote.collectionSchemas : {};
  for (const [name, local] of Object.entries(localSchemas || {})) {
    const remoteEntry = remoteSchemas[name];
    if (!remoteEntry || typeof remoteEntry !== "object") continue;
    const localVersion = Number.isFinite(local?.schemaVersion) ? local.schemaVersion : null;
    const remoteVersion = Number.isFinite(remoteEntry.schemaVersion) ? remoteEntry.schemaVersion : null;
    if (localVersion !== null && remoteVersion !== null && localVersion !== remoteVersion) {
      mismatches.set(name, createProtocolCompatibilityError({
        code: CTOX_PROTOCOL_ERROR_CODES.schemaVersionMismatch,
        message: `CTOX RxDB schema version mismatch for ${name}.`,
        expected: localVersion,
        actual: remoteVersion,
        collection: name
      }));
      continue;
    }
    const localHash = local?.schemaHash || null;
    const remoteHash = remoteEntry.schemaHash || null;
    if (localHash && remoteHash && localHash !== remoteHash) {
      mismatches.set(name, createProtocolCompatibilityError({
        code: CTOX_PROTOCOL_ERROR_CODES.schemaHashMismatch,
        message: `CTOX RxDB schema hash mismatch for ${name}.`,
        expected: localHash,
        actual: remoteHash,
        collection: name
      }));
    }
  }
  return mismatches;
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
    const done = idbTransactionDone(tx);
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
      const lwt = documentLwt(doc, now);
      const stored = {
        collection: this.name,
        id,
        lwt,
        deleted: Boolean(doc._deleted),
        indexValues: indexValuesFor(this.indexes, doc),
        doc: normalizeDocument(doc, lwt, replicationOrigin)
      };
      const previous = await idbRequest(store.get([this.name, id]));
      if (!shouldAcceptDocumentWrite(previous, lwt)) {
        continue;
      }
      await idbRequest(store.put(stored));
      success[id] = stored.doc;
    }
    await done;
    if (Object.keys(success).length) {
      this.events.emit("change", {
        collection: this.name,
        success,
        at: now
      });
    }
    return { success, error };
  }
  /// V1.5 eviction hook. Hard-deletes documents from the primary store
  /// (does NOT soft-delete via _deleted=true — the cache layer wants the
  /// row gone, not tombstoned). Caller is responsible for never invoking
  /// this on dirty docs; the sidecar enforces that.
  async hardDeleteByIds(ids) {
    if (!Array.isArray(ids) || !ids.length) return 0;
    const tx = this.db.transaction(DOCUMENT_STORE, "readwrite");
    const store = tx.objectStore(DOCUMENT_STORE);
    let removed = 0;
    for (const id of ids) {
      await idbRequest(store.delete([this.name, String(id)]));
      removed += 1;
    }
    await idbTransactionDone(tx);
    return removed;
  }
  async findDocumentsById(ids, { withDeleted = false } = {}) {
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const result = {};
    for (const id of ids) {
      const record = await idbRequest(store.get([this.name, String(id)]));
      if (record && (withDeleted || !record.deleted)) {
        result[String(id)] = record.doc;
      }
    }
    await done;
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
  async queryDocuments(query = {}, helpers = {}) {
    if (canUseCollectionLwtQuery(query)) {
      return this.queryDocumentsByLwt(query, helpers);
    }
    const docs = await this.allDocuments();
    return applyQueryToDocuments(docs, query, helpers);
  }
  async queryDocumentsByLwt(query = {}, helpers = {}) {
    const { matchesSelector: matchesSelector2 = () => true, sortDocuments: sortDocuments2 = (docs) => docs } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const maxMatches = Number.isFinite(limit) ? skip + limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collectionLwtId");
    const range = IDBKeyRange.bound(
      [this.name, 0, ""],
      [this.name, Number.MAX_SAFE_INTEGER, "\uFFFF"],
      false,
      false
    );
    const documents = [];
    await iterateCursor(index.openCursor(range, "prev"), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector2(record.doc, selector)) {
        documents.push(record.doc);
      }
      return documents.length < maxMatches;
    });
    await idbTransactionDone(tx);
    let sorted = sortDocuments2(documents, query?.sort || []);
    if (skip > 0) sorted = sorted.slice(skip);
    if (Number.isFinite(limit)) sorted = sorted.slice(0, limit);
    return sorted;
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
function shouldAcceptDocumentWrite(existingRecord, incomingLwt) {
  if (!existingRecord) return true;
  const existingLwt = Number(existingRecord.lwt || existingRecord.doc?._meta?.lwt || 0);
  const nextLwt = Number(incomingLwt || 0);
  if (!Number.isFinite(existingLwt) || !Number.isFinite(nextLwt)) return true;
  return nextLwt >= existingLwt;
}
function documentLwt(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0)
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
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
function canUseCollectionLwtQuery(query = {}) {
  if (!Number.isFinite(query?.limit)) return false;
  const sortFields = normalizeSortFields(query?.sort);
  if (!sortFields.length) return false;
  const firstSort = sortFields[0];
  if (!["updated_at_ms", "updatedAtMs", "_meta.lwt"].includes(firstSort)) return false;
  const firstSortEntry = Array.isArray(query?.sort) ? query.sort[0] : null;
  const direction = typeof firstSortEntry === "string" ? "asc" : String(Object.values(firstSortEntry || {})[0] || "").toLowerCase();
  return ["desc", "-1"].includes(direction);
}
function applyQueryToDocuments(docs = [], query = {}, helpers = {}) {
  const { matchesSelector: matchesSelector2 = () => true, sortDocuments: sortDocuments2 = (items) => items } = helpers || {};
  let filtered = docs.filter((doc) => matchesSelector2(doc, query?.selector || {}));
  filtered = sortDocuments2(filtered, query?.sort || []);
  if (Number.isFinite(query?.skip) && query.skip > 0) {
    filtered = filtered.slice(query.skip);
  }
  if (Number.isFinite(query?.limit)) {
    filtered = filtered.slice(0, query.limit);
  }
  return filtered;
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
  selectBestIndex,
  shouldAcceptDocumentWrite
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
var MAX_GLOBAL_RTC_PEER_CONNECTIONS = 64;
var RTC_CONNECTION_QUEUE_TIMEOUT_MS = 45e3;
var RTC_HANDSHAKE_TIMEOUT_MS = 15e3;
var GLOBAL_RTC_CONNECTION_POOL_KEY = /* @__PURE__ */ Symbol.for("ctox.rxdb.webrtc-rtc-pool.v1");
var RECENT_RTC_EVENT_LIMIT = 40;
var SIGNALING_RECONNECT_BASE_MS = 1e3;
var SIGNALING_RECONNECT_MAX_MS = 3e4;
var SHELL_CRITICAL_COLLECTIONS = /* @__PURE__ */ new Set([
  "ctox_runtime_settings",
  "business_module_catalog",
  "business_commands",
  "ctox_queue_tasks",
  "browser_sessions",
  "browser_tabs",
  "browser_frames",
  "browser_input_events"
]);
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
    expectedNativePeerId = "",
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
      expectedNativePeerId,
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
      sentInlineFrames: 0,
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
    this.recentConnectionEvents = [];
    this.recentMessages = [];
    this.connectionRequests = /* @__PURE__ */ new Map();
    this.forceInitiatorPeers = /* @__PURE__ */ new Set();
    this.closed = false;
    this.signalingReconnectTimer = null;
    this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
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
      this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
      this.events.emit("signaling-open", { url: redactUrl(url) });
    };
    socket.onmessage = (event) => this.handleSignalingMessage(event.data);
    socket.onerror = () => this.events.emit("error", this.lastControlPlaneError || { code: "ctox_signaling_socket_error" });
    socket.onclose = () => {
      this.events.emit("signaling-close", {});
      if (!this.closed) this.scheduleSignalingReconnect();
    };
    return this;
  }
  scheduleSignalingReconnect() {
    if (this.closed || this.signalingReconnectTimer) return;
    const delay = this.signalingReconnectDelayMs;
    this.signalingReconnectDelayMs = Math.min(delay * 2, SIGNALING_RECONNECT_MAX_MS);
    this.signalingReconnectTimer = setTimeout(() => {
      this.signalingReconnectTimer = null;
      if (this.closed) return;
      this.events.emit("signaling-reconnect", { delayMs: delay });
      this.connect();
    }, delay);
  }
  close() {
    this.closed = true;
    if (this.signalingReconnectTimer) {
      clearTimeout(this.signalingReconnectTimer);
      this.signalingReconnectTimer = null;
    }
    cancelRtcPeerConnectionRequestsForOwner(this, "peer-close");
    this.connectionRequests.clear();
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
      while (!this.closed && this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState === "open") {
        const item = nextQueuedSend(connection.sendQueue);
        if (!item) break;
        this.refreshSendQueueStatus(connection);
        this.recordTransportStatus({
          sentScheduledFrames: this.transportStats.sentScheduledFrames + 1,
          lastSendPriority: item.priority
        });
        if (item.inline) {
          await this.waitForSendBuffer(connection.channel);
          if (this.connections.get(connection.remotePeerId) !== connection || connection.channel?.readyState !== "open") {
            this.removeConnection(connection.remotePeerId, "send-queue-channel-closed");
            break;
          }
          try {
            connection.channel.send(item.text);
            this.recordSentInlineFrame(item.payload, connection.channel);
          } catch (error) {
            this.removeConnection(connection.remotePeerId, "send-queue-send-failed");
            throw error;
          }
          continue;
        }
        try {
          await this.sendFramed(connection, item.text);
        } catch (error) {
          const peerClosed = isPeerClosedError(error);
          if (this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState !== "open") {
            this.removeConnection(connection.remotePeerId, "frame-send-channel-closed");
          }
          this.events.emit("error", {
            code: peerClosed ? "ctox_webrtc_peer_closed" : "ctox_webrtc_frame_send_failed",
            peerId: connection.remotePeerId,
            priority: item.priority,
            reason: error?.reason || null,
            lifecycle: peerClosed,
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
      try {
        if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== "open") {
          throw createPeerClosedError(connection.remotePeerId, "frame-send-channel-closed");
        }
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
        for (let windowStart = 0; windowStart < totalFrames; windowStart += FRAME_ACK_WINDOW) {
          await this.drainHighPriorityInlineFrames(connection);
          const windowEnd = Math.min(windowStart + FRAME_ACK_WINDOW, totalFrames) - 1;
          const ack = this.awaitFrameAck(transferId, connection.remotePeerId, windowEnd);
          for (let seq = windowStart; seq <= windowEnd; seq += 1) {
            await this.waitForSendBuffer(channel);
            if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== "open") {
              throw createPeerClosedError(connection.remotePeerId, "frame-send-channel-closed");
            }
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
            await this.awaitFrameAckWithControlDrain(connection, ack);
          } catch (error) {
            const resumed = await this.requestFrameResume(connection, transferId, attempt, windowEnd);
            if (!resumed) throw error;
          }
        }
        this.recordTransportStatus({ activeTransfers: Math.max(0, this.transportStats.activeTransfers - 1) });
        return;
      } catch (error) {
        lastError = error;
        if (isPeerClosedError(error)) break;
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
  async awaitFrameAckWithControlDrain(connection, ackPromise) {
    let settled = false;
    const wrapped = Promise.resolve(ackPromise).then(
      (value) => {
        settled = true;
        return { ok: true, value };
      },
      (error) => {
        settled = true;
        return { ok: false, error };
      }
    );
    while (!settled && this.connections.get(connection.remotePeerId) === connection && connection.channel?.readyState === "open") {
      const result = await Promise.race([
        wrapped,
        delay(50).then(() => null)
      ]);
      if (result) {
        if (result.ok) return result.value;
        throw result.error;
      }
      await this.drainHighPriorityInlineFrames(connection);
    }
    const result = await wrapped;
    if (result.ok) return result.value;
    throw result.error;
  }
  async drainHighPriorityInlineFrames(connection) {
    const queue = connection.sendQueue;
    if (!queue) return;
    while (connection.channel?.readyState === "open") {
      const item = nextHighPriorityInlineSend(queue);
      if (!item) break;
      this.refreshSendQueueStatus(connection);
      await this.waitForSendBuffer(connection.channel);
      connection.channel.send(item.text);
      this.recordSentInlineFrame(item.payload, connection.channel);
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
      if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== "open") {
        resolve(false);
        return;
      }
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
  // Phase 3 multiplex: callers tag a `collection` so one DataChannel can carry
  // every collection. The frame's `collection` is the native demux routing
  // key; responses are still correlated by request `id`.
  request(remotePeerId, method, params = [], timeoutMs = 15e3, collection = null) {
    const id = `${this.options.clientId}|${Date.now()}|${this.requestCounter++}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        const error = new Error(`Timed out waiting for WebRTC response ${method}`);
        const peerId = String(remotePeerId || "");
        const connection = this.connections.get(peerId);
        if (connection) {
          this.recordConnectionEvent(connection, "request-timeout", { method });
          if (shouldRecycleConnectionAfterRequestTimeout(method)) {
            this.forceInitiatorPeers.add(peerId);
            this.removeConnection(peerId, `request-timeout-${method}`);
          }
        }
        reject(error);
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method, peerId: remotePeerId });
      const frame = { id, method, params };
      if (collection) frame.collection = collection;
      const sent = this.send(remotePeerId, frame);
      if (!sent) {
        this.pending.delete(id);
        clearTimeout(timer);
        this.scheduleReconnect(remotePeerId, `send-not-open-${method}`);
        reject(new Error(`WebRTC peer ${remotePeerId} is not open`));
      }
    });
  }
  scheduleReconnect(remotePeerId, reason = "peer-reconnect") {
    const peerId = String(remotePeerId || "");
    if (!peerId || this.closed || !this.shouldConnectToRemotePeer(peerId)) return;
    setTimeout(() => {
      if (this.closed || this.connections.has(peerId) || !this.shouldConnectToRemotePeer(peerId)) return;
      try {
        this.ensureConnection(peerId);
      } catch (reconnectError) {
        this.events.emit("error", normalizePeerSignalError(reconnectError, peerId));
      }
    }, 250 + Math.floor(Math.random() * 500));
    this.events.emit("peer-state", { peerId, state: "reconnect-scheduled", reason });
  }
  handleSignalingMessage(raw) {
    let message;
    try {
      message = JSON.parse(raw);
    } catch (error) {
      this.events.emit("error", { code: "ctox_signaling_invalid_json", message: error.message });
      return;
    }
    if (message.type === "init" || message.type === "joined" || message.type === "ctoxPresence") {
      const ownPeerId = message.yourPeerId || message.peerId || this.options.clientId;
      if (ownPeerId && ownPeerId !== this.options.clientId) {
        this.options.clientId = String(ownPeerId);
      }
      const descriptors = signalingPeerDescriptors(message);
      const previousMetadata = new Map(this.peerMetadata);
      for (const descriptor of descriptors) {
        if (descriptor.peerId) this.rememberPeerMetadata(descriptor.peerId, descriptor);
      }
      this.pruneStaleNativeCandidateConnections(descriptors);
      const expectedNativePeerId = String(this.options.expectedNativePeerId || "").trim();
      const hasExpectedDescriptor = Boolean(expectedNativePeerId) && descriptors.some((descriptor) => this.peerMatchesExpectedNativePeerId(descriptor.peerId, descriptor));
      for (const descriptor of descriptors) {
        const remotePeerId = descriptor.peerId;
        if (!remotePeerId) continue;
        if (hasExpectedDescriptor && !this.peerMatchesExpectedNativePeerId(remotePeerId, descriptor)) {
          this.removeConnection(remotePeerId, "signaling-non-target-native-peer");
          continue;
        }
        const previousDescriptor = previousMetadata.get(remotePeerId);
        const nativePeerRejoined = message.type === "joined" && remotePeerId !== this.options.clientId && this.connections.has(remotePeerId) && peerJoinedAtChanged(previousDescriptor, descriptor);
        if (nativePeerRejoined) {
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
    const slot = tryAcquireRtcPeerConnectionSlot(this, remotePeerId);
    if (!slot) {
      this.queueConnection(remotePeerId).catch((error) => {
        this.events.emit("error", normalizePeerSignalError(error, remotePeerId));
      });
      return void 0;
    }
    return this.createConnection(remotePeerId, slot);
  }
  queueConnection(remotePeerId) {
    if (this.closed || !this.shouldConnectToRemotePeer(remotePeerId)) {
      return Promise.resolve(void 0);
    }
    const existing = this.connections.get(remotePeerId);
    if (existing) return Promise.resolve(existing);
    const pending = this.connectionRequests.get(remotePeerId);
    if (pending) return pending;
    const request = acquireRtcPeerConnectionSlot(this, remotePeerId).then((slot) => {
      if (this.closed || !this.shouldConnectToRemotePeer(remotePeerId)) {
        releaseRtcPeerConnectionSlot(slot, "queued-peer-abandoned");
        return void 0;
      }
      const current = this.connections.get(remotePeerId);
      if (current) {
        releaseRtcPeerConnectionSlot(slot, "queued-peer-existing");
        return current;
      }
      return this.createConnection(remotePeerId, slot);
    }).finally(() => {
      this.connectionRequests.delete(remotePeerId);
    });
    this.connectionRequests.set(remotePeerId, request);
    return request;
  }
  createConnection(remotePeerId, rtcPoolSlot = null) {
    let peer;
    try {
      peer = new RTCPeerConnection({ iceServers: this.options.iceServers });
    } catch (error) {
      releaseRtcPeerConnectionSlot(rtcPoolSlot, "rtc-constructor-failed");
      throw error;
    }
    const connection = {
      peer,
      channel: null,
      remotePeerId,
      pendingCandidates: [],
      rtcPoolSlot,
      createdAtMs: Date.now(),
      lastStateChangeAtMs: Date.now(),
      lastError: null,
      signalStats: createPeerSignalStats(),
      localCandidateTypes: {},
      remoteCandidateTypes: {},
      handshakeTimer: null,
      forceInitiator: this.forceInitiatorPeers.has(remotePeerId)
    };
    this.connections.set(remotePeerId, connection);
    connection.handshakeTimer = setTimeout(() => {
      const current = this.connections.get(remotePeerId);
      if (this.closed || current !== connection) return;
      if (connection.channel?.readyState === "open") return;
      this.recordConnectionEvent(connection, "handshake-timeout", {
        ageMs: Date.now() - connection.createdAtMs,
        connectionState: peer.connectionState || "",
        iceConnectionState: peer.iceConnectionState || "",
        iceGatheringState: peer.iceGatheringState || "",
        signalingState: peer.signalingState || ""
      });
      this.events.emit("peer-state", { peerId: remotePeerId, state: "handshake-timeout" });
      this.forceInitiatorPeers.add(remotePeerId);
      this.removeConnection(remotePeerId, "rtc-handshake-timeout");
    }, RTC_HANDSHAKE_TIMEOUT_MS);
    this.recordConnectionEvent(connection, "created", { state: peer.connectionState || "new" });
    peer.onicecandidate = (event) => {
      if (event.candidate) {
        recordCandidateType(connection.localCandidateTypes, event.candidate?.candidate);
        connection.signalStats.candidateSent += 1;
        connection.signalStats.lastLocalCandidateType = candidateTypeFromLine(event.candidate?.candidate);
        connection.signalStats.lastSignalAtMs = Date.now();
        this.sendSignal(remotePeerId, { type: "candidate", candidate: event.candidate.toJSON() });
        return;
      }
      connection.signalStats.localCandidateComplete = true;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, "local-candidates-complete", { state: peer.connectionState || "" });
    };
    peer.oniceconnectionstatechange = () => {
      this.recordConnectionEvent(connection, "ice-connection-state", {
        state: peer.iceConnectionState || ""
      });
    };
    peer.onicegatheringstatechange = () => {
      this.recordConnectionEvent(connection, "ice-gathering-state", {
        state: peer.iceGatheringState || ""
      });
    };
    peer.onconnectionstatechange = () => {
      const state = peer.connectionState;
      this.recordConnectionEvent(connection, "connection-state", { state });
      this.events.emit("peer-state", { peerId: remotePeerId, state });
      if (["closed", "failed", "disconnected"].includes(state)) {
        this.removeConnection(remotePeerId, `peer-${state}`);
      }
    };
    peer.ondatachannel = (event) => this.attachChannel(connection, event.channel);
    if (this.shouldInitiate(remotePeerId, connection)) {
      this.attachChannel(connection, peer.createDataChannel("ctox-rxdb"));
      this.createOffer(remotePeerId, peer).catch((error) => {
        this.events.emit("error", normalizePeerSignalError(error, remotePeerId));
      });
    }
    return connection;
  }
  shouldInitiate(remotePeerId, connection = null) {
    if (connection?.forceInitiator) return true;
    const remoteRole = this.peerMetadata.get(String(remotePeerId || ""))?.role || "";
    if (this.options.role === "browser" && remoteRole === "ctox_instance") return true;
    if (this.options.role === "ctox_instance" && remoteRole === "browser") return false;
    return String(this.options.clientId) < String(remotePeerId);
  }
  async createOffer(remotePeerId, peer) {
    if (this.closed || peer.signalingState === "closed") return;
    const offer = await peer.createOffer();
    if (this.closed || peer.signalingState === "closed") return;
    await peer.setLocalDescription(offer);
    const connection = this.connections.get(remotePeerId);
    if (connection) {
      connection.signalStats.offerSent += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, "offer-sent", { signalingState: peer.signalingState });
    }
    this.sendSignal(remotePeerId, { type: offer.type, sdp: offer.sdp });
  }
  async handlePeerSignal(remotePeerId, signal) {
    const connection = this.ensureConnection(remotePeerId);
    if (!connection) return;
    const peer = connection.peer;
    const data = typeof signal === "string" ? JSON.parse(signal) : signal;
    if (data.type === "candidate") {
      recordCandidateType(connection.remoteCandidateTypes, data.candidate?.candidate);
      connection.signalStats.candidateReceived += 1;
      connection.signalStats.lastRemoteCandidateType = candidateTypeFromLine(data.candidate?.candidate);
      connection.signalStats.lastSignalAtMs = Date.now();
      await this.addIceCandidateWhenReady(connection, data.candidate);
      return;
    }
    if (data.type === "offer") {
      connection.signalStats.offerReceived += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, "offer-received", { signalingState: peer.signalingState });
      if (this.shouldInitiate(remotePeerId, connection)) {
        this.recordConnectionEvent(connection, "offer-ignored-local-initiator", {
          signalingState: peer.signalingState
        });
        return;
      }
      if (peer.signalingState !== "stable") {
        await rollbackLocalDescription(peer);
      }
      await peer.setRemoteDescription(data);
      await this.flushPendingIceCandidates(connection);
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      connection.signalStats.answerSent += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, "answer-sent", { signalingState: peer.signalingState });
      this.sendSignal(remotePeerId, { type: answer.type, sdp: answer.sdp });
      return;
    }
    if (data.type === "answer") {
      connection.signalStats.answerReceived += 1;
      connection.signalStats.lastSignalAtMs = Date.now();
      this.recordConnectionEvent(connection, "answer-received", { signalingState: peer.signalingState });
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
      this.recordConnectionEvent(connection, "candidate-queued", { pendingCandidates: connection.pendingCandidates.length });
      return;
    }
    try {
      await peer.addIceCandidate(candidate);
      this.recordConnectionEvent(connection, "candidate-added", { pendingCandidates: connection.pendingCandidates.length });
    } catch (error) {
      if (!peer.remoteDescription && isMissingRemoteDescriptionIceError(error)) {
        connection.pendingCandidates.push(candidate);
        this.recordConnectionEvent(connection, "candidate-queued", { pendingCandidates: connection.pendingCandidates.length });
        return;
      }
      connection.lastError = normalizePeerSignalError(error, connection.remotePeerId);
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
      if (connection.handshakeTimer) {
        clearTimeout(connection.handshakeTimer);
        connection.handshakeTimer = null;
      }
      markCriticalRtcPeerConnectionOpened(connection.rtcPoolSlot);
      this.forceInitiatorPeers.delete(connection.remotePeerId);
      drainRtcPeerConnectionQueue("critical-peer-opened");
      this.recordConnectionEvent(connection, "datachannel-open", { readyState: channel.readyState || "open" });
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
    channel.onerror = () => {
      connection.lastError = { code: "ctox_data_channel_error", peerId: connection.remotePeerId };
      this.recordConnectionEvent(connection, "datachannel-error", { readyState: channel.readyState || "" });
      this.events.emit("error", connection.lastError);
    };
    channel.onclose = () => {
      this.recordConnectionEvent(connection, "datachannel-close", { readyState: channel.readyState || "closed" });
      this.removeConnection(connection.remotePeerId, "channel-close");
    };
  }
  async handleDataChannelFrame(peerId, payload) {
    if (this.closed) return;
    if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
      await this.handleTransportFrame(peerId, payload);
      return;
    }
    this.recordMessageMeta(peerId, payload);
    this.events.emit("message", { peerId, payload });
    const masterChangeCollection = masterChangeStreamCollection(payload);
    if (masterChangeCollection !== null) {
      this.events.emit("master-change", {
        peerId,
        result: payload.result,
        collection: masterChangeCollection || payload.collection || null
      });
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
        const result = await this.handleRequest(
          peerId,
          payload.method,
          payload.params || [],
          payload.collection || null
        );
        const response = { id: payload.id, result, error: null };
        if (payload.collection) response.collection = payload.collection;
        this.send(peerId, response);
      } catch (error) {
        const normalized = serializeFrameError(error, payload.method);
        this.events.emit("error", normalized);
        const response = { id: payload.id, result: null, error: normalized };
        if (payload.collection) response.collection = payload.collection;
        this.send(peerId, response);
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
  async handleRequest(peerId, method, params, collection = null) {
    this.recordObservedRequest(peerId, method);
    if (method === "token") {
      return this.options.storageToken;
    }
    if (method === "ctoxProtocol") {
      return this.protocolPayload(peerId, params, collection);
    }
    const handler = this.options.requestHandlers?.[method];
    if (typeof handler === "function") {
      return handler({ peerId, params, collection, peer: this });
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
  async protocolPayload(peerId, params = [], collection = null) {
    if (typeof this.options.protocolPayload === "function") {
      return this.options.protocolPayload({ peerId, params, collection, peer: this });
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
    this.connectionRequests.delete(peerId);
    if (connection.handshakeTimer) {
      clearTimeout(connection.handshakeTimer);
      connection.handshakeTimer = null;
    }
    try {
      connection.channel?.close?.();
    } catch {
    }
    try {
      connection.peer?.close?.();
    } catch {
    }
    releaseRtcPeerConnectionSlot(connection.rtcPoolSlot, reason);
    this.rejectPendingForPeer(peerId, createPeerClosedError(peerId, reason));
    this.events.emit("peer-close", { peerId, reason });
    if (reason !== "peer-close") {
      this.scheduleReconnect(peerId, reason);
    }
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
    if (this.peerMatchesExpectedNativePeerId(peerId, metadata)) return true;
    if (this.nativeCandidateConnectionCount(peerId) > 0) return false;
    return this.isNativePeerCandidate(peerId, metadata);
  }
  isNativePeerCandidate(peerId, metadata = {}) {
    return this.peerMatchesExpectedNativePeerId(peerId, metadata) || peerId.startsWith("ctox-business-os-native") || peerId.startsWith("ctox-core-") || metadata?.role === "ctox_instance";
  }
  pruneStaleNativeCandidateConnections(descriptors = []) {
    const liveNativePeerIds = new Set(descriptors.filter((descriptor) => descriptor?.peerId && this.isNativePeerCandidate(descriptor.peerId, descriptor)).map((descriptor) => descriptor.peerId));
    if (!liveNativePeerIds.size) return;
    for (const peerId of [...this.connections.keys()]) {
      if (liveNativePeerIds.has(peerId)) continue;
      const metadata = this.peerMetadata.get(peerId);
      if (!this.isNativePeerCandidate(peerId, metadata)) continue;
      this.removeConnection(peerId, "peer-close");
    }
  }
  peerMatchesExpectedNativePeerId(peerId, metadata = {}) {
    const expectedNativePeerId = String(this.options.expectedNativePeerId || "").trim();
    if (!expectedNativePeerId) return false;
    const candidates = [
      peerId,
      metadata?.peerId,
      metadata?.nativePeerId,
      metadata?.native_peer_id,
      metadata?.corePeerId,
      metadata?.core_peer_id,
      metadata?.clientId,
      metadata?.client_id,
      metadata?.client
    ];
    return candidates.some((candidate) => String(candidate || "").trim() === expectedNativePeerId);
  }
  nativeCandidateConnectionCount(excludePeerId = "") {
    let count = 0;
    for (const peerId of this.connections.keys()) {
      if (peerId === excludePeerId) continue;
      const metadata = this.peerMetadata.get(peerId);
      if (this.isNativePeerCandidate(peerId, metadata)) {
        count += 1;
      }
    }
    return count;
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
      pendingRequests: this.pending.size,
      pendingRequestMethods: [...this.pending.values()].map((pending) => pending.method || "").filter(Boolean).slice(-20),
      observedRequestMethods: [...this.observedRequests.keys()].map((key) => String(key).split("|").slice(1).join("|")).slice(-20),
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size,
      rtcConnectionPool: rtcPeerConnectionPoolSnapshot(),
      rtcConnections: [...this.connections.values()].map((connection) => peerConnectionSnapshot(connection)),
      recentRtcEvents: this.recentConnectionEvents.slice(-RECENT_RTC_EVENT_LIMIT),
      connectionCount: this.connections.size,
      connectionStates: [...this.connections.values()].map((connection) => ({
        peerId: connection.remotePeerId,
        peerConnectionState: connection.peer?.connectionState || "",
        iceConnectionState: connection.peer?.iceConnectionState || "",
        iceGatheringState: connection.peer?.iceGatheringState || "",
        signalingState: connection.peer?.signalingState || "",
        channelState: connection.channel?.readyState || "",
        channelLabel: connection.channel?.label || "",
        pendingCandidates: Array.isArray(connection.pendingCandidates) ? connection.pendingCandidates.length : 0
      })),
      recentMessages: this.recentMessages.slice(-30)
    };
  }
  recordConnectionEvent(connection, event, detail = {}) {
    if (!connection) return;
    connection.lastStateChangeAtMs = Date.now();
    const entry = {
      atMs: connection.lastStateChangeAtMs,
      event,
      peerId: connection.remotePeerId,
      collection: collectionNameFromTopic(this.options.room),
      ...detail
    };
    this.recentConnectionEvents.push(entry);
    if (this.recentConnectionEvents.length > RECENT_RTC_EVENT_LIMIT) {
      this.recentConnectionEvents.splice(0, this.recentConnectionEvents.length - RECENT_RTC_EVENT_LIMIT);
    }
    this.events.emit("transport-status", this.getTransportStatus());
  }
  recordSentTransportFrame(payload, channel) {
    this.recordTransportStatus({
      sentFrames: this.transportStats.sentFrames + 1,
      sentBytes: this.transportStats.sentBytes + encodedSize(JSON.stringify(payload)),
      lastBufferedAmount: Number(channel?.bufferedAmount || 0)
    });
  }
  recordSentInlineFrame(payload, channel) {
    this.recordTransportStatus({
      sentInlineFrames: this.transportStats.sentInlineFrames + 1,
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
  recordMessageMeta(peerId, payload) {
    if (!payload || typeof payload !== "object") return;
    this.recentMessages.push({
      atMs: Date.now(),
      peerId: String(peerId || ""),
      id: typeof payload.id === "string" ? payload.id.slice(0, 120) : "",
      method: typeof payload.method === "string" ? payload.method.slice(0, 80) : "",
      collection: typeof payload.collection === "string" ? payload.collection.slice(0, 120) : "",
      hasResult: Object.prototype.hasOwnProperty.call(payload, "result"),
      hasError: Object.prototype.hasOwnProperty.call(payload, "error")
    });
    if (this.recentMessages.length > 60) {
      this.recentMessages.splice(0, this.recentMessages.length - 60);
    }
    this.events.emit("transport-status", this.getTransportStatus());
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
function isPeerClosedError(error) {
  if (!error) return false;
  if (error.lifecycle === true && error.code === "ERR_CONNECTION_FAILURE") return true;
  const reason = String(error.reason || "");
  const message = String(error.message || error || "");
  return error.code === "ERR_CONNECTION_FAILURE" || reason.includes("peer-close") || reason.includes("channel-close") || reason.includes("channel-closed") || message.includes(" closed: ") || message.includes("channel-close") || message.includes("channel-closed");
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
function tryAcquireRtcPeerConnectionSlot(owner, remotePeerId) {
  const pool = getRtcPeerConnectionPool();
  noteCriticalRequested(pool, owner);
  const key = rtcPeerConnectionOwnerKey(owner, remotePeerId);
  const existing = pool.active.get(key);
  if (existing) return existing;
  const priority = rtcPeerConnectionPriority(owner);
  if (priority > 0 && isBrowserRuntime() && isBusinessOsRoom(owner?.options?.room) && !criticalRtcPeerConnectionsReady(pool)) {
    return null;
  }
  if (priority === 0) preemptOptionalRtcPeerConnectionSlot(pool);
  if (pool.active.size >= pool.maxActive) return null;
  const slot = createRtcPeerConnectionSlot(owner, remotePeerId, key);
  pool.active.set(key, slot);
  return slot;
}
function acquireRtcPeerConnectionSlot(owner, remotePeerId) {
  const immediate = tryAcquireRtcPeerConnectionSlot(owner, remotePeerId);
  if (immediate) return Promise.resolve(immediate);
  const pool = getRtcPeerConnectionPool();
  const key = rtcPeerConnectionOwnerKey(owner, remotePeerId);
  const existingQueued = pool.queue.find((entry2) => entry2.key === key);
  if (existingQueued) {
    scheduleRtcPeerConnectionQueueDrain("existing-slot-request");
    return existingQueued.promise;
  }
  noteCriticalRequested(pool, owner);
  let resolve;
  let reject;
  const promise = new Promise((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  const entry = {
    key,
    owner,
    remotePeerId,
    priority: rtcPeerConnectionPriority(owner),
    enqueuedAt: Date.now(),
    resolve,
    reject,
    promise,
    timer: null
  };
  entry.timer = setTimeout(() => {
    removeQueuedRtcPeerConnection(entry);
    reject(new Error(`Timed out waiting for browser WebRTC connection budget for ${remotePeerId}`));
  }, RTC_CONNECTION_QUEUE_TIMEOUT_MS);
  pool.queue.push(entry);
  sortRtcPeerConnectionQueue(pool);
  owner?.events?.emit?.("peer-state", { peerId: remotePeerId, state: "queued" });
  scheduleRtcPeerConnectionQueueDrain("slot-request-queued");
  return promise;
}
function releaseRtcPeerConnectionSlot(slot, reason = "closed") {
  if (!slot?.key) return;
  const pool = getRtcPeerConnectionPool();
  pool.active.delete(slot.key);
  drainRtcPeerConnectionQueue(reason);
}
function drainRtcPeerConnectionQueue(reason = "slot-released") {
  const pool = getRtcPeerConnectionPool();
  sortRtcPeerConnectionQueue(pool);
  while (pool.active.size < pool.maxActive && pool.queue.length) {
    const entryIndex = nextGrantableRtcPeerConnectionQueueIndex(pool);
    if (entryIndex < 0) break;
    const [entry] = pool.queue.splice(entryIndex, 1);
    if (entry.timer) clearTimeout(entry.timer);
    if (entry.owner?.closed) continue;
    if (pool.active.has(entry.key)) {
      entry.resolve(pool.active.get(entry.key));
      continue;
    }
    const slot = createRtcPeerConnectionSlot(entry.owner, entry.remotePeerId, entry.key);
    pool.active.set(entry.key, slot);
    entry.owner?.events?.emit?.("peer-state", { peerId: entry.remotePeerId, state: "slot-granted", reason });
    entry.resolve(slot);
  }
}
function scheduleRtcPeerConnectionQueueDrain(reason = "slot-drain-scheduled") {
  const pool = getRtcPeerConnectionPool();
  if (pool.drainScheduled) return;
  pool.drainScheduled = true;
  const schedule = typeof queueMicrotask === "function" ? queueMicrotask : (callback) => Promise.resolve().then(callback);
  schedule(() => {
    pool.drainScheduled = false;
    drainRtcPeerConnectionQueue(reason);
  });
}
function removeQueuedRtcPeerConnection(entry) {
  const pool = getRtcPeerConnectionPool();
  const index = pool.queue.indexOf(entry);
  if (index >= 0) pool.queue.splice(index, 1);
  if (entry?.timer) clearTimeout(entry.timer);
}
function cancelRtcPeerConnectionRequestsForOwner(owner, reason = "owner-closed") {
  const pool = getRtcPeerConnectionPool();
  const queued = pool.queue.filter((entry) => entry.owner === owner);
  for (const entry of queued) {
    removeQueuedRtcPeerConnection(entry);
    entry.reject(new Error(`Cancelled browser WebRTC connection budget request: ${reason}`));
  }
}
function sortRtcPeerConnectionQueue(pool) {
  pool.queue.sort((left, right) => {
    if (left.priority !== right.priority) return left.priority - right.priority;
    return left.enqueuedAt - right.enqueuedAt;
  });
}
function createRtcPeerConnectionSlot(owner, remotePeerId, key = rtcPeerConnectionOwnerKey(owner, remotePeerId)) {
  return {
    key,
    owner,
    remotePeerId: String(remotePeerId || ""),
    room: String(owner?.options?.room || ""),
    priority: rtcPeerConnectionPriority(owner),
    acquiredAtMs: Date.now()
  };
}
function getRtcPeerConnectionPool() {
  const root = globalThis || {};
  if (!root[GLOBAL_RTC_CONNECTION_POOL_KEY]) {
    root[GLOBAL_RTC_CONNECTION_POOL_KEY] = {
      maxActive: MAX_GLOBAL_RTC_PEER_CONNECTIONS,
      active: /* @__PURE__ */ new Map(),
      queue: [],
      criticalOpened: /* @__PURE__ */ new Set(),
      criticalRequested: /* @__PURE__ */ new Set(),
      drainScheduled: false
    };
  } else if (root[GLOBAL_RTC_CONNECTION_POOL_KEY].maxActive < MAX_GLOBAL_RTC_PEER_CONNECTIONS) {
    root[GLOBAL_RTC_CONNECTION_POOL_KEY].maxActive = MAX_GLOBAL_RTC_PEER_CONNECTIONS;
  }
  return root[GLOBAL_RTC_CONNECTION_POOL_KEY];
}
function rtcPeerConnectionPoolSnapshot() {
  const pool = getRtcPeerConnectionPool();
  return {
    maxActive: pool.maxActive,
    active: pool.active.size,
    queued: pool.queue.length,
    activeCritical: activeCriticalRtcPeerConnectionCount(pool),
    queuedCritical: queuedCriticalRtcPeerConnectionNames(pool).length,
    criticalOpened: [...pool.criticalOpened].sort(),
    criticalReady: criticalRtcPeerConnectionsReady(pool),
    activeConnections: [...pool.active.values()].map((slot) => rtcPeerConnectionSlotSnapshot(slot)),
    queuedConnections: pool.queue.map((entry) => ({
      collection: collectionNameFromTopic(entry.owner?.options?.room || ""),
      priority: entry.priority,
      queuedForMs: Date.now() - entry.enqueuedAt
    }))
  };
}
function rtcPeerConnectionOwnerKey(owner, remotePeerId) {
  return `${String(owner?.options?.room || "")}|${String(owner?.options?.clientId || "")}|${String(remotePeerId || "")}`;
}
function rtcPeerConnectionPriority(owner) {
  void owner;
  return 0;
}
function noteCriticalRequested(pool, owner) {
  if (!pool || !owner) return;
  const room = owner?.options?.room || "";
  if (!isBusinessOsRoom(room)) return;
  const collection = collectionNameFromTopic(room);
  if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) return;
  if (!pool.criticalRequested) pool.criticalRequested = /* @__PURE__ */ new Set();
  pool.criticalRequested.add(collection);
}
function criticalRtcPeerConnectionsReady(pool) {
  const requested = pool?.criticalRequested;
  if (!requested || requested.size === 0) return true;
  for (const collection of requested) {
    if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) continue;
    if (!pool.criticalOpened?.has(collection)) return false;
  }
  return true;
}
function queuedCriticalRtcPeerConnectionNames(pool) {
  const queuedCriticalRooms = /* @__PURE__ */ new Set();
  for (const entry of pool.queue) {
    const collection = collectionNameFromTopic(entry?.owner?.options?.room || "");
    if (SHELL_CRITICAL_COLLECTIONS.has(collection)) queuedCriticalRooms.add(collection);
  }
  return [...queuedCriticalRooms].sort();
}
function activeCriticalRtcPeerConnectionCount(pool) {
  let count = 0;
  for (const slot of pool.active.values()) {
    if (SHELL_CRITICAL_COLLECTIONS.has(collectionNameFromTopic(slot.room))) count += 1;
  }
  return count;
}
function preemptOptionalRtcPeerConnectionSlot(pool) {
  if (pool.active.size < pool.maxActive) return false;
  for (const slot of pool.active.values()) {
    const collection = collectionNameFromTopic(slot.room);
    if (SHELL_CRITICAL_COLLECTIONS.has(collection)) continue;
    try {
      slot.owner?.removeConnection?.(slot.remotePeerId, "rtc-preempted-for-shell-critical");
    } catch {
    }
    return true;
  }
  return false;
}
function nextGrantableRtcPeerConnectionQueueIndex(pool) {
  for (let index = 0; index < pool.queue.length; index += 1) {
    const entry = pool.queue[index];
    if (!entry) continue;
    if (entry.priority === 0 || !isBrowserRuntime() || !isBusinessOsRoom(entry.owner?.options?.room)) {
      return index;
    }
    if (criticalRtcPeerConnectionsReady(pool)) {
      return index;
    }
  }
  return -1;
}
function markCriticalRtcPeerConnectionOpened(slot) {
  if (!slot || slot.priority !== 0 || !isBusinessOsRoom(slot.room)) return;
  const collection = collectionNameFromTopic(slot.room);
  if (!SHELL_CRITICAL_COLLECTIONS.has(collection)) return;
  getRtcPeerConnectionPool().criticalOpened.add(collection);
}
function rtcPeerConnectionSlotSnapshot(slot) {
  return {
    collection: collectionNameFromTopic(slot.room),
    priority: slot.priority,
    activeForMs: Date.now() - slot.acquiredAtMs
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
    joinedAt: entry.joinedAt ?? null,
    capabilities
  };
}
function peerJoinedAtChanged(previous = {}, next = {}) {
  if (!previous || !next) return false;
  if (previous.joinedAt === null || previous.joinedAt === void 0) return false;
  if (next.joinedAt === null || next.joinedAt === void 0) return false;
  return String(previous.joinedAt) !== String(next.joinedAt);
}
function createPeerSignalStats() {
  return {
    offerSent: 0,
    offerReceived: 0,
    answerSent: 0,
    answerReceived: 0,
    candidateSent: 0,
    candidateReceived: 0,
    localCandidateComplete: false,
    lastLocalCandidateType: "",
    lastRemoteCandidateType: "",
    lastSignalAtMs: 0
  };
}
function peerConnectionSnapshot(connection) {
  const peer = connection?.peer;
  const channel = connection?.channel;
  return {
    peerId: connection?.remotePeerId || "",
    collection: collectionNameFromTopic(connection?.rtcPoolSlot?.room || ""),
    createdAtMs: connection?.createdAtMs || 0,
    ageMs: connection?.createdAtMs ? Date.now() - connection.createdAtMs : 0,
    signalingState: peer?.signalingState || "",
    iceConnectionState: peer?.iceConnectionState || "",
    iceGatheringState: peer?.iceGatheringState || "",
    connectionState: peer?.connectionState || "",
    channelReadyState: channel?.readyState || "",
    pendingCandidates: Array.isArray(connection?.pendingCandidates) ? connection.pendingCandidates.length : 0,
    hasLocalDescription: Boolean(peer?.localDescription),
    hasRemoteDescription: Boolean(peer?.remoteDescription),
    localCandidateTypes: { ...connection?.localCandidateTypes || {} },
    remoteCandidateTypes: { ...connection?.remoteCandidateTypes || {} },
    signal: { ...connection?.signalStats || {} },
    lastError: connection?.lastError || null,
    lastStateChangeAtMs: connection?.lastStateChangeAtMs || 0
  };
}
function recordCandidateType(target, candidateLine) {
  const type = candidateTypeFromLine(candidateLine);
  if (!type) return;
  target[type] = Number(target[type] || 0) + 1;
}
function candidateTypeFromLine(candidateLine) {
  const match = String(candidateLine || "").match(/\styp\s+([a-z0-9-]+)/i);
  return match?.[1] ? match[1].toLowerCase() : "";
}
function isBusinessOsRoom(room) {
  return String(room || "").startsWith("ctox-business-os:");
}
function isBrowserRuntime() {
  return typeof window === "object" && typeof document === "object";
}
function collectionNameFromTopic(topic) {
  const parts = String(topic || "").split(":").filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}
var MASTER_CHANGE_STREAM_ID = "masterChangeStream$";
function masterChangeStreamCollection(payload) {
  const id = payload?.id;
  if (typeof id !== "string") return null;
  if (id === MASTER_CHANGE_STREAM_ID) return "";
  const prefix = `${MASTER_CHANGE_STREAM_ID}:`;
  if (id.startsWith(prefix)) return id.slice(prefix.length);
  return null;
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
function nextHighPriorityInlineSend(queue) {
  if (!queue?.high?.length) return null;
  const index = queue.high.findIndex((item) => item?.inline);
  if (index < 0) return null;
  return queue.high.splice(index, 1)[0] || null;
}
function shouldRecycleConnectionAfterRequestTimeout(method = "") {
  return ["ctoxProtocol", "token"].includes(String(method || ""));
}
function classifySendPriority(payload = {}, text = "") {
  if (payload?.ctoxFrame === CTOX_FRAME_PROTOCOL) {
    return ["ack", "resume", "start"].includes(payload.kind) ? "high" : "normal";
  }
  const method = String(payload?.method || "");
  if ([
    "ctoxProtocol",
    "token",
    "rxdb.activeCollections",
    "masterChangesSince",
    "rxdb.query.fetch",
    "rxdb.query.cancel",
    "rxdb.file.fetch",
    "rxdb.file.cancel"
  ].includes(method)) return "high";
  if (method === "masterWrite" && encodedSize(text) > MAX_INLINE_FRAME_BYTES) return "low";
  if (method === "masterWrite") return "high";
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

// src/apps/business-os/rxdb/src/chunk-decoder.mjs
async function decodeChunk(chunk) {
  if (!chunk || typeof chunk !== "object") {
    throw new TypeError("chunk must be an object");
  }
  if (!chunk.compressed) {
    return chunk.documents || [];
  }
  if (chunk.compressed !== "deflate") {
    throw new Error(`unsupported chunk compression: ${chunk.compressed}`);
  }
  if (typeof chunk.compressedBase64 !== "string") {
    throw new Error("compressed chunk missing compressedBase64");
  }
  const bytes = base64ToBytes(chunk.compressedBase64);
  const json = await deflateInflate(bytes);
  return JSON.parse(json);
}
function base64ToBytes(b64) {
  if (typeof Buffer !== "undefined" && typeof Buffer.from === "function") {
    const buf = Buffer.from(b64, "base64");
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  }
  const bin = globalThis.atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}
async function deflateInflate(bytes) {
  if (typeof globalThis.DecompressionStream === "function") {
    const stream = new Blob([bytes]).stream().pipeThrough(new globalThis.DecompressionStream("deflate-raw"));
    const buf = await new Response(stream).arrayBuffer();
    return new TextDecoder().decode(buf);
  }
  throw new Error('DecompressionStream("deflate-raw") is required for compressed CTOX DB chunks');
}

// src/apps/business-os/rxdb/src/demand-loading-transport.mjs
var ACK_RESPONSE = Object.freeze({ ack: true });
var SERVER_QUERY_STREAM_LIMIT = Math.max(1, Number(CTOX_QUERY_RPC.maxInFlightStreams) || 4);
var CLIENT_QUERY_STREAM_LIMIT = Math.max(1, Math.min(6, SERVER_QUERY_STREAM_LIMIT - 1 || 1));
var QUERY_STREAM_LIMIT_RETRY_MS = 160;
var QUERY_STREAM_LIMIT_RETRIES = 6;
var QUERY_PEER_RETRY_MS = 250;
var QUERY_PEER_RETRIES = 24;
var GLOBAL_QUERY_STREAM_STATE_KEY = /* @__PURE__ */ Symbol.for("ctox.rxdb.query-stream-state.v1");
var CANCELLED_QUERY_REQUEST_LIMIT = 256;
function createDemandLoadingTransport({ getPeerId } = {}) {
  if (typeof getPeerId !== "function") {
    throw new TypeError("createDemandLoadingTransport requires getPeerId");
  }
  const queryCollectors = /* @__PURE__ */ new Map();
  const fileCollectors = /* @__PURE__ */ new Map();
  const queryStreamState = getGlobalQueryStreamState();
  const cancelledQueryRequests = /* @__PURE__ */ new Map();
  function routeQueryChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = queryCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    if (chunk.complete) {
      queryCollectors.delete(chunk.requestId);
      slot.resolve(slot.chunks);
    }
  }
  function routeQueryError(err) {
    if (!err || !err.requestId) return;
    const slot = queryCollectors.get(err.requestId);
    if (!slot) return;
    queryCollectors.delete(err.requestId);
    const e = new Error(`${err.code || "QUERY_ERROR"}: ${err.message || ""}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }
  function routeFileChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = fileCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    if (chunk.complete) {
      fileCollectors.delete(chunk.requestId);
      slot.resolve(slot.chunks);
    }
  }
  function routeFileError(err) {
    if (!err || !err.requestId) return;
    const slot = fileCollectors.get(err.requestId);
    if (!slot) return;
    fileCollectors.delete(err.requestId);
    const e = new Error(`${err.code || "FILE_ERROR"}: ${err.message || ""}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }
  const requestHandlers = {
    "rxdb.query.chunk": async ({ params }) => {
      routeQueryChunk(params?.[0]);
      return ACK_RESPONSE;
    },
    "rxdb.query.error": async ({ params }) => {
      routeQueryError(params?.[0]);
      return ACK_RESPONSE;
    },
    "rxdb.file.chunk": async ({ params }) => {
      routeFileChunk(params?.[0]);
      return ACK_RESPONSE;
    },
    "rxdb.file.error": async ({ params }) => {
      routeFileError(params?.[0]);
      return ACK_RESPONSE;
    }
  };
  let peer = null;
  function attach(p) {
    peer = p;
  }
  async function requestQueryFetch(envelope) {
    return withQueryStreamSlot(envelope?.requestId, () => requestQueryFetchWithRetry(envelope));
  }
  function withQueryStreamSlot(requestId, fn) {
    return new Promise((resolve, reject) => {
      const run = () => {
        queryStreamState.active += 1;
        Promise.resolve().then(fn).then(resolve, reject).finally(() => {
          queryStreamState.active = Math.max(0, queryStreamState.active - 1);
          const next = queryStreamState.queue.shift();
          if (next) queueMicrotask(typeof next === "function" ? next : next.run);
        });
      };
      if (queryStreamState.active < CLIENT_QUERY_STREAM_LIMIT) run();
      else queryStreamState.queue.push({ requestId: String(requestId || ""), run, reject });
    });
  }
  async function requestQueryFetchWithRetry(envelope) {
    const baseRequestId = envelope?.requestId;
    let attempt = 0;
    for (; ; ) {
      const requestId = attempt === 0 ? baseRequestId : `${baseRequestId}|retry-${attempt}`;
      try {
        return await requestQueryFetchOnce({ ...envelope, requestId });
      } catch (error) {
        const peerUnavailable = isRetryableQueryPeerUnavailable(error);
        const retryLimit = peerUnavailable ? QUERY_PEER_RETRIES : QUERY_STREAM_LIMIT_RETRIES;
        if (!isRetryableQueryFetch(error) || attempt >= retryLimit) {
          throw error;
        }
        attempt += 1;
        await delay3((peerUnavailable ? QUERY_PEER_RETRY_MS : QUERY_STREAM_LIMIT_RETRY_MS) * attempt);
      }
    }
  }
  async function requestQueryFetchOnce(envelope) {
    const requestId = envelope?.requestId;
    const cancelReason = consumeQueryCancelReason(requestId);
    if (cancelReason) throw createQueryCancelError(cancelReason);
    if (!peer) throw new Error("demand transport has no peer attached");
    const peerId = resolvePeerId();
    if (!peerId) throw new Error("PEER_UNAVAILABLE");
    const promise = new Promise((resolve, reject) => {
      queryCollectors.set(requestId, { chunks: [], resolve, reject });
    });
    try {
      await peer.request(peerId, CTOX_QUERY_RPC.fetch, [envelope]);
    } catch (err) {
      queryCollectors.delete(requestId);
      throw err;
    }
    const chunks = await promise;
    const documents = [];
    let authoritativeRevision = null;
    for (const c of chunks) {
      const decoded = await decodeChunk(c);
      for (const d of decoded) documents.push(d);
      if (c.authoritativeRevision) authoritativeRevision = c.authoritativeRevision;
    }
    return { documents, authoritativeRevision };
  }
  function isRetryableQueryStreamLimit(error) {
    const code = String(error?.code || "");
    const message = String(error?.message || "");
    return Boolean(error?.retryable) && (code === "STREAM_LIMIT_EXCEEDED" || message.includes("STREAM_LIMIT_EXCEEDED"));
  }
  function isRetryableQueryFetch(error) {
    return isRetryableQueryStreamLimit(error) || isRetryableQueryPeerUnavailable(error);
  }
  function isRetryableQueryPeerUnavailable(error) {
    const message = String(error?.message || "");
    return message === "PEER_UNAVAILABLE" || /WebRTC peer .* is not open/.test(message);
  }
  function delay3(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
  async function requestQueryCancel({ requestId, reason = "client-abort" }) {
    if (!requestId) return;
    const matchingRequestIds = matchingQueryRequestIds(requestId);
    const queuedRequestIds = rejectQueuedQueryRequests(requestId, reason);
    if (!matchingRequestIds.length && !queuedRequestIds.length) {
      markQueryCancelled(requestId, reason);
    }
    const error = createQueryCancelError(reason);
    for (const activeRequestId of matchingRequestIds) {
      rejectQueryCollector(activeRequestId, error);
    }
    const cancelRequestIds = matchingRequestIds.length ? matchingRequestIds : queuedRequestIds.length ? [] : [requestId];
    const peerId = peer ? resolvePeerId() : "";
    if (peer && peerId) {
      for (const activeRequestId of cancelRequestIds) {
        try {
          await peer.request(peerId, CTOX_QUERY_RPC.cancel, [{ requestId: activeRequestId, reason }], 2e3);
        } catch {
        }
      }
    }
  }
  async function requestFileFetch({ requestId, fileId, range, knownSequences, collectionName }) {
    if (!peer) throw new Error("demand transport has no peer attached");
    const peerId = resolvePeerId();
    if (!peerId) throw new Error("PEER_UNAVAILABLE");
    const promise = new Promise((resolve, reject) => {
      fileCollectors.set(requestId, { chunks: [], resolve, reject });
    });
    try {
      await peer.request(peerId, "rxdb.file.fetch", [{
        requestId,
        collectionName,
        fileId,
        range: range ?? null,
        knownSequences: knownSequences ?? []
      }]);
    } catch (err) {
      fileCollectors.delete(requestId);
      throw err;
    }
    const chunks = await promise;
    return chunks.map((c) => ({ sequence: c.sequence, bytesBase64: c.bytesBase64, hash: c.hash }));
  }
  function pendingQueryCount() {
    return queryCollectors.size + queryStreamState.queue.length;
  }
  function pendingFileCount() {
    return fileCollectors.size;
  }
  function matchingQueryRequestIds(requestId) {
    const raw = String(requestId || "");
    if (!raw) return [];
    const ids = [];
    if (queryCollectors.has(raw)) ids.push(raw);
    const prefix = `${raw}|`;
    for (const id of queryCollectors.keys()) {
      if (id !== raw && id.startsWith(prefix)) ids.push(id);
    }
    return ids;
  }
  function rejectQueryCollector(requestId, error) {
    const slot = queryCollectors.get(requestId);
    if (!slot) return false;
    queryCollectors.delete(requestId);
    slot.reject(error);
    return true;
  }
  function rejectQueuedQueryRequests(requestId, reason) {
    const raw = String(requestId || "");
    if (!raw) return [];
    const prefix = `${raw}|`;
    const remaining = [];
    const rejectedIds = [];
    const error = createQueryCancelError(reason);
    for (const entry of queryStreamState.queue) {
      const queuedRequestId = queuedQueryRequestId(entry);
      if (queuedRequestId && (queuedRequestId === raw || queuedRequestId.startsWith(prefix))) {
        rejectedIds.push(queuedRequestId);
        entry.reject(error);
      } else {
        remaining.push(entry);
      }
    }
    if (rejectedIds.length) {
      queryStreamState.queue.splice(0, queryStreamState.queue.length, ...remaining);
    }
    return rejectedIds;
  }
  function queuedQueryRequestId(entry) {
    if (!entry || typeof entry === "function") return "";
    return String(entry.requestId || "");
  }
  function markQueryCancelled(requestId, reason) {
    const raw = String(requestId || "");
    if (!raw) return;
    cancelledQueryRequests.set(raw, reason || "client-abort");
    while (cancelledQueryRequests.size > CANCELLED_QUERY_REQUEST_LIMIT) {
      const oldest = cancelledQueryRequests.keys().next().value;
      cancelledQueryRequests.delete(oldest);
    }
  }
  function consumeQueryCancelReason(requestId) {
    const raw = String(requestId || "");
    if (!raw) return "";
    if (cancelledQueryRequests.has(raw)) {
      const reason = cancelledQueryRequests.get(raw);
      cancelledQueryRequests.delete(raw);
      return reason;
    }
    for (const [cancelledRequestId, reason] of cancelledQueryRequests) {
      if (raw.startsWith(`${cancelledRequestId}|`)) {
        cancelledQueryRequests.delete(cancelledRequestId);
        return reason;
      }
    }
    return "";
  }
  function createQueryCancelError(reason) {
    const error = new Error(`QUERY_CANCELLED: ${reason || "client-abort"}`);
    error.code = "QUERY_CANCELLED";
    error.retryable = false;
    return error;
  }
  function resolvePeerId() {
    const configured = getPeerId();
    if (configured) return configured;
    return firstOpenPeerId();
  }
  function firstOpenPeerId() {
    const entries = peer?.connections?.entries?.();
    if (!entries) return "";
    for (const [peerId, connection] of entries) {
      const channelState = connection?.channel?.readyState || connection?.channelReadyState || "";
      const peerState = connection?.peer?.connectionState || connection?.peerConnectionState || connection?.connectionState || "";
      if (channelState === "open" && !["closed", "failed", "disconnected"].includes(String(peerState))) {
        return peerId;
      }
    }
    return "";
  }
  return {
    requestHandlers,
    attach,
    requestQueryFetch,
    requestQueryCancel,
    requestFileFetch,
    pendingQueryCount,
    pendingFileCount
  };
}
function getGlobalQueryStreamState() {
  if (!globalThis[GLOBAL_QUERY_STREAM_STATE_KEY]) {
    globalThis[GLOBAL_QUERY_STREAM_STATE_KEY] = { active: 0, queue: [] };
  }
  return globalThis[GLOBAL_QUERY_STREAM_STATE_KEY];
}

// src/apps/business-os/rxdb/src/query-fingerprint.mjs
var PROTOCOL_VERSION = "1.5";
function canonicalizeQueryInput(input) {
  if (!input || typeof input !== "object") {
    throw new TypeError("query input must be an object");
  }
  const collection = String(input.collection || "");
  if (!collection) throw new Error("collection is required");
  const schemaVersion = Number.isFinite(Number(input.schemaVersion)) ? Number(input.schemaVersion) : 0;
  return {
    collection,
    schemaVersion,
    protocolVersion: PROTOCOL_VERSION,
    selector: canonicalizeSelector(input.selector),
    sort: canonicalizeSort(input.sort),
    limit: normalizeOptionalNumber(input.limit),
    skip: normalizeOptionalNumber(input.skip),
    window: canonicalizeWindow(input.window)
  };
}
function canonicalQueryJson(input) {
  return canonicalJson(canonicalizeQueryInput(input));
}
async function queryFingerprint(input) {
  return sha256Hex(canonicalQueryJson(input));
}
function canonicalizeSelector(selector) {
  if (selector === void 0 || selector === null) return {};
  if (typeof selector !== "object" || Array.isArray(selector)) {
    throw new TypeError("selector must be a plain object");
  }
  return canonicalizeSelectorValue(selector);
}
function canonicalizeSelectorValue(value) {
  if (value === null) return null;
  if (Array.isArray(value)) {
    return value.map(canonicalizeSelectorValue);
  }
  if (typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      const v = canonicalizeSelectorValue(value[key]);
      if (key === "$in" || key === "$nin") {
        out[key] = sortAndDedupeArray(v);
      } else {
        out[key] = v;
      }
    }
    return out;
  }
  return value;
}
function sortAndDedupeArray(value) {
  if (!Array.isArray(value)) return value;
  const seen = /* @__PURE__ */ new Set();
  const out = [];
  for (const item of value) {
    const key = canonicalJson(item);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  out.sort((a, b) => {
    const sa = canonicalJson(a);
    const sb = canonicalJson(b);
    return sa < sb ? -1 : sa > sb ? 1 : 0;
  });
  return out;
}
function canonicalizeSort(sort) {
  if (sort === void 0 || sort === null) return [];
  if (!Array.isArray(sort)) {
    throw new TypeError("sort must be an array of single-key direction objects");
  }
  return sort.map((entry) => {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      throw new TypeError("sort entries must be single-key objects");
    }
    const keys = Object.keys(entry);
    if (keys.length !== 1) {
      throw new TypeError("sort entries must have exactly one key");
    }
    const key = keys[0];
    const direction = normalizeSortDirection(entry[key]);
    return { [key]: direction };
  });
}
function normalizeSortDirection(direction) {
  const raw = typeof direction === "string" ? direction.toLowerCase() : direction;
  if (raw === "desc" || raw === -1 || raw === "-1") return "desc";
  if (raw === "asc" || raw === 1 || raw === "1") return "asc";
  throw new TypeError(`invalid sort direction: ${direction}`);
}
function normalizeOptionalNumber(value) {
  if (value === void 0 || value === null) return null;
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0) {
    throw new TypeError("optional number must be a non-negative finite value");
  }
  return Math.floor(n);
}
function canonicalizeWindow(window2) {
  if (window2 === void 0 || window2 === null) return null;
  if (typeof window2 !== "object") {
    throw new TypeError("window must be an object");
  }
  return {
    offset: normalizeOptionalNumber(window2.offset) ?? 0,
    limit: normalizeOptionalNumber(window2.limit) ?? 200
  };
}

// src/apps/business-os/rxdb/src/query-demand-loader.mjs
var DEFAULT_WINDOW_LIMIT = 200;
function createQueryDemandLoader({
  storageCollection,
  sidecar,
  collectionName,
  schemaVersion,
  requestQueryFetch,
  requestCancel = null,
  multiTabBroker = null,
  status = null,
  clock = Date.now
}) {
  if (!storageCollection) throw new TypeError("demand loader requires storageCollection");
  if (!sidecar) throw new TypeError("demand loader requires sidecar");
  if (!collectionName) throw new TypeError("demand loader requires collectionName");
  if (typeof requestQueryFetch !== "function") {
    throw new TypeError("demand loader requires requestQueryFetch");
  }
  const inflightByFingerprint = /* @__PURE__ */ new Map();
  return {
    async resolveQuery(query, { window: window2 } = {}) {
      const normalizedWindow = normalizeWindow(window2, query);
      const fingerprintInput = {
        collection: collectionName,
        schemaVersion: schemaVersion ?? 0,
        selector: query?.selector ?? {},
        sort: normalizeSort(query?.sort),
        limit: query?.limit,
        skip: query?.skip,
        window: normalizedWindow
      };
      const fingerprint = await queryFingerprint(fingerprintInput);
      const sidecarKey = [collectionName, fingerprint, normalizedWindow.offset, normalizedWindow.limit];
      const cached = await sidecar.getQueryWindow(sidecarKey);
      if (cached && cached.complete) {
        if (query?.requireRevision && cached.authoritativeRevision !== query.requireRevision) {
        } else {
          await touchSidecarAccess(sidecar, collectionName, cached.documentIds);
          return readLocalDocuments(storageCollection, query, normalizedWindow);
        }
      }
      const dedupKey = `${collectionName}|${fingerprint}|${normalizedWindow.offset}|${normalizedWindow.limit}`;
      if (inflightByFingerprint.has(dedupKey)) {
        bumpStatus(status, "queryFetchDedupHitCount");
        return inflightByFingerprint.get(dedupKey);
      }
      bumpStatus(status, "queryFetchInFlight", 1);
      v15Log("fetch:start", { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
      const job = (async () => {
        const startedAt = clock();
        try {
          const effectiveSkip = Number.isFinite(Number(query?.skip)) ? Math.max(0, Math.floor(Number(query.skip))) : normalizedWindow.offset;
          const effectiveLimit = Number.isFinite(Number(query?.limit)) ? Math.max(1, Math.floor(Number(query.limit))) : normalizedWindow.limit;
          const result = await requestQueryFetch({
            requestId: `${dedupKey}|${startedAt}`,
            databaseName: storageCollection?.databaseName ?? null,
            collectionName,
            schemaVersion: schemaVersion ?? 0,
            queryFingerprint: fingerprint,
            query: {
              selector: query?.selector ?? {},
              sort: normalizeSort(query?.sort),
              limit: effectiveLimit,
              skip: effectiveSkip
            },
            window: normalizedWindow
          });
          await materializeChunks(storageCollection, result.documents || []);
          const documentIds = (result.documents || []).map(extractId).filter(Boolean);
          await sidecar.upsertQueryWindow({
            collection: collectionName,
            queryFingerprint: fingerprint,
            offset: normalizedWindow.offset,
            limit: normalizedWindow.limit,
            documentIds,
            complete: true,
            authoritativeRevision: result.authoritativeRevision ?? null
          });
          await sidecar.touchDocuments(collectionName, documentIds, {
            estimatedBytes: estimateBytes(result.documents || [])
          });
          bumpStatus(status, "queryFetchSuccessCount");
          if (status) status.lastQueryFetchMs = clock() - startedAt;
          v15Log("fetch:ok", { fingerprint, docs: documentIds.length, ms: clock() - startedAt });
          return readLocalDocuments(storageCollection, query, normalizedWindow);
        } catch (error) {
          bumpStatus(status, "queryFetchErrorCount");
          if (!isIndexedDbConnectionClosingError(error)) {
            v15Log("fetch:error", { fingerprint, error: String(error?.message ?? error) });
          }
          throw error;
        } finally {
          bumpStatus(status, "queryFetchInFlight", -1);
          inflightByFingerprint.delete(dedupKey);
        }
      })();
      inflightByFingerprint.set(dedupKey, job);
      return job;
    },
    inflightSize() {
      return inflightByFingerprint.size;
    },
    // Wave 7: invalidation hook. When the replication layer reports that a
    // document in `collectionName` was changed remotely, call this with the
    // changed document ids — any cached query window that references those
    // ids is marked incomplete so the next exec triggers a remote refresh.
    async invalidateDocumentChange(changedDocumentIds = []) {
      if (!changedDocumentIds.length) return 0;
      const all = await sidecar.backend.scanQueryWindows();
      const ids = new Set(changedDocumentIds);
      let invalidated = 0;
      for (const window2 of all) {
        if (window2.collection !== collectionName) continue;
        if (window2.documentIds.some((id) => ids.has(id))) {
          await sidecar.invalidateQueryWindow([
            window2.collection,
            window2.queryFingerprint,
            window2.offset,
            window2.limit
          ]);
          invalidated += 1;
        }
      }
      return invalidated;
    },
    async invalidateCollectionChange() {
      const all = await sidecar.backend.scanQueryWindows();
      let invalidated = 0;
      for (const window2 of all) {
        if (window2.collection !== collectionName) continue;
        await sidecar.invalidateQueryWindow([
          window2.collection,
          window2.queryFingerprint,
          window2.offset,
          window2.limit
        ]);
        invalidated += 1;
      }
      return invalidated;
    },
    // Wave 7 + production hardening: reconnect-cancel. Aborts all in-flight
    // fetches and removes any partially-materialized documents from the
    // primary store so the next fetch starts from a clean slate (no orphans).
    async abortAllInFlight(reason = "reconnect") {
      const cancelled = [];
      for (const [dedupKey, job] of inflightByFingerprint.entries()) {
        const [, fingerprint] = dedupKey.split("|");
        cancelled.push({ dedupKey, fingerprint });
        if (typeof requestCancel === "function") {
          try {
            await requestCancel({ requestId: dedupKey, fingerprint, reason });
          } catch {
          }
        }
        try {
          job.catch?.(() => {
          });
        } catch {
        }
      }
      inflightByFingerprint.clear();
      try {
        const allWindows = await sidecar.backend.scanQueryWindows();
        for (const { fingerprint } of cancelled) {
          const partial = allWindows.filter(
            (w) => w.queryFingerprint === fingerprint && !w.complete
          );
          for (const window2 of partial) {
            const ids = window2.documentIds || [];
            if (ids.length && typeof storageCollection.bulkWrite === "function") {
              const tombstones = ids.map((id) => ({ id, _deleted: true }));
              try {
                await storageCollection.bulkWrite(tombstones);
              } catch {
              }
            }
            await sidecar.backend.deleteQueryWindow([
              window2.collection,
              window2.queryFingerprint,
              window2.offset,
              window2.limit
            ]);
          }
        }
      } catch {
      }
    },
    // Wave 7: multi-tab dedup. If a `multiTabBroker` is provided, it is
    // consulted before kicking off a remote fetch; followers wait for the
    // leader's materialization signal instead of fetching themselves.
    async leaderClaim(windowKey) {
      if (!multiTabBroker?.claim) return true;
      return multiTabBroker.claim(windowKey);
    },
    async leaderRelease(windowKey) {
      if (!multiTabBroker?.release) return;
      await multiTabBroker.release(windowKey);
    }
  };
}
function normalizeWindow(window2, query) {
  if (window2 && typeof window2 === "object") {
    return {
      offset: Math.max(0, Math.floor(Number(window2.offset) || 0)),
      limit: Math.max(1, Math.floor(Number(window2.limit) || DEFAULT_WINDOW_LIMIT))
    };
  }
  return {
    offset: Math.max(0, Math.floor(Number(query?.skip) || 0)),
    limit: Math.max(1, Math.floor(Number(query?.limit) || DEFAULT_WINDOW_LIMIT))
  };
}
function normalizeSort(sort) {
  if (!Array.isArray(sort)) return [];
  return sort.map((entry) => {
    if (!entry || typeof entry !== "object") return entry;
    const keys = Object.keys(entry);
    if (keys.length !== 1) return entry;
    const key = keys[0];
    const direction = entry[key];
    return { [key]: direction === -1 || direction === "desc" || direction === "DESC" ? "desc" : "asc" };
  });
}
async function readLocalDocuments(storageCollection, query, window2) {
  if (typeof storageCollection.queryDocuments === "function") {
    return storageCollection.queryDocuments(
      { ...query, skip: window2.offset, limit: window2.limit },
      {
        matchesSelector: defaultMatcher,
        sortDocuments: defaultSorter
      }
    );
  }
  const docs = await storageCollection.allDocuments();
  return applyQueryToDocs(docs, query, window2);
}
async function materializeChunks(storageCollection, documents) {
  if (!documents.length) return;
  await storageCollection.bulkWrite(documents);
}
async function touchSidecarAccess(sidecar, collectionName, documentIds) {
  if (!documentIds?.length) return;
  await sidecar.touchDocuments(collectionName, documentIds);
}
function extractId(doc) {
  if (!doc || typeof doc !== "object") return null;
  return doc.id || doc._id || null;
}
function estimateBytes(documents) {
  try {
    return JSON.stringify(documents).length;
  } catch {
    return documents.length * 256;
  }
}
function bumpStatus(status, field, delta = 1) {
  if (!status) return;
  if (typeof status[field] !== "number") status[field] = 0;
  status[field] += delta;
}
var v15LogSink = null;
function setV15LogSink(fn) {
  v15LogSink = typeof fn === "function" ? fn : null;
}
function v15Log(event, fields) {
  if (v15LogSink) {
    try {
      v15LogSink(event, fields);
    } catch {
    }
    return;
  }
  if (globalThis?.console?.debug) {
    globalThis.console.debug("[V1.5]", event, fields);
  }
}
function defaultMatcher(doc, selector = {}) {
  for (const [key, expected] of Object.entries(selector)) {
    if (key.startsWith("$")) return true;
    const actual = doc?.[key];
    if (expected && typeof expected === "object" && !Array.isArray(expected)) {
      if ("$eq" in expected && actual !== expected.$eq) return false;
      if ("$ne" in expected && actual === expected.$ne) return false;
      if ("$in" in expected && !expected.$in.includes(actual)) return false;
      if ("$gte" in expected && !(actual >= expected.$gte)) return false;
      if ("$lte" in expected && !(actual <= expected.$lte)) return false;
      continue;
    }
    if (actual !== expected) return false;
  }
  return true;
}
function defaultSorter(docs, sort = []) {
  if (!sort?.length) return docs;
  return docs.slice().sort((a, b) => {
    for (const entry of sort) {
      const [key, direction] = Object.entries(entry)[0] || [];
      const factor = direction === "desc" ? -1 : 1;
      const av = a?.[key];
      const bv = b?.[key];
      if (av < bv) return -1 * factor;
      if (av > bv) return 1 * factor;
    }
    return 0;
  });
}
function applyQueryToDocs(docs, query, window2) {
  let filtered = (docs || []).filter((doc) => defaultMatcher(doc, query?.selector || {}));
  filtered = defaultSorter(filtered, normalizeSort(query?.sort));
  if (window2.offset > 0) filtered = filtered.slice(window2.offset);
  if (Number.isFinite(window2.limit)) filtered = filtered.slice(0, window2.limit);
  return filtered;
}

// src/apps/business-os/rxdb/src/file-demand-loader.mjs
var FILE_CHUNK_PRESENCE_KEY = (collection, fileId) => `${collection}|${fileId}`;
function createFileDemandLoader({
  collectionName,
  storageCollection,
  sidecarBackend,
  requestFileFetch,
  status = null,
  clock = Date.now
}) {
  if (!collectionName) throw new TypeError("file loader requires collectionName");
  if (!storageCollection) throw new TypeError("file loader requires storageCollection");
  if (!sidecarBackend) throw new TypeError("file loader requires sidecarBackend");
  if (typeof requestFileFetch !== "function") {
    throw new TypeError("file loader requires requestFileFetch");
  }
  const inflight = /* @__PURE__ */ new Map();
  return {
    async fetchFile(fileId, { range = null } = {}) {
      if (inflight.has(fileId)) {
        bump(status, "fileStreamDedupHits");
        return inflight.get(fileId);
      }
      const job = (async () => {
        const startedAt = clock();
        bump(status, "activeFileStreams", 1);
        try {
          const presence = await getPresence(sidecarBackend, collectionName, fileId);
          const chunks = await requestFileFetch({
            requestId: `file-${fileId}-${startedAt}`,
            collectionName,
            fileId,
            range,
            knownSequences: presence?.presentSequences || []
          });
          if (!Array.isArray(chunks)) {
            throw new TypeError("requestFileFetch must return an array of chunks");
          }
          for (const chunk of chunks) {
            if (!chunk || typeof chunk !== "object") continue;
            await storageCollection.bulkWrite([
              {
                id: `${fileId}-${chunk.sequence}`,
                file_id: fileId,
                sequence: chunk.sequence,
                bytes_base64: chunk.bytesBase64,
                hash: chunk.hash || null
              }
            ]);
            bump(status, "fileBytesReceived", chunk.bytesBase64?.length || 0);
          }
          const sequences = chunks.map((c) => c.sequence).sort((a, b) => a - b);
          const expectedTotal = Math.max(
            ...sequences,
            presence?.expectedChunkCount || 0
          ) + 1;
          await sidecarBackend.putDocumentAccess({
            collection: collectionName,
            id: `${fileId}-presence`,
            lastAccessedAt: clock(),
            pinReason: "file-chunks",
            dirty: false,
            estimatedBytes: 0
          });
          await putPresence(sidecarBackend, collectionName, fileId, {
            collection: collectionName,
            fileId,
            expectedChunkCount: expectedTotal,
            presentSequences: dedupeSorted([
              ...presence?.presentSequences || [],
              ...sequences
            ]),
            lastVerifiedAt: clock()
          });
          if (status) status.lastFileFetchMs = clock() - startedAt;
          return chunks;
        } catch (error) {
          bump(status, "fileStreamErrors");
          throw error;
        } finally {
          bump(status, "activeFileStreams", -1);
          inflight.delete(fileId);
        }
      })();
      inflight.set(fileId, job);
      return job;
    },
    inflightSize() {
      return inflight.size;
    }
  };
}
async function getPresence(backend, collection, fileId) {
  const record = await backend.getDocumentAccess(collection, `${fileId}-presence`);
  if (!record || !record.fileChunkPresence) return null;
  return record.fileChunkPresence;
}
async function putPresence(backend, collection, fileId, presence) {
  await backend.putDocumentAccess({
    collection,
    id: `${fileId}-presence`,
    lastAccessedAt: presence.lastVerifiedAt,
    pinReason: "file-chunks",
    dirty: false,
    estimatedBytes: 0,
    fileChunkPresence: presence
  });
}
function bump(status, field, delta = 1) {
  if (!status) return;
  if (typeof status[field] !== "number") status[field] = 0;
  status[field] += delta;
}
function dedupeSorted(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  const out = [];
  for (const v of sorted) {
    if (out.length === 0 || out[out.length - 1] !== v) out.push(v);
  }
  return out;
}

// src/apps/business-os/rxdb/src/query-meta-backend-memory.mjs
function createMemoryMetaBackend() {
  const queryWindows = /* @__PURE__ */ new Map();
  const documentAccess = /* @__PURE__ */ new Map();
  const cacheStats = /* @__PURE__ */ new Map();
  return {
    name: "memory",
    async putQueryWindow(record) {
      const key = queryWindowKey(record);
      queryWindows.set(key, { ...record });
    },
    async getQueryWindow(key) {
      const entry = queryWindows.get(stringKey(key));
      return entry ? { ...entry } : null;
    },
    async deleteQueryWindow(key) {
      queryWindows.delete(stringKey(key));
    },
    async scanQueryWindows() {
      return Array.from(queryWindows.values(), (record) => ({ ...record }));
    },
    async putDocumentAccess(record) {
      documentAccess.set(documentAccessKey(record), { ...record });
    },
    async getDocumentAccess(collection, id) {
      const entry = documentAccess.get(`${collection}|${id}`);
      return entry ? { ...entry } : null;
    },
    async deleteDocumentAccess(collection, id) {
      documentAccess.delete(`${collection}|${id}`);
    },
    async scanDocumentAccess() {
      return Array.from(documentAccess.values(), (record) => ({ ...record }));
    },
    async putCacheStats(record) {
      cacheStats.set(record.databaseName, { ...record });
    },
    async getCacheStats(databaseName) {
      const entry = cacheStats.get(databaseName);
      return entry ? { ...entry } : null;
    },
    async clear() {
      queryWindows.clear();
      documentAccess.clear();
      cacheStats.clear();
    },
    async close() {
    }
  };
}
function queryWindowKey(record) {
  return [record.collection, record.queryFingerprint, record.offset, record.limit].join("|");
}
function documentAccessKey(record) {
  return `${record.collection}|${record.id}`;
}
function stringKey(key) {
  if (Array.isArray(key)) return key.join("|");
  if (typeof key === "string") return key;
  throw new TypeError("query window key must be array or string");
}

// src/apps/business-os/rxdb/src/query-meta-storage.mjs
var SIDECAR_DATABASE_NAME = "ctox_business_os_v1_5_meta";
var SIDECAR_PIN_RECENT_READ_TTL_MS = 6e4;
var PIN_RECENT_READ = "recently-read";
var QueryMetaStorage = class {
  constructor(backend, { databaseName, clock = Date.now, primaryDelete = null } = {}) {
    if (!backend) throw new TypeError("QueryMetaStorage requires a backend");
    if (!databaseName) throw new TypeError("QueryMetaStorage requires a databaseName");
    this.backend = backend;
    this.databaseName = databaseName;
    this.clock = clock;
    this.primaryDelete = typeof primaryDelete === "function" ? primaryDelete : null;
  }
  setPrimaryDelete(fn) {
    this.primaryDelete = typeof fn === "function" ? fn : null;
  }
  async getQueryWindow(key) {
    const record = await this.backend.getQueryWindow(stringKey2(key));
    if (!record) return null;
    record.lastAccessedAt = this.clock();
    await this.backend.putQueryWindow(record);
    return record;
  }
  async upsertQueryWindow({ collection, queryFingerprint: queryFingerprint2, offset, limit, documentIds, complete, authoritativeRevision }) {
    const now = this.clock();
    const existing = await this.backend.getQueryWindow(
      [collection, queryFingerprint2, offset, limit].join("|")
    );
    const record = {
      collection,
      queryFingerprint: queryFingerprint2,
      offset,
      limit,
      documentIds: [...documentIds],
      complete: Boolean(complete),
      authoritativeRevision: authoritativeRevision ?? null,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
      lastAccessedAt: now
    };
    await this.backend.putQueryWindow(record);
    return record;
  }
  async invalidateQueryWindow(key) {
    const stringified = stringKey2(key);
    const existing = await this.backend.getQueryWindow(stringified);
    if (!existing) return;
    existing.complete = false;
    existing.updatedAt = this.clock();
    await this.backend.putQueryWindow(existing);
  }
  async touchDocuments(collection, ids, { estimatedBytes = 0, pinReason = PIN_RECENT_READ } = {}) {
    const now = this.clock();
    for (const id of ids) {
      const previous = await this.backend.getDocumentAccess(collection, id) || {};
      await this.backend.putDocumentAccess({
        collection,
        id,
        lastAccessedAt: now,
        pinReason: previous.dirty ? "dirty" : pinReason,
        dirty: Boolean(previous.dirty),
        estimatedBytes: estimatedBytes || previous.estimatedBytes || 0
      });
    }
  }
  async markDirty(collection, id, dirty) {
    const previous = await this.backend.getDocumentAccess(collection, id) || {
      collection,
      id,
      lastAccessedAt: this.clock(),
      estimatedBytes: 0
    };
    await this.backend.putDocumentAccess({
      ...previous,
      dirty: Boolean(dirty),
      pinReason: dirty ? "dirty" : previous.pinReason ?? null
    });
  }
  async getDocumentAccess(collection, id) {
    const record = await this.backend.getDocumentAccess(collection, id);
    return record ? { ...record } : null;
  }
  async evictDocuments(ids) {
    const now = this.clock();
    let removed = 0;
    for (const { collection, id } of ids) {
      const record = await this.backend.getDocumentAccess(collection, id);
      if (!record) continue;
      if (record.dirty) continue;
      if (record.pinReason === PIN_RECENT_READ && now - record.lastAccessedAt < SIDECAR_PIN_RECENT_READ_TTL_MS) {
        continue;
      }
      if (this.primaryDelete) {
        try {
          await this.primaryDelete(collection, id);
        } catch {
          continue;
        }
      }
      await this.backend.deleteDocumentAccess(collection, id);
      removed += 1;
    }
    const stats = await this.backend.getCacheStats(this.databaseName) || {
      databaseName: this.databaseName,
      estimatedBytes: 0,
      budgetBytes: 0,
      lastEvictionAt: null
    };
    stats.lastEvictionAt = removed > 0 ? now : stats.lastEvictionAt;
    await this.backend.putCacheStats(stats);
    return removed;
  }
  async estimateWorkingSetBytes() {
    const docs = await this.backend.scanDocumentAccess();
    return docs.reduce((sum, record) => sum + (record.estimatedBytes || 0), 0);
  }
  async setBudgetBytes(budgetBytes) {
    const stats = await this.backend.getCacheStats(this.databaseName) || {
      databaseName: this.databaseName,
      estimatedBytes: 0,
      budgetBytes: 0,
      lastEvictionAt: null
    };
    stats.budgetBytes = Number(budgetBytes) || 0;
    await this.backend.putCacheStats(stats);
  }
  async getCacheStats() {
    return await this.backend.getCacheStats(this.databaseName) || {
      databaseName: this.databaseName,
      estimatedBytes: 0,
      budgetBytes: 0,
      lastEvictionAt: null
    };
  }
  async clear() {
    await this.backend.clear();
  }
  async close() {
    await this.backend.close();
  }
  /// Evicts LRU document access entries until the working set fits the budget.
  /// Skips dirty docs and unexpired recently-read pins. Returns the number of
  /// document records removed.
  async runEvictionIfOverBudget() {
    const stats = await this.getCacheStats();
    if (!stats.budgetBytes || stats.estimatedBytes <= stats.budgetBytes) {
      return 0;
    }
    const all = await this.backend.scanDocumentAccess();
    const now = this.clock();
    const candidates = all.filter((record) => !record.dirty).filter((record) => {
      if (record.pinReason !== "recently-read") return true;
      return now - record.lastAccessedAt >= SIDECAR_PIN_RECENT_READ_TTL_MS;
    }).sort((a, b) => a.lastAccessedAt - b.lastAccessedAt);
    let removed = 0;
    let remainingBytes = stats.estimatedBytes;
    for (const candidate of candidates) {
      if (remainingBytes <= stats.budgetBytes) break;
      if (this.primaryDelete) {
        try {
          await this.primaryDelete(candidate.collection, candidate.id);
        } catch {
          continue;
        }
      }
      await this.backend.deleteDocumentAccess(candidate.collection, candidate.id);
      remainingBytes -= candidate.estimatedBytes || 0;
      removed += 1;
    }
    if (removed > 0) {
      const updated = { ...stats, estimatedBytes: remainingBytes, lastEvictionAt: now };
      await this.backend.putCacheStats(updated);
    }
    return removed;
  }
  async recordEstimatedBytes(bytes) {
    const stats = await this.getCacheStats();
    stats.estimatedBytes = Math.max(0, Number(bytes) || 0);
    await this.backend.putCacheStats(stats);
  }
  /// Wraps an IDB write attempt in a quota-recovery loop. On
  /// `QuotaExceededError` we run eviction once and retry; on second failure
  /// the error propagates. Use this from production paths that materialize
  /// fetched chunks into the primary store.
  async withQuotaRecovery(writeFn) {
    try {
      return await writeFn();
    } catch (err) {
      if (!isQuotaExceeded(err)) throw err;
      const stats = await this.getCacheStats();
      const tighten = Math.max(1024, Math.floor((stats.budgetBytes || stats.estimatedBytes || 65536) / 2));
      await this.setBudgetBytes(tighten);
      await this.runEvictionIfOverBudget();
      try {
        return await writeFn();
      } catch (retryErr) {
        if (stats.budgetBytes) await this.setBudgetBytes(stats.budgetBytes);
        throw retryErr;
      }
    }
  }
  /// Starts a periodic eviction scheduler. The handle returned has a
  /// `stop()` method. Idempotent: calling twice with the same handle is
  /// safe. Default interval: 30s.
  startEvictionScheduler({ intervalMs = 3e4 } = {}) {
    if (this._evictionTimer) return { stop: () => this.stopEvictionScheduler() };
    this._evictionTimer = setInterval(() => {
      this.runEvictionIfOverBudget().catch(() => {
      });
    }, intervalMs);
    if (typeof this._evictionTimer.unref === "function") {
      this._evictionTimer.unref();
    }
    return { stop: () => this.stopEvictionScheduler() };
  }
  stopEvictionScheduler() {
    if (this._evictionTimer) {
      clearInterval(this._evictionTimer);
      this._evictionTimer = null;
    }
  }
  /// Orphan-window GC: drop sidecar query-window entries that haven't been
  /// read in `maxAgeMs` milliseconds (default 7 days). Documents referenced
  /// by other windows remain. This keeps the sidecar from growing monotonically
  /// as one-off queries accumulate.
  async runWindowGc({ maxAgeMs = 7 * 24 * 60 * 60 * 1e3 } = {}) {
    const now = this.clock();
    const all = await this.backend.scanQueryWindows();
    let removed = 0;
    for (const window2 of all) {
      const age = now - (window2.lastAccessedAt ?? window2.updatedAt ?? window2.createdAt ?? now);
      if (age >= maxAgeMs) {
        await this.backend.deleteQueryWindow([
          window2.collection,
          window2.queryFingerprint,
          window2.offset,
          window2.limit
        ]);
        removed += 1;
      }
    }
    return removed;
  }
};
function isQuotaExceeded(err) {
  if (!err) return false;
  if (err.name === "QuotaExceededError") return true;
  if (typeof err.code === "number" && err.code === 22) return true;
  const msg = String(err.message || "").toLowerCase();
  return msg.includes("quota") || msg.includes("storage full");
}
function createSidecarWithMemoryBackend({ databaseName = SIDECAR_DATABASE_NAME, clock = Date.now } = {}) {
  return new QueryMetaStorage(createMemoryMetaBackend(), { databaseName, clock });
}
function stringKey2(key) {
  if (Array.isArray(key)) return key.join("|");
  if (typeof key === "string") return key;
  throw new TypeError("query window key must be array or string");
}

// src/apps/business-os/rxdb/src/query-meta-backend-indexeddb.mjs
var SIDECAR_DB_VERSION = 1;
var STORE_QUERY_WINDOWS = "queryWindows";
var STORE_DOCUMENT_ACCESS = "documentAccess";
var STORE_CACHE_STATS = "cacheStats";
var OPEN_TIMEOUT_MS = 4e3;
function createIndexedDbMetaBackend({ databaseName }) {
  if (!databaseName) throw new TypeError("createIndexedDbMetaBackend requires databaseName");
  let dbPromise = null;
  const open = () => {
    if (!dbPromise) dbPromise = openSidecarDatabase(databaseName);
    return dbPromise;
  };
  return {
    name: "indexeddb",
    async putQueryWindow(record) {
      const db = await open();
      await runRequest(
        db.transaction(STORE_QUERY_WINDOWS, "readwrite").objectStore(STORE_QUERY_WINDOWS).put(record)
      );
    },
    async getQueryWindow(key) {
      const db = await open();
      return runRequest(
        db.transaction(STORE_QUERY_WINDOWS, "readonly").objectStore(STORE_QUERY_WINDOWS).get(parseQueryWindowKey(key))
      );
    },
    async deleteQueryWindow(key) {
      const db = await open();
      await runRequest(
        db.transaction(STORE_QUERY_WINDOWS, "readwrite").objectStore(STORE_QUERY_WINDOWS).delete(parseQueryWindowKey(key))
      );
    },
    async scanQueryWindows() {
      const db = await open();
      return runRequest(
        db.transaction(STORE_QUERY_WINDOWS, "readonly").objectStore(STORE_QUERY_WINDOWS).getAll()
      );
    },
    async putDocumentAccess(record) {
      const db = await open();
      await runRequest(
        db.transaction(STORE_DOCUMENT_ACCESS, "readwrite").objectStore(STORE_DOCUMENT_ACCESS).put(record)
      );
    },
    async getDocumentAccess(collection, id) {
      const db = await open();
      return runRequest(
        db.transaction(STORE_DOCUMENT_ACCESS, "readonly").objectStore(STORE_DOCUMENT_ACCESS).get([collection, id])
      );
    },
    async deleteDocumentAccess(collection, id) {
      const db = await open();
      await runRequest(
        db.transaction(STORE_DOCUMENT_ACCESS, "readwrite").objectStore(STORE_DOCUMENT_ACCESS).delete([collection, id])
      );
    },
    async scanDocumentAccess() {
      const db = await open();
      return runRequest(
        db.transaction(STORE_DOCUMENT_ACCESS, "readonly").objectStore(STORE_DOCUMENT_ACCESS).getAll()
      );
    },
    async putCacheStats(record) {
      const db = await open();
      await runRequest(
        db.transaction(STORE_CACHE_STATS, "readwrite").objectStore(STORE_CACHE_STATS).put(record)
      );
    },
    async getCacheStats(databaseName2) {
      const db = await open();
      return runRequest(
        db.transaction(STORE_CACHE_STATS, "readonly").objectStore(STORE_CACHE_STATS).get(databaseName2)
      );
    },
    async clear() {
      const db = await open();
      for (const name of [STORE_QUERY_WINDOWS, STORE_DOCUMENT_ACCESS, STORE_CACHE_STATS]) {
        await runRequest(db.transaction(name, "readwrite").objectStore(name).clear());
      }
    },
    async close() {
      if (dbPromise) {
        const db = await dbPromise;
        db.close();
        dbPromise = null;
      }
    }
  };
}
function openSidecarDatabase(databaseName) {
  if (!globalThis.indexedDB) {
    throw new Error("indexedDB is required for sidecar metadata storage");
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`IndexedDB open timed out for sidecar ${databaseName}`));
    }, OPEN_TIMEOUT_MS);
    const request = globalThis.indexedDB.open(databaseName, SIDECAR_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_QUERY_WINDOWS)) {
        const store = db.createObjectStore(STORE_QUERY_WINDOWS, {
          keyPath: ["collection", "queryFingerprint", "offset", "limit"]
        });
        store.createIndex("collection", "collection", { unique: false });
        store.createIndex("collection_lastAccessedAt", ["collection", "lastAccessedAt"], {
          unique: false
        });
      }
      if (!db.objectStoreNames.contains(STORE_DOCUMENT_ACCESS)) {
        const store = db.createObjectStore(STORE_DOCUMENT_ACCESS, {
          keyPath: ["collection", "id"]
        });
        store.createIndex("collection_lastAccessedAt", ["collection", "lastAccessedAt"], {
          unique: false
        });
      }
      if (!db.objectStoreNames.contains(STORE_CACHE_STATS)) {
        db.createObjectStore(STORE_CACHE_STATS, { keyPath: "databaseName" });
      }
    };
    request.onsuccess = () => {
      clearTimeout(timer);
      resolve(request.result);
    };
    request.onerror = () => {
      clearTimeout(timer);
      reject(request.error || new Error(`failed to open sidecar ${databaseName}`));
    };
    request.onblocked = () => {
      clearTimeout(timer);
      reject(new Error(`IndexedDB open blocked for sidecar ${databaseName}`));
    };
  });
}
function parseQueryWindowKey(key) {
  if (Array.isArray(key)) return key;
  if (typeof key === "string") {
    const parts = key.split("|");
    if (parts.length !== 4) throw new TypeError(`invalid query window key: ${key}`);
    const [collection, fingerprint, offset, limit] = parts;
    return [collection, fingerprint, Number(offset), Number(limit)];
  }
  throw new TypeError("query window key must be array or string");
}
function runRequest(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

// src/apps/business-os/rxdb/src/active-collections.mjs
var RECENT_EXEC_ACTIVE_MS = 15e3;
var ACTIVE_NOTIFY_DEBOUNCE_MS = 100;
var ActiveCollectionRegistry = class {
  constructor({ clock = () => Date.now(), recentExecMs = RECENT_EXEC_ACTIVE_MS } = {}) {
    this.clock = clock;
    this.recentExecMs = recentExecMs;
    this.subscriptionCounts = /* @__PURE__ */ new Map();
    this.lastExecAt = /* @__PURE__ */ new Map();
    this.listeners = /* @__PURE__ */ new Set();
    this.notifyTimer = null;
    this.lastNotifiedKey = null;
  }
  // A live query/collection subscription started for `collectionName`.
  subscriptionStarted(collectionName) {
    if (!collectionName) return;
    this.subscriptionCounts.set(
      collectionName,
      (this.subscriptionCounts.get(collectionName) || 0) + 1
    );
    this.scheduleNotify();
  }
  // A live subscription ended.
  subscriptionEnded(collectionName) {
    if (!collectionName) return;
    const next = (this.subscriptionCounts.get(collectionName) || 0) - 1;
    if (next <= 0) {
      this.subscriptionCounts.delete(collectionName);
    } else {
      this.subscriptionCounts.set(collectionName, next);
    }
    this.scheduleNotify();
  }
  // A one-shot `.exec()` read happened on `collectionName` — keep it active for
  // a short window so imperative reads also get foreground priority.
  markRead(collectionName) {
    if (!collectionName) return;
    this.lastExecAt.set(collectionName, this.clock());
    this.scheduleNotify();
  }
  // The current active set: every collection with a live subscription, plus
  // every collection read within the recent-exec window.
  activeCollections() {
    const now = this.clock();
    const active = /* @__PURE__ */ new Set();
    for (const [name, count] of this.subscriptionCounts.entries()) {
      if (count > 0) active.add(name);
    }
    for (const [name, at] of this.lastExecAt.entries()) {
      if (now - at <= this.recentExecMs) active.add(name);
      else this.lastExecAt.delete(name);
    }
    return active;
  }
  // Listener receives a sorted array of active collection names whenever the
  // active set changes. Returns an unsubscribe function. The listener fires
  // immediately with the current set.
  onChange(listener) {
    if (typeof listener !== "function") return () => {
    };
    this.listeners.add(listener);
    try {
      listener(this.activeCollectionsList());
    } catch {
    }
    return () => {
      this.listeners.delete(listener);
    };
  }
  activeCollectionsList() {
    return Array.from(this.activeCollections()).sort();
  }
  scheduleNotify() {
    if (this.notifyTimer != null) return;
    this.notifyTimer = setTimeout(() => {
      this.notifyTimer = null;
      const list = this.activeCollectionsList();
      const key = list.join("\0");
      if (key === this.lastNotifiedKey) return;
      this.lastNotifiedKey = key;
      for (const listener of this.listeners) {
        try {
          listener(list);
        } catch {
        }
      }
    }, ACTIVE_NOTIFY_DEBOUNCE_MS);
  }
};
var SINGLETON = null;
function getActiveCollectionRegistry() {
  if (!SINGLETON) SINGLETON = new ActiveCollectionRegistry();
  return SINGLETON;
}
function createActiveCollectionRegistry(options = {}) {
  return new ActiveCollectionRegistry(options);
}

// src/apps/business-os/rxdb/src/replication-webrtc.mjs
var ACTIVE_COLLECTIONS_METHOD = "rxdb.activeCollections";
var DEFAULT_QUERY_META_BUDGET_BYTES = 128 * 1024 * 1024;
var BROWSER_CAPABILITIES = [
  "ctox-rxdb-browser-v1",
  "ctox-file-chunks-v1",
  "ctox-schema-hash-v1",
  "ctox-peer-session-v1",
  "ctox-checkpoint-epoch-v1",
  CTOX_QUERY_FETCH_CAPABILITY
];
function remoteSupportsQueryFetch(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== "object") return false;
  const capabilities = Array.isArray(remoteProtocol.capabilities) ? remoteProtocol.capabilities : [];
  if (!capabilities.includes(CTOX_QUERY_FETCH_CAPABILITY)) return false;
  const flag = remoteProtocol.v1_5?.queryDemandLoadingEnabled;
  if (flag === false) return false;
  return true;
}
function getConnectionHandlerSimplePeer({ signalingServerUrl, config } = {}) {
  return {
    kind: "ctox-native-webrtc",
    signalingServerUrl,
    config: config || {}
  };
}
var SHARED_ROOM_PEERS = /* @__PURE__ */ new Map();
var SHARED_HANDSHAKE_TIMEOUT_MS = 6e4;
var SHARED_TOKEN_TIMEOUT_MS = 3e4;
var SHARED_PEER_OPEN_WAIT_MS = 6e4;
var VOLATILE_SIGNALING_QUERY_PARAMS = /* @__PURE__ */ new Set([
  "client",
  "role",
  "peer_role",
  "instance_id",
  "instance",
  "protocol",
  "cap",
  "capability",
  "capabilities",
  "token",
  "token_iat",
  "token_exp"
]);
function sharedRoomPeerKey(signalingUrl, room) {
  return `${stableSignalingUrlKey(signalingUrl)}::${String(room || "")}`;
}
function stableSignalingUrlKey(signalingUrl) {
  const raw = String(signalingUrl || "");
  try {
    const url = new URL(raw, "ws://local");
    for (const key of [...url.searchParams.keys()]) {
      if (VOLATILE_SIGNALING_QUERY_PARAMS.has(key)) {
        url.searchParams.delete(key);
      }
    }
    url.hash = "";
    return url.toString();
  } catch {
    return raw;
  }
}
var replicationWebRtcTestInternals = Object.freeze({
  sharedRoomPeerKey,
  stableSignalingUrlKey
});
function isTransientSharedPeerError(error) {
  const message = String(error?.message || error || "");
  return message.includes(" is not open") || message.includes("WebRTC peer") || message.includes("Peer closed") || message.includes("peer closed") || message.includes("channel-close") || message.includes("Timed out waiting for WebRTC response ctoxProtocol");
}
var SharedRoomPeer = class {
  constructor({ key, signalingUrl, room, iceServers, expectedNativePeerId }) {
    this.key = key;
    this.signalingUrl = signalingUrl;
    this.room = room;
    this.iceServers = iceServers;
    this.expectedNativePeerId = expectedNativePeerId;
    this.collections = /* @__PURE__ */ new Map();
    this.refCount = 0;
    this.peer = null;
    this.demandTransport = createDemandLoadingTransport({
      getPeerId: () => this.activeRemotePeerId
    });
    this.activeRemotePeerId = null;
    this.started = false;
    this.peerOpenQueue = Promise.resolve();
    this.negotiated = null;
    this.schemaMismatchCollections = /* @__PURE__ */ new Set();
    this.collectionCatchUps = /* @__PURE__ */ new Map();
    this.negotiationCatchUp = null;
    this.activeRegistry = getActiveCollectionRegistry();
    this.activeRegistryUnsub = null;
    this.lastActiveCollectionsSent = null;
  }
  representativeCollection() {
    const first = this.collections.keys().next();
    return first.done ? null : this.collections.get(first.value);
  }
  register(collection, registration) {
    this.collections.set(collection, registration);
    this.refCount += 1;
    this.scheduleCollectionCatchUp(collection, registration);
  }
  scheduleAllCollectionCatchUps() {
    for (const [collection, registration] of this.collections.entries()) {
      this.scheduleCollectionCatchUp(collection, registration);
    }
  }
  scheduleCollectionCatchUp(collection, registration) {
    if (!collection || this.collectionCatchUps.has(collection)) return;
    const run = this.peerOpenQueue.then(() => this.catchUpRegisteredCollection(collection, registration)).catch((error) => registration.state?.emitError?.(error)).finally(() => this.collectionCatchUps.delete(collection));
    this.collectionCatchUps.set(collection, run);
  }
  async catchUpRegisteredCollection(collection, registration) {
    const negotiated = await this.ensureNegotiatedPeer();
    if (!negotiated || !this.isPeerOpen(negotiated.peerId)) return;
    const { peerId, queryFetchCapable } = negotiated;
    const existingPeerStates = registration.state?.peerStates$?.getValue?.();
    if (existingPeerStates?.has?.(peerId) && registration.state?.isPeerOpen?.(peerId)) return;
    if (this.schemaMismatchCollections.has(collection)) return;
    const remoteProtocol = this.remoteProtocolForCollection(negotiated.remoteProtocol, collection);
    const localSchemas = await this.collectCollectionSchemas();
    const only = { [collection]: localSchemas[collection] };
    const mismatches = assertCollectionSchemasCompatible(only, remoteProtocol);
    if (mismatches.has(collection)) {
      this.schemaMismatchCollections.add(collection);
      registration.state?.emitError?.(mismatches.get(collection));
      return;
    }
    await registration.state?.onPeerReady?.(peerId, remoteProtocol, queryFetchCapable);
  }
  async ensureNegotiatedPeer(peerIdHint = "") {
    if (this.negotiated && this.isPeerOpen(this.negotiated.peerId)) return this.negotiated;
    if (this.negotiationCatchUp) return this.negotiationCatchUp;
    const hintedPeerId = peerIdHint && this.isPeerOpen(peerIdHint) ? peerIdHint : "";
    const peerId = hintedPeerId || this.openSharedPeerIds()[0] || await this.waitForOpenSharedPeerId().catch(() => null);
    if (!peerId) return null;
    this.negotiationCatchUp = Promise.resolve().then(async () => {
      if (this.negotiated && this.isPeerOpen(this.negotiated.peerId)) return this.negotiated;
      if (!this.isPeerOpen(peerId)) return null;
      return this.negotiatePeer(peerId);
    }).finally(() => {
      this.negotiationCatchUp = null;
    });
    return this.negotiationCatchUp;
  }
  unregister(collection) {
    this.collections.delete(collection);
    this.refCount = Math.max(0, this.refCount - 1);
    if (this.refCount === 0) {
      SHARED_ROOM_PEERS.delete(this.key);
      try {
        this.peer?.close?.();
      } catch {
      }
      this.peer = null;
      this.started = false;
      if (this.activeRegistryUnsub) {
        try {
          this.activeRegistryUnsub();
        } catch {
        }
        this.activeRegistryUnsub = null;
      }
    }
  }
  ensurePeer() {
    if (this.peer) return this.peer;
    this.peer = createCtoxWebRtcNativePeer({
      signalingUrl: this.signalingUrl,
      // Phase 3: the room is the bare sync_room — NOT a per-collection topic.
      room: this.room,
      clientId: browserInitiatorPeerId(this.room),
      role: "browser",
      capabilities: BROWSER_CAPABILITIES,
      iceServers: this.iceServers,
      expectedNativePeerId: this.expectedNativePeerId || "",
      protocolPayload: async ({ collection } = {}) => this.buildProtocolPayload(collection),
      requestHandlers: {
        masterChangesSince: async ({ params, peerId, collection }) => this.routeMasterChangesSince(collection, params, peerId),
        masterWrite: async ({ params, peerId, collection }) => this.routeMasterWrite(collection, params, peerId),
        ...this.demandTransport.requestHandlers
      }
    });
    this.demandTransport.attach(this.peer);
    this.peer.on("error", (event) => this.fanout("error", event.detail || event));
    this.peer.on("transport-status", (event) => this.fanout("transport-status", event.detail || event));
    this.peer.on("peer-open", (event) => {
      const peerId = event.detail.peerId;
      this.peerOpenQueue = this.peerOpenQueue.then(async () => {
        try {
          const negotiated = await this.ensureNegotiatedPeer(peerId);
          if (!negotiated) return;
          this.scheduleAllCollectionCatchUps();
        } catch (error) {
          if (isTransientSharedPeerError(error)) return;
          this.fanout("handshake-error", error);
        }
      });
    });
    this.peer.on("peer-close", (event) => {
      if (this.negotiated && this.negotiated.peerId === event.detail?.peerId) {
        this.negotiated = null;
      }
      if (this.activeRemotePeerId === event.detail?.peerId) {
        this.activeRemotePeerId = null;
      }
      this.fanout("peer-close", event.detail);
    });
    this.peer.on("peer-state", (event) => this.fanout("peer-state", event.detail));
    this.peer.on("master-change", (event) => {
      const collection = event.detail?.collection || event.collection || null;
      this.fanoutMasterChange(collection);
    });
    if (!this.activeRegistryUnsub) {
      this.activeRegistryUnsub = this.activeRegistry.onChange((names) => {
        this.sendActiveCollections(names);
      });
    }
    return this.peer;
  }
  // Phase 2: send `rxdb.activeCollections` (fire-and-forget) to the active
  // native peer. No-op until a peer is open. Resent on (re)handshake because
  // the native peer drops its per-peer active set on disconnect.
  sendActiveCollections(names) {
    const list = Array.isArray(names) ? names : this.activeRegistry.activeCollectionsList();
    const peerId = this.activeRemotePeerId;
    if (!peerId || !this.peer) return;
    const key = list.join(" ");
    this.lastActiveCollectionsSent = key;
    try {
      this.peer.send(peerId, {
        id: `active-collections|${Date.now()}`,
        method: ACTIVE_COLLECTIONS_METHOD,
        params: [list]
      });
    } catch {
    }
  }
  start() {
    this.ensurePeer();
    if (this.started) return;
    this.started = true;
    this.peer.connect();
  }
  fanout(eventName, detail) {
    for (const registration of this.collections.values()) {
      try {
        registration.state?.onSharedEvent?.(eventName, detail);
      } catch {
      }
    }
  }
  fanoutMasterChange(collection) {
    if (collection) {
      const registration = this.collections.get(collection);
      registration?.state?.onMasterChange?.();
      return;
    }
    for (const registration of this.collections.values()) {
      try {
        registration.state?.onMasterChange?.();
      } catch {
      }
    }
  }
  async buildProtocolPayload(collection) {
    const registration = collection && this.collections.get(collection) || this.representativeCollection();
    if (!registration) {
      return buildProtocolPayload({
        role: "browser",
        peerSessionId: `browser:${this.room}`,
        peerGeneration: 1,
        capabilities: BROWSER_CAPABILITIES
      });
    }
    const payload = await registration.state.buildProtocolPayload();
    if (this.collections.size > 1) {
      payload.collectionSchemas = await this.collectCollectionSchemas();
      payload.collectionCheckpoints = await this.collectCollectionCheckpoints();
    }
    return payload;
  }
  // Build `{ collectionName -> { schemaVersion, schemaHash, schemaHashSource } }`
  // across every registered collection on this shared connection.
  async collectCollectionSchemas() {
    const map = {};
    for (const [name, registration] of this.collections.entries()) {
      const state = registration.state;
      if (!state) continue;
      let hash = state.schemaHashValue;
      if (!hash) {
        try {
          hash = await state.collection.schema.hash();
        } catch {
          hash = null;
        }
      }
      map[name] = {
        schemaVersion: state.collection?.schema?.version ?? null,
        schemaHash: hash || null,
        schemaHashSource: schemaHashSource(name)
      };
    }
    return map;
  }
  async collectCollectionCheckpoints() {
    const map = {};
    for (const [name, registration] of this.collections.entries()) {
      const state = registration.state;
      if (!state) continue;
      let hash = state.schemaHashValue;
      if (!hash) {
        try {
          hash = await state.collection.schema.hash();
        } catch {
          hash = null;
        }
      }
      try {
        const checkpoint = await state.collection.storageCollection.replicationCheckpointStatus(hash || null);
        if (checkpoint && typeof checkpoint === "object") {
          map[name] = {
            ...checkpoint,
            collection: checkpoint.collection || name
          };
        }
      } catch {
      }
    }
    return map;
  }
  remoteProtocolForCollection(remoteProtocol, collection) {
    if (!remoteProtocol || typeof remoteProtocol !== "object" || !collection) return remoteProtocol;
    const checkpoint = remoteProtocol.collectionCheckpoints?.[collection] || (remoteProtocol.collection?.name === collection ? remoteProtocol.collection?.checkpoint : null) || (remoteProtocol.checkpoint?.collection === collection ? remoteProtocol.checkpoint : null) || null;
    const schema = remoteProtocol.collectionSchemas?.[collection] || null;
    if (!checkpoint && !schema && remoteProtocol.collection?.name === collection) return remoteProtocol;
    const baseCollection = remoteProtocol.collection && typeof remoteProtocol.collection === "object" ? remoteProtocol.collection : {};
    return {
      ...remoteProtocol,
      checkpoint: checkpoint || remoteProtocol.checkpoint || null,
      collection: {
        ...baseCollection,
        name: collection,
        ...(schema || {}),
        checkpoint: checkpoint || baseCollection.checkpoint || remoteProtocol.checkpoint || null
      }
    };
  }
  async routeMasterChangesSince(collection, params, peerId) {
    const registration = collection && this.collections.get(collection);
    if (!registration) {
      return { documents: [], checkpoint: params?.[0] || null };
    }
    return registration.state.masterChangesSince(params, peerId);
  }
  async routeMasterWrite(collection, params, peerId) {
    const registration = collection && this.collections.get(collection);
    if (!registration) return [];
    return registration.state.masterWrite(params, peerId);
  }
  async negotiatePeer(peerId) {
    const representative = this.representativeCollection();
    if (!representative) return null;
    if (!this.isPeerOpen(peerId)) return null;
    const localProtocol = await this.peer.protocolPayload(peerId, [], representative.collection);
    if (!this.isPeerOpen(peerId)) return null;
    const remoteProtocol = await this.peer.request(
      peerId,
      "ctoxProtocol",
      [localProtocol],
      SHARED_HANDSHAKE_TIMEOUT_MS,
      representative.collection
    );
    const normalizedRemoteProtocol = normalizeRemoteProtocol(remoteProtocol);
    if (!this.isPeerOpen(peerId)) return null;
    const multiplexed = this.collections.size > 1;
    try {
      assertCompatibleProtocol(localProtocol, normalizedRemoteProtocol, {
        requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
        // Under multiplex the representative collection in the room handshake
        // may differ from the remote's representative, so the SINGLE-collection
        // name/hash check on `localProtocol.collection` is meaningless here. We
        // still enforce protocol + required capabilities, and validate every
        // collection's schema individually below via `collectionSchemas`.
        validateSchema: !multiplexed
      });
    } catch (error) {
      this.peer?.removeConnection?.(peerId, "protocol-incompatible");
      this.fanout("handshake-error", error);
      throw error;
    }
    if (normalizedRemoteProtocol?.peerSession?.role !== "ctox_instance") {
      this.peer?.removeConnection?.(peerId, "non-native-peer-role");
      return null;
    }
    this.schemaMismatchCollections = /* @__PURE__ */ new Set();
    if (multiplexed) {
      const localSchemas = await this.collectCollectionSchemas();
      const mismatches = assertCollectionSchemasCompatible(localSchemas, normalizedRemoteProtocol);
      for (const [name, error] of mismatches.entries()) {
        this.schemaMismatchCollections.add(name);
        const registration = this.collections.get(name);
        registration?.state?.emitError(error);
      }
    }
    await this.awaitRemoteMasterReady(peerId);
    const queryFetchCapable = remoteSupportsQueryFetch(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    this.sendActiveCollections();
    this.negotiated = { peerId, remoteProtocol: normalizedRemoteProtocol, queryFetchCapable };
    return this.negotiated;
  }
  isPeerOpen(peerId) {
    const connection = this.peer?.connections?.get?.(peerId);
    if (!connection) return false;
    const channelState = connection.channel?.readyState || "";
    const pcState = connection.peer?.connectionState || "";
    return channelState === "open" && !["closed", "failed", "disconnected"].includes(pcState);
  }
  openSharedPeerIds() {
    const ids = [];
    for (const peerId of this.peer?.connections?.keys?.() || []) {
      if (this.isPeerOpen(peerId)) ids.push(peerId);
    }
    return ids;
  }
  async waitForOpenSharedPeerId(timeoutMs = SHARED_PEER_OPEN_WAIT_MS) {
    const immediate = this.openSharedPeerIds()[0];
    if (immediate) return immediate;
    this.ensurePeer();
    return new Promise((resolve, reject) => {
      let settled = false;
      let unsubscribe = null;
      let interval = null;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (interval) clearInterval(interval);
        try {
          unsubscribe?.();
        } catch {
        }
        handler(value);
      };
      const inspect = () => {
        const peerId = this.openSharedPeerIds()[0];
        if (peerId) settle(resolve, peerId);
      };
      const timer = setTimeout(() => {
        settle(reject, new Error(`Timed out waiting for shared WebRTC peer in ${this.room}`));
      }, timeoutMs);
      unsubscribe = this.peer?.on?.("peer-open", (event) => {
        const peerId = event.detail?.peerId;
        if (peerId && this.isPeerOpen(peerId)) settle(resolve, peerId);
        else inspect();
      }) || null;
      interval = setInterval(inspect, 500);
      inspect();
    });
  }
  async awaitRemoteMasterReady(peerId) {
    try {
      await this.peer.waitForRequest?.(peerId, "token", 2e3);
    } catch {
    }
    await delay2(100);
  }
  getTransportStatus() {
    return this.peer?.getTransportStatus?.() || {};
  }
};
function getOrCreateSharedRoomPeer({ signalingUrl, room, iceServers, expectedNativePeerId }) {
  const key = sharedRoomPeerKey(signalingUrl, room);
  let shared = SHARED_ROOM_PEERS.get(key);
  if (!shared) {
    shared = new SharedRoomPeer({ key, signalingUrl, room, iceServers, expectedNativePeerId });
    SHARED_ROOM_PEERS.set(key, shared);
  }
  return shared;
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
    this.shared = null;
    this.initialReplicationDeferred = createDeferred();
    this.initialReplication = this.initialReplicationDeferred.promise;
    this.cancelled = false;
    this.pullCheckpointsByPeer = /* @__PURE__ */ new Map();
    this.pushCheckpointsByPeer = /* @__PURE__ */ new Map();
    this.changeSubscription = null;
    this.periodicPullTimer = null;
    this.periodicPushTimer = null;
    this.pullInProgress = false;
    this.pullInProgressPromise = null;
    this.pullAgainAfterCurrent = false;
    this.pushInProgress = false;
    this.pushInProgressPromise = null;
    this.activeRemotePeerId = null;
    this.demandLoaderActive = false;
    this.schemaHashValue = null;
    this.peerReadyPromisesByPeer = /* @__PURE__ */ new Map();
  }
  get peer() {
    return this.shared?.peer || null;
  }
  async start(connectionHandlerCreator) {
    this.schemaHashValue = await this.collection.schema.hash();
    const signalingUrl = connectionHandlerCreator?.signalingServerUrl;
    const iceServers = connectionHandlerCreator?.config?.iceServers || [];
    this.shared = getOrCreateSharedRoomPeer({
      signalingUrl,
      room: this.topic,
      iceServers,
      expectedNativePeerId: this.ctox?.expectedNativePeerId || ""
    });
    this.shared.register(this.collection.name, {
      collection: this.collection.name,
      state: this
    });
    this.shared.start();
    this.changeSubscription = this.collection.observe(() => {
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    });
    const periodicPushMs = this.periodicPushIntervalMs();
    if (periodicPushMs > 0) {
      this.periodicPushTimer = setInterval(() => {
        this.pushToRemotePeers().catch((error) => this.error$.next(error));
      }, periodicPushMs);
    }
  }
  // ----- shared peer event sinks (called by SharedRoomPeer) ---------------
  onSharedEvent(eventName, detail) {
    if (this.cancelled) return;
    if (eventName === "error") {
      this.error$.next(detail?.detail || detail);
      return;
    }
    if (eventName === "handshake-error") {
      this.rejectInitialReplication(detail);
      this.error$.next(detail);
      return;
    }
    if (eventName === "transport-status") {
      this.transportStatus$.next(this.decorateTransportStatus(detail || {}));
      return;
    }
    if (eventName === "peer-close") {
      this.removePeer(detail?.peerId, detail?.reason || "peer-close");
      return;
    }
    if (eventName === "peer-state") {
      const stateName = detail?.state || "";
      if (["closed", "failed", "disconnected"].includes(stateName)) {
        this.removePeer(detail?.peerId, `peer-${stateName}`);
      }
    }
  }
  onMasterChange() {
    if (this.cancelled) return;
    this.pullFromRemotePeers().catch((error) => this.error$.next(error));
  }
  emitError(error) {
    this.error$.next(error);
  }
  async buildProtocolPayload() {
    const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    return buildProtocolPayload({
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema.version,
      schemaHash: this.schemaHashValue,
      schemaHashSource: schemaHashSource(this.collection.name),
      peerSessionId: `browser:${this.topic}`,
      peerGeneration: 1,
      checkpoint,
      role: "browser",
      capabilities: BROWSER_CAPABILITIES
    });
  }
  async onPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable) {
    if (this.peerReadyPromisesByPeer.has(peerId)) {
      return this.peerReadyPromisesByPeer.get(peerId);
    }
    const run = this.runPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable).finally(() => this.peerReadyPromisesByPeer.delete(peerId));
    this.peerReadyPromisesByPeer.set(peerId, run);
    return run;
  }
  async runPeerReady(peerId, normalizedRemoteProtocol, queryFetchCapable) {
    if (this.cancelled) return;
    this.ctox?.onPeerProtocol?.(normalizedRemoteProtocol);
    this.activeRemotePeerId = peerId;
    const peerStates = new Map(this.peerStates$.getValue() || /* @__PURE__ */ new Map());
    peerStates.set(peerId, {
      peerId,
      replicationState: this,
      remoteProtocol: normalizedRemoteProtocol,
      queryFetchCapable
    });
    this.peerStates$.next(peerStates);
    this.active$.next(true);
    this.transportStatus$.next(this.decorateTransportStatus(this.shared?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {}));
    if (queryFetchCapable && !this.demandLoaderActive) {
      try {
        await this.enableDemandLoading();
      } catch (error) {
        this.error$.next(error);
      }
    }
    this.ctox?.onPeerCapabilityNegotiated?.({
      peerId,
      queryFetchCapable,
      capabilities: normalizedRemoteProtocol?.capabilities || [],
      demandLoaderActive: this.demandLoaderActive
    });
    try {
      this.initialReplication = this.pullFromRemotePeers().then(() => this.pushToRemotePeers());
      await this.initialReplication;
      this.resolveInitialReplication();
    } catch (error) {
      this.rejectInitialReplication(error);
      throw error;
    }
  }
  // ----- pull / push (collection-tagged over the shared peer) -------------
  async pullFromRemotePeers() {
    if (!this.pull) return;
    if (this.pullInProgressPromise) {
      this.pullAgainAfterCurrent = true;
      return this.pullInProgressPromise;
    }
    this.pullInProgress = true;
    this.pullAgainAfterCurrent = false;
    this.pullInProgressPromise = (async () => {
      do {
        this.pullAgainAfterCurrent = false;
        const peerIds = this.openPeerIds();
        const results = await Promise.allSettled(peerIds.map((peerId) => this.pullFromPeer(peerId)));
        this.reportPeerResults(results, peerIds);
      } while (this.pullAgainAfterCurrent && !this.cancelled);
    })().finally(() => {
      this.pullInProgress = false;
      this.pullInProgressPromise = null;
      this.pullAgainAfterCurrent = false;
    });
    return this.pullInProgressPromise;
  }
  async pullFromPeer(peerId) {
    const batchSize = Number(this.pull?.batchSize || 10);
    let activePeerId = peerId;
    let checkpoint = this.pullCheckpointsByPeer.get(activePeerId) || null;
    while (!this.cancelled) {
      const response = await this.requestMasterChangesSince(activePeerId, checkpoint, batchSize);
      activePeerId = response.peerId || activePeerId;
      const result = response.result || {};
      const documents = Array.isArray(result?.documents) ? result.documents : [];
      if (documents.length) {
        await this.invalidateDemandCacheForRemoteWrite();
        await this.collection.storageCollection.bulkWrite(documents, {
          replicationOrigin: this.replicationOriginForPeer(activePeerId)
        });
        await this.invalidateDemandCacheForRemoteWrite();
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pullCheckpointsByPeer.set(activePeerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }
  async requestMasterChangesSince(peerId, checkpoint, batchSize) {
    const timeoutMs = this.requestTimeoutMsFor("masterChangesSince");
    const maxAttempts = 3;
    let activePeerId = peerId;
    let lastError = null;
    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      try {
        const result = await this.peer.request(
          activePeerId,
          "masterChangesSince",
          [checkpoint, batchSize],
          timeoutMs,
          this.collection.name
        );
        return { peerId: activePeerId, result };
      } catch (error) {
        lastError = error;
        if (attempt >= maxAttempts || this.cancelled || !this.isTransientMasterChangesSinceError(error)) {
          throw error;
        }
        activePeerId = await this.waitForOpenPeerId().catch(() => {
          throw error;
        });
        await delay2(250);
      }
    }
    throw lastError;
  }
  async pushToRemotePeers() {
    if (!this.push) return;
    if (this.pushInProgressPromise) return this.pushInProgressPromise;
    this.pushInProgress = true;
    this.pushInProgressPromise = (async () => {
      const peerIds = this.openPeerIds();
      try {
        const results = await Promise.allSettled(peerIds.map((peerId) => this.pushToPeer(peerId)));
        this.reportPeerResults(results, peerIds);
      } finally {
        this.pushInProgress = false;
        this.pushInProgressPromise = null;
      }
    })();
    return this.pushInProgressPromise;
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
      let rows = documents.map((doc) => ({
        newDocumentState: doc,
        assumedMasterState: null
      }));
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const conflicts = await this.peer.request(
          peerId,
          "masterWrite",
          [rows],
          this.requestTimeoutMsFor("masterWrite"),
          this.collection.name
        );
        const conflictMap = documentsByPrimaryPath(conflicts, this.collection.schema.primaryPath);
        if (!conflictMap.size) {
          rows = [];
          break;
        }
        rows = rows.map((row) => {
          const id = primaryValue(row.newDocumentState, this.collection.schema.primaryPath);
          const assumedMasterState = conflictMap.get(id);
          return assumedMasterState ? { ...row, assumedMasterState } : null;
        }).filter(Boolean);
        if (!rows.length) break;
      }
      if (rows.length) {
        throw new Error(`masterWrite conflicts remained for ${this.collection.name}`);
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      if (documents.length < batchSize) break;
    }
  }
  // ----- master handler (when CTOX picks the browser as fork's master) ----
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
      await this.invalidateDemandCacheForRemoteWrite();
      await this.collection.storageCollection.bulkWrite(docs, {
        replicationOrigin: this.replicationOriginForPeer(peerId)
      });
      await this.invalidateDemandCacheForRemoteWrite();
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
    return this.decorateTransportStatus(this.shared?.getTransportStatus?.() || this.transportStatus$.getValue?.() || {});
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
    try {
      this.demandLoader?.abortAllInFlight?.("replication-cancel");
    } catch {
    }
    try {
      this.demandSidecar?.stopEvictionScheduler?.();
    } catch {
    }
    try {
      await this.demandSidecar?.close?.();
    } catch {
    }
    this.shared?.unregister?.(this.collection.name);
    this.shared = null;
  }
  /// V1.5 production wiring: build the sidecar + query demand loader and attach
  /// them to the underlying collection so that `find().exec()` and observable
  /// queries flow through the on-demand pipeline. Idempotent. Uses the SHARED
  /// peer's demand transport (chunks route by requestId globally).
  async enableDemandLoading({
    databaseName,
    indexedDbAvailable = typeof globalThis.indexedDB === "object" && globalThis.indexedDB
  } = {}) {
    if (this.demandLoaderActive) return this.demandLoader;
    const demandTransport = this.shared?.demandTransport;
    if (!demandTransport) return null;
    const dbName = databaseName || `ctox_business_os_v1_5_meta_${this.collection.name}`;
    const backend = indexedDbAvailable ? createIndexedDbMetaBackend({ databaseName: dbName }) : createMemoryMetaBackend();
    const primaryDelete = async (collection, id) => {
      if (collection !== this.collection.name) return;
      if (typeof this.collection.storageCollection.hardDeleteByIds === "function") {
        await this.collection.storageCollection.hardDeleteByIds([id]);
      }
    };
    this.demandSidecar = new QueryMetaStorage(backend, {
      databaseName: dbName,
      primaryDelete
    });
    try {
      await this.demandSidecar.setBudgetBytes(DEFAULT_QUERY_META_BUDGET_BYTES);
    } catch {
    }
    try {
      this.demandSidecar.startEvictionScheduler({ intervalMs: 3e4 });
    } catch {
    }
    this.demandLoader = createQueryDemandLoader({
      storageCollection: this.collection.storageCollection,
      sidecar: this.demandSidecar,
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema?.version || 0,
      requestQueryFetch: (envelope) => demandTransport.requestQueryFetch(envelope),
      requestCancel: ({ requestId, reason }) => demandTransport.requestQueryCancel({ requestId, reason }),
      status: null
    });
    if (typeof this.collection.setDemandLoader === "function") {
      this.collection.setDemandLoader(this.demandLoader);
    }
    this.demandFileLoader = createFileDemandLoader({
      collectionName: this.collection.name,
      storageCollection: this.collection.storageCollection,
      sidecarBackend: backend,
      requestFileFetch: ({ requestId, fileId, range, knownSequences }) => demandTransport.requestFileFetch({
        requestId,
        fileId,
        range,
        knownSequences,
        collectionName: this.collection.name
      })
    });
    this.demandLoaderActive = true;
    return this.demandLoader;
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
  remoteProtocolForPeer(peerId) {
    const localProtocol = (this.peerStates$.getValue() || /* @__PURE__ */ new Map()).get(peerId)?.remoteProtocol || null;
    if (localProtocol) return localProtocol;
    const negotiated = this.shared?.negotiated || null;
    return negotiated?.peerId === peerId ? this.shared?.remoteProtocolForCollection?.(negotiated.remoteProtocol, this.collection.name) || negotiated.remoteProtocol || null : null;
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
  async invalidateDemandCacheForRemoteWrite() {
    try {
      await this.demandLoader?.invalidateCollectionChange?.();
    } catch {
    }
  }
  requestTimeoutMsFor(method) {
    if (this.collection.name === "desktop_file_chunks") {
      return method === "masterChangesSince" ? 45e3 : 3e4;
    }
    return 15e3;
  }
  periodicPullIntervalMs() {
    return 0;
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
    if (!open.length && this.shared?.negotiated?.peerId && this.shared.isPeerOpen?.(this.shared.negotiated.peerId)) {
      open.push(this.shared.negotiated.peerId);
    }
    return open;
  }
  async waitForOpenPeerId(timeoutMs = 8e3) {
    const immediatePeerId = this.openPeerIds()[0];
    if (immediatePeerId) return immediatePeerId;
    return new Promise((resolve, reject) => {
      let settled = false;
      let subscription = null;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        try {
          subscription?.unsubscribe?.();
        } catch {
        }
        handler(value);
      };
      const inspect = () => {
        const peerId = this.openPeerIds()[0];
        if (peerId) settle(resolve, peerId);
      };
      const timer = setTimeout(() => {
        settle(reject, new Error(`Timed out waiting for WebRTC peer reopen for ${this.collection.name}`));
      }, timeoutMs);
      subscription = this.peerStates$?.subscribe?.(inspect) || null;
      inspect();
    });
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
    const localPeerCount = (this.peerStates$.getValue?.() || /* @__PURE__ */ new Map()).size;
    const sharedPeerCount = this.shared?.openSharedPeerIds?.().length || 0;
    const connectionPeerCount = Array.isArray(status.connectionStates) ? status.connectionStates.filter((connection) => {
      const channelState = connection?.channelState || "";
      const pcState = connection?.peerConnectionState || "";
      return channelState === "open" && !["closed", "failed", "disconnected"].includes(pcState);
    }).length : 0;
    return {
      ...status,
      collection: this.collection.name,
      topic: this.topic,
      activePeerCount: Math.max(localPeerCount, sharedPeerCount, connectionPeerCount),
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
var BROWSER_PEER_SESSION_ID = createBrowserPeerSessionId();
function browserInitiatorPeerId(topic) {
  const origin = browserPeerOriginId();
  const stableScope = `${String(topic || "ctox")}|${origin}|${BROWSER_PEER_SESSION_ID}`;
  return `000-browser-${hashString(stableScope)}`;
}
function browserPeerOriginId() {
  try {
    return String(globalThis.location?.origin || globalThis.location?.host || "local");
  } catch {
    return "local";
  }
}
function createBrowserPeerSessionId() {
  try {
    const bytes = new Uint8Array(8);
    globalThis.crypto?.getRandomValues?.(bytes);
    if (bytes.some(Boolean)) {
      return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    }
  } catch {
  }
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
}
function hashString(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}
function documentsByPrimaryPath(documents = [], primaryPath = "id") {
  const map = /* @__PURE__ */ new Map();
  for (const doc of Array.isArray(documents) ? documents : []) {
    const id = primaryValue(doc, primaryPath);
    if (id) map.set(id, doc);
  }
  return map;
}
function primaryValue(doc = {}, primaryPath = "id") {
  if (!doc || typeof doc !== "object") return "";
  if (doc.id != null) return String(doc.id);
  if (doc._id != null) return String(doc._id);
  return String(replicationValueAtPath(doc, primaryPath) ?? "");
}
function replicationValueAtPath(obj, path) {
  if (!path || path === "id") return obj?.id;
  return String(path).split(".").reduce((acc, part) => acc == null ? void 0 : acc[part], obj);
}
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
    checkpoint: payload.checkpoint || payload.collection?.checkpoint || null,
    collectionCheckpoints: normalizeRemoteCollectionCheckpoints(payload.collectionCheckpoints)
  };
}
function normalizeRemoteCollectionCheckpoints(map) {
  if (!map || typeof map !== "object") return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== "object") continue;
    out[name] = {
      ...entry,
      collection: typeof entry.collection === "string" && entry.collection ? entry.collection : name
    };
  }
  return Object.keys(out).length > 0 ? out : null;
}

// src/apps/business-os/rxdb/src/rx-database.mjs
function getCtoxIndexedDbStorage() {
  return { name: "ctox-indexeddb-native" };
}
async function createRxDatabase({
  name,
  storage = getCtoxIndexedDbStorage(),
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
    getCtoxIndexedDbStorage,
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
    this.demandLoader = null;
  }
  setDemandLoader(loader) {
    this.demandLoader = loader || null;
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
      sortFields: normalizeSort2(normalized.sort).map((entry) => Object.keys(entry)[0]).filter(Boolean),
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
        const registry = getActiveCollectionRegistry();
        registry.subscriptionStarted(this.name);
        let pendingTimer = null;
        const debounceMs = OBSERVABLE_DEBOUNCE_MS;
        const flushEmit = async () => {
          pendingTimer = null;
          if (!active) return;
          let documents;
          try {
            documents = await this.find().exec();
          } catch (error) {
            if (isIndexedDbConnectionClosingError(error)) return;
            throw error;
          }
          if (active) listener({ collectionName: this.name, documents });
        };
        const emit = () => {
          if (pendingTimer != null) return;
          pendingTimer = setTimeout(flushEmit, debounceMs);
        };
        flushEmit();
        const unsubscribe = this.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
            }
            unsubscribe();
            registry.subscriptionEnded(this.name);
          }
        };
      }
    };
  }
};
var OBSERVABLE_DEBOUNCE_MS = 50;
function isIndexedDbConnectionClosingError(error) {
  const message = String(error?.message || error || "");
  return error?.name === "InvalidStateError" && message.includes("database connection is closing");
}
var CtoxRxQuery = class _CtoxRxQuery {
  constructor(collection, query, single) {
    this.collection = collection;
    this.query = normalizeQuery(query, collection.schema.primaryPath);
    this.single = single;
    this.$ = {
      subscribe: (listener) => {
        let active = true;
        const registry = getActiveCollectionRegistry();
        registry.subscriptionStarted(this.collection.name);
        let pendingTimer = null;
        const flushEmit = () => {
          pendingTimer = null;
          if (!active) return;
          this.exec().then((value) => {
            if (active) listener(value);
          }).catch(() => {
          });
        };
        const emit = () => {
          if (pendingTimer != null) return;
          pendingTimer = setTimeout(flushEmit, 50);
        };
        flushEmit();
        const unsubscribe = this.collection.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
            }
            unsubscribe();
            registry.subscriptionEnded(this.collection.name);
          }
        };
      }
    };
  }
  selector(selector = {}) {
    return this._clone({ selector });
  }
  sort(sort = []) {
    return this._clone({ sort: normalizeSort2(sort) });
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
    getActiveCollectionRegistry().markRead(this.collection.name);
    let docs;
    if (this.collection.demandLoader) {
      const demandOptions = this.single && !Number.isFinite(Number(this.query.limit)) ? { window: { offset: Number(this.query.skip || 0), limit: 1 } } : void 0;
      docs = await this.collection.demandLoader.resolveQuery(this.query, demandOptions);
    } else if (typeof this.collection.storageCollection.queryDocuments === "function") {
      docs = await this.collection.storageCollection.queryDocuments(this.query, {
        matchesSelector,
        sortDocuments
      });
    } else {
      docs = await this.collection.storageCollection.allDocuments();
      docs = docs.filter((doc) => matchesSelector(doc, this.query.selector));
      docs = sortDocuments(docs, this.query.sort);
      if (Number.isFinite(this.query.skip) && this.query.skip > 0) {
        docs = docs.slice(this.query.skip);
      }
      if (Number.isFinite(this.query.limit)) {
        docs = docs.slice(0, this.query.limit);
      }
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
    const updatedAtMs = Number(fields?.updated_at_ms || Date.now());
    const next = {
      ...this._data,
      ...fields,
      updated_at_ms: updatedAtMs,
      _meta: {
        ...this._data._meta || {},
        ...fields?._meta || {},
        lwt: updatedAtMs
      }
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
    sort: normalizeSort2(query?.sort),
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
function normalizeSort2(sort = []) {
  if (!sort) return [];
  if (typeof sort === "string") return [{ [sort]: "asc" }];
  if (!Array.isArray(sort)) return [];
  return sort.map((entry) => {
    if (typeof entry === "string") return { [entry]: "asc" };
    if (!entry || typeof entry !== "object") return {};
    const [key, direction] = Object.entries(entry)[0] || [];
    if (!key) return {};
    return { [key]: normalizeSortDirection2(direction) };
  }).filter((entry) => Object.keys(entry).length);
}
function normalizeSortDirection2(direction) {
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
    lwt: documentLwt(normalized)
  };
  return normalized;
}
var ctoxRxdbTestInternals = {
  matchesSelector,
  normalizeDoc,
  normalizeQuery,
  normalizeSort: normalizeSort2,
  sortDocuments
};

// src/apps/business-os/rxdb/src/v1_5_status.mjs
var V1_5_QUERY_FETCH_CAPABILITY = CTOX_QUERY_FETCH_CAPABILITY;
var V1_5_QUERY_RPC = CTOX_QUERY_RPC;
var V1_5_STATUS_FIELDS = Object.freeze([
  "rxdbRuntime",
  "rxdbProtocolVersion",
  "transport",
  "peerConnected",
  "peerCapabilityQueryFetchV1",
  "queryDemandLoadingEnabled",
  "queryDemandLoadingActive",
  "queryFetchInFlight",
  "queryFetchSuccessCount",
  "queryFetchErrorCount",
  "queryFetchDedupHitCount",
  "indexedDbWorkingSetBytes",
  "indexedDbEvictionCount",
  "pinnedDocCount",
  "pinnedBytes",
  "lastQueryFetchMs",
  "lastTransportBackpressureMs",
  "lastReloadHydrationMs",
  "activeFileStreams",
  "fileBytesReceived",
  "fileStreamErrors",
  "fileStreamDedupHits",
  "lastFileFetchMs"
]);
function createV1_5StatusState() {
  return {
    rxdbRuntime: "ctox-rxdb-js",
    rxdbProtocolVersion: "1",
    transport: "webrtc",
    peerConnected: false,
    peerCapabilityQueryFetchV1: false,
    queryDemandLoadingEnabled: false,
    queryDemandLoadingActive: false,
    queryFetchInFlight: 0,
    queryFetchSuccessCount: 0,
    queryFetchErrorCount: 0,
    queryFetchDedupHitCount: 0,
    indexedDbWorkingSetBytes: 0,
    indexedDbEvictionCount: 0,
    pinnedDocCount: 0,
    pinnedBytes: 0,
    lastQueryFetchMs: null,
    lastTransportBackpressureMs: null,
    lastReloadHydrationMs: null,
    activeFileStreams: 0,
    fileBytesReceived: 0,
    fileStreamErrors: 0,
    fileStreamDedupHits: 0,
    lastFileFetchMs: null
  };
}
function projectStatusFromSidecar(state, sidecarStats, registry = null) {
  const next = { ...state };
  if (sidecarStats) {
    next.indexedDbWorkingSetBytes = sidecarStats.estimatedBytes || 0;
  }
  if (registry?.pinnedDocCount !== void 0) next.pinnedDocCount = registry.pinnedDocCount;
  if (registry?.pinnedBytes !== void 0) next.pinnedBytes = registry.pinnedBytes;
  return next;
}
function snapshotV1_5Status(state) {
  const snapshot = {};
  for (const field of V1_5_STATUS_FIELDS) {
    snapshot[field] = state?.[field] ?? null;
  }
  return snapshot;
}

// src/apps/business-os/rxdb/src/multi-tab-broker.mjs
var CHANNEL_PREFIX = "ctox-rxdb-v1_5-broker-";
var CLAIM_TTL_MS = 3e4;
function createBroadcastChannelBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  if (!databaseName) throw new TypeError("broker requires databaseName");
  if (typeof globalThis.BroadcastChannel !== "function") {
    return createMemoryBroker({ databaseName, tabId, clock });
  }
  const channel = new globalThis.BroadcastChannel(`${CHANNEL_PREFIX}${databaseName}`);
  const localClaims = /* @__PURE__ */ new Map();
  const remoteClaims = /* @__PURE__ */ new Map();
  const completions = /* @__PURE__ */ new Map();
  channel.onmessage = (event) => {
    const msg = event?.data;
    if (!msg || typeof msg !== "object") return;
    const now = clock();
    if (msg.type === "claim") {
      remoteClaims.set(msg.windowKey, { tabId: msg.tabId, expiresAt: now + CLAIM_TTL_MS });
    } else if (msg.type === "release") {
      remoteClaims.delete(msg.windowKey);
    } else if (msg.type === "complete") {
      remoteClaims.delete(msg.windowKey);
      const waiter = completions.get(msg.windowKey);
      if (waiter) {
        completions.delete(msg.windowKey);
        waiter.resolve(msg.result);
      }
    }
  };
  function expired(claim, now) {
    return !claim || claim.expiresAt < now;
  }
  return {
    kind: "broadcast-channel",
    tabId,
    async claim(windowKey) {
      const now = clock();
      const remote = remoteClaims.get(windowKey);
      if (remote && expired(remote, now)) {
        remoteClaims.delete(windowKey);
      } else if (remote) {
        return false;
      }
      const local = localClaims.get(windowKey);
      if (local && !expired(local, now)) return false;
      localClaims.set(windowKey, { expiresAt: now + CLAIM_TTL_MS });
      channel.postMessage({ type: "claim", windowKey, tabId, at: now });
      return true;
    },
    async release(windowKey, result = null) {
      localClaims.delete(windowKey);
      channel.postMessage({ type: "complete", windowKey, tabId, result, at: clock() });
    },
    async waitForRemote(windowKey, timeoutMs = 5e3) {
      return new Promise((resolve) => {
        const timer = setTimeout(() => {
          completions.delete(windowKey);
          resolve(null);
        }, timeoutMs);
        completions.set(windowKey, {
          resolve: (val) => {
            clearTimeout(timer);
            resolve(val);
          }
        });
      });
    },
    close() {
      try {
        channel.close();
      } catch {
      }
    }
  };
}
function createMemoryBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  const claims = /* @__PURE__ */ new Set();
  return {
    kind: "memory",
    tabId,
    async claim(windowKey) {
      if (claims.has(windowKey)) return false;
      claims.add(windowKey);
      return true;
    },
    async release(windowKey) {
      claims.delete(windowKey);
    },
    async waitForRemote() {
      return null;
    },
    close() {
    }
  };
}
function randomTabId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `tab-${Math.random().toString(36).slice(2, 12)}`;
}

// src/apps/business-os/rxdb/src/advanced-status-bridge.mjs
function buildBusinessOsAdvancedStatus({
  v15Status,
  peerSessions = [],
  remoteProtocol = null,
  feature = {}
} = {}) {
  const snapshot = snapshotV1_5Status(v15Status);
  const remoteCapabilities = Array.isArray(remoteProtocol?.capabilities) ? remoteProtocol.capabilities : [];
  const v15Negotiated = remoteCapabilities.includes(CTOX_QUERY_FETCH_CAPABILITY) && remoteProtocol?.v1_5?.queryDemandLoadingEnabled !== false;
  const ok = snapshot.peerConnected === true && snapshot.queryFetchErrorCount < 5 && snapshot.fileStreamErrors < 5;
  return {
    version: "business-os-advanced-status-v1",
    ok,
    rxdbRuntime: {
      name: "ctox-rxdb-js",
      publicName: "CTOX DB",
      source: "app-local",
      packageManager: "none",
      compatibility: "ctox-db-api",
      upstreamCompatible: false,
      upstreamCompatibility: "not-upstream-rxdb",
      apiContract: "ctox-db-business-os-v1",
      protocolVersion: snapshot.rxdbProtocolVersion
    },
    checks: {
      rxdbRuntimeAppLocal: true,
      queryDemandLoadingEnabled: snapshot.queryDemandLoadingEnabled === true,
      queryDemandLoadingActive: snapshot.queryDemandLoadingActive === true,
      peerCapabilityQueryFetch: snapshot.peerCapabilityQueryFetchV1 === true
    },
    sync: {
      mode: "webrtc",
      protocol: "ctox-rxdb-protocol-v1",
      capabilities: remoteCapabilities,
      peerSessions,
      featureFlag: feature.queryDemandLoadingEnabled ?? null,
      v15Negotiated
    },
    v1_5: {
      query: {
        inFlight: snapshot.queryFetchInFlight,
        success: snapshot.queryFetchSuccessCount,
        errors: snapshot.queryFetchErrorCount,
        dedupHits: snapshot.queryFetchDedupHitCount,
        lastFetchMs: snapshot.lastQueryFetchMs
      },
      file: {
        active: snapshot.activeFileStreams,
        bytesReceived: snapshot.fileBytesReceived,
        errors: snapshot.fileStreamErrors,
        dedupHits: snapshot.fileStreamDedupHits,
        lastFetchMs: snapshot.lastFileFetchMs
      },
      cache: {
        workingSetBytes: snapshot.indexedDbWorkingSetBytes,
        evictionCount: snapshot.indexedDbEvictionCount,
        pinnedDocs: snapshot.pinnedDocCount,
        pinnedBytes: snapshot.pinnedBytes
      },
      transport: {
        lastBackpressureMs: snapshot.lastTransportBackpressureMs,
        reloadHydrationMs: snapshot.lastReloadHydrationMs
      }
    }
  };
}
export {
  ACTIVE_COLLECTIONS_METHOD,
  ACTIVE_NOTIFY_DEBOUNCE_MS,
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
  DEFAULT_QUERY_META_BUDGET_BYTES,
  DEFAULT_WINDOW_LIMIT,
  FILE_CHUNK_PRESENCE_KEY,
  OBSERVABLE_DEBOUNCE_MS,
  QueryMetaStorage,
  RECENT_EXEC_ACTIVE_MS,
  RxDBMigrationSchemaPlugin,
  SHELL_CRITICAL_COLLECTIONS,
  SIDECAR_DATABASE_NAME,
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  V1_5_QUERY_FETCH_CAPABILITY,
  V1_5_QUERY_RPC,
  V1_5_STATUS_FIELDS,
  addRxPlugin,
  assertCompatibleProtocol,
  buildBusinessOsAdvancedStatus,
  buildProtocolPayload,
  canonicalJson,
  canonicalQueryJson,
  canonicalizeQueryInput,
  createActiveCollectionRegistry,
  createBroadcastChannelBroker,
  createCtoxWebRtcNativePeer,
  createDemandLoadingTransport,
  createFileDemandLoader,
  createIndexedDbMetaBackend,
  createMemoryBroker,
  createMemoryMetaBackend,
  createQueryDemandLoader,
  createRxDatabase,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
  ctoxIndexedDbStorageTestInternals,
  ctoxRxdbTestInternals,
  decodeChunk,
  getActiveCollectionRegistry,
  getConnectionHandlerSimplePeer,
  getCtoxIndexedDbStorage,
  normalizeSignalingControlPlaneError,
  openCtoxIndexedDbStorage,
  projectStatusFromSidecar,
  queryFingerprint,
  remoteSupportsQueryFetch,
  removeRxDatabase,
  replicateWebRTC,
  replicationWebRtcTestInternals,
  rxdbCore,
  schemaHash,
  schemaHashSource,
  setV15LogSink,
  sha256Hex,
  snapshotV1_5Status,
  waitForEvent
};
