// CTOX Sync Engine app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs.

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
var CTOX_PRESENCE_CAPABILITY = "ctox-presence-v1";
var CTOX_PRESENCE_RPC = Object.freeze({
  update: "rxdb.presence.update",
  streamId: "presence$",
  ttlMs: 45e3,
  refreshMs: 2e4,
  maxEntriesPerPeer: 32
});
var CTOX_COMMAND_LIFECYCLE_CAPABILITY = "ctox-command-lifecycle-v2";
var CTOX_CHECKPOINT_GENERATION_CAPABILITY = "ctox-checkpoint-generation-v2";
var CTOX_APP_RUNTIME_CAPABILITY = "ctox-app-runtime-v1";

// src/apps/business-os/rxdb/src/schema.mjs
var CTOX_SCHEMA_HASH_CAPABILITY = "ctox-schema-hash-v1";
var CTOX_PEER_SESSION_CAPABILITY = "ctox-peer-session-v1";
var CTOX_CHECKPOINT_EPOCH_CAPABILITY = "ctox-checkpoint-epoch-v1";
var CTOX_BUSINESS_OS_SCHEMA_HASHES = Object.freeze({
  accounting_accounts: "49289609d65cd6dd1ce2ed700bcdf90e3ea4d1c544a49d6c53bc198f0b36090b",
  accounting_bank_statement_lines: "293037b34bd0611ad1517b57ed3cb6e16df4c64f6f2d015a29244cc44b8d1c79",
  accounting_bank_statements: "9afe72d42212dae5f94b0280530c466fee83121dbfc125c831e0cf4e5082e29c",
  accounting_credit_notes: "aaa9857647733e4bc62a0a6582cc2726c540f7c6ca646cf89d8e04562aef1c73",
  accounting_dunning_letters: "e65a8857631082094370117507ff0639d4c12a9819f19a510c1f9d152590c31d",
  accounting_dunning_runs: "f8b7492ce9da3b0edda0bcf6543c984fc62cf75bf54bfd6c573652f2b58f6ec8",
  accounting_invoice_approvals: "a1c36be266686fa71031af4208883f99e11764ae3326b5426b9d47f76d9d953d",
  accounting_invoice_attachments: "ef0d908b819f26e471f55e084d0033968f7b418b15053c816613cdcc3a3fd9c6",
  accounting_invoice_lines: "723f7c87a10efb70775ac6cf897b34d7ed62c508e719cb38ac6891692d1d50cb",
  accounting_invoices: "c17ee2b8f1f3095d4b083373fff82b6219c19eb60503c3be44764baf215ad704",
  accounting_journal_entries: "d19825f6d3426fd1d2c15f3a2aeae033d2e96aad1b2bf06e49d896b4c3d26b73",
  accounting_journal_entry_lines: "6ba7ca11b7f81f044ddd5886758a3df79323e0b1778bf2d3048528fead22d91d",
  accounting_ledger_entries: "58d17a1089b6591e0a78e3f9f1a093815fa4742f73347bb8c404a099dc9c4461",
  accounting_number_series: "5fe7fda57fb6df74e86432afebd2672ec233140d2de7181a62889d82f8e08bce",
  accounting_payment_allocations: "fadad3a78419a2e7e43100f45539ce53343f13eda6a79ccff515b782a7b24ebc",
  accounting_payment_terms: "f3dd933e24d4309281b12cf3ee6bceb79110b02074583846d965a9869d8505a5",
  accounting_payments: "ad499684f4082c37411936777b743f94873f11fdb42e256ce78f065d3a87a518",
  accounting_receipts: "f0995e7532ead4def819aa6bc554d53c4abbaccc8bee0db89272c0a11a5e43a0",
  accounting_recurring_invoices: "924982a4dc4773c2fdc01bf5a05a29ee26fd3e5708f69a6ad061bef0dadcc67a",
  applications: "ef7b52e117777575029346888b1c17412e2f4bda0ef9ca6c5a5e89c54a310e00",
  appsec_approvals: "ab75d7da2d137f7a328e99735d3fc0350be18d7cbbffccc05cd84dccca5feaf3",
  appsec_artifacts: "96e48a95be0e013773561dd52b5be8170d13aa0409488d5569e7060654eb22e1",
  appsec_assessments: "d3de37068175a449d2a17e5f63aa296265faf050e03900781c8cfaf8e4bf3e3f",
  appsec_coverage: "6fb9caf08dcf52ab1e7a669950bdb62159aec0b37112fb89aed0f93de475dde1",
  appsec_findings: "1c16a6500dc1dfbb3401ca83208338f2d8371c5abe88e8f5dfd21d4b3a73fbec",
  appsec_investigations: "fd4aea13736e18cc279d7829417cffe44b5068dbc99aaca8743a64d9598ec12e",
  appsec_pipeline_stages: "fd99085c9659da98841542afe627873bbfe19a892d0af52092686d5fd81c5d87",
  appsec_runs: "815d0931042bedab1356e27a22b3172b8e62e6c3eff983e7e2f74be89f30b326",
  appsec_scanner_inventory: "529588bf3120a75dbf86d7f8663eb5e088cc19e13477042ae5338fa0bb079bb3",
  browser_frames: "9cf5482db6b78cdf4aec40ec1cb308c1a2c1a5e31ae2de191c63e9ae34f31ba4",
  browser_input_events: "7435ba467228c2be5b43837e0644c2ebbb5c748ecc15098e508144a82605b0d9",
  browser_sessions: "67fa8ec7abfbdd8651ad73042909ca417b1902a7c6d59a94d4f98f4d50392f42",
  browser_tabs: "d06a73ca6896eeda3cf494616118b0bd4d7ca2f5b31315fc7f668cbb66fe8187",
  business_chats: "0e52de33b4ea565122debb0e46296b44cdbe13f60190b9d9d06259f3719918d7",
  business_commands: "83f3dc7b9078ae89640b9ffcc33bf29e1c74ec40df7cf63af9889b5bc0e4d238",
  business_consents: "4e0031090f60e466e8d9b2818a73faac41d89adabba5c2f2fd75a4b48cef9d68",
  business_credentials: "5583908188482df5c694d6214ef4f3a250fdcd09d7111a5a859a5976f4a40b7d",
  business_module_acl: "7f2c6c44ffadefb0c9be30dba9f3067fc48e0847424e3f2709638c5ebcd8bedf",
  business_module_catalog: "332763869d93c2bb55fa6b217c36521d1c1f17be4701d8538d686cda89f5cea0",
  business_module_releases: "8d9ff79eec5eccc04353a885002a8982deb169dbbf3a348998b88fafb7e219f7",
  business_module_reports: "440b04e33e1040e556c62741d7c4289422b6d0d01203c74e5aee391d5f050ed1",
  business_module_source_files: "fa9cdeda3530f04bd84b926cb8ffae650c8f5886efac079daee0d01315737551",
  business_users: "e735ef56eaa56c6661ac40b2549d4172a4654fb9d7f3aadc638b3a99bc71293f",
  business_workspace_branding: "a53d4f3e84454928bfeb239c22820b305cec6c657bb6d7340f68594f20baae22",
  calendar_availability_rules: "be220a21b86c15d22627f8685e0a90849a485baaa2b07b7133364107ffde661a",
  calendar_booking_holds: "1add78cb8f30596fd320eafcd798f54e06cd09451097398d87f56b6dcea6bde4",
  calendar_booking_pages: "cab127f6ea856a86f4b9f9e6b241f14490107ebea9673a99e1049cac2fcb9290",
  calendar_bookings: "65d02d6737bb81b4065137ac98d9514b1bc2fff83737f8daf969fab61d94e1f5",
  calendar_calendars: "5aecddfa2b457b45137356e178da572fb38488435ae050d8ecc11413c007bb82",
  calendar_event_instances: "664cae43311b56a13ad6f0fe1aa405bc6829dd179f33502a05ef15ea9d62e86f",
  calendar_events: "6baa83e7381279ac9f9963c907605d301e4686126204ebda54e80b0420bc0d45",
  calendar_sources: "db772cfda4ba2e0bc52b86899d155ea06e15a5811f370714206e5f3aff555d38",
  channel_pairing_state: "d93ceef99b772bc57939143bc6ef0044bf816801700d2dbc8f88def356aa246a",
  coding_agent_events: "784ad1849525cceb945125e8383a2994c33e802ee3bef4591e2ea331c63e3781",
  coding_agent_sessions: "55eebef3dd6ec5bc0650ae37280552ad81b4185d4f0acca5766fc3d0da4da090",
  coding_agent_workspace_grants: "e377e22121858098b74d6c29de67c19345c6c9e27eae46900434bb292cdb09e0",
  communication_accounts: "d40ca549e2f112071b6eb39bf0999a743643073279af4471a477cef259275653",
  communication_messages: "10d120234ec23bbe98124d255599f44d2ef68ecb5ff29787b9b647aaf6537b6f",
  communication_threads: "2111d907ee8cc8c7c2c4e9f10a43bc56f217071dbee0610a96b0457ef6473a8d",
  ctox_bug_reports: "f7329368ad5144b8ea740600265f06c6ac19ad049de751cec92818d9d9de94b5",
  ctox_queue_tasks: "00600391951e915fdf962bd89edaeabe24ed3475fed178f23eb37cba5e06faff",
  ctox_runs: "73df37bddc2e511b0567496f6199089aef436dd598a3e0bf85f462d38b4f3fff",
  ctox_runtime_settings: "3958bb6580e9705f3688fcf453a80ec33c486b43ac6988f015ffc16cb5ac918d",
  ctox_task_approval_requests: "5bbda4583cadd08e30c5948d2ed197cbf4a1f8f342580c1e531fd2a054da84fe",
  ctox_ticket_approvals: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_cases: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_clarification_requests: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_control_bundles: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_event_routing_state: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_events: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_items: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_label_assignments: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_self_work_items: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
  ctox_ticket_self_work_notes: "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
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
  desktop_file_chunks: "f3d9e6f8278f3140993109de6e34617376a4cd2b850e59dc066c0af066d6cc63",
  desktop_files: "5c8ea6eddecd37233ef1b99ad10280afe9ae5654bc77819d85d56236257be627",
  desktop_icons: "b3fc7cde6c2df59469255353b9ce91e5213ad091b86e8b3f2372e63db8c5ecd9",
  desktop_layout: "d741aa98029c7e0c38fb2ef53e32319ee4c7891b808c875802c540d60bdf5c3c",
  desktop_notifications: "5c312d2c291bf2b36fdbda8aacc1b2de7873c6ee7058c9960897bbb5b0797d0d",
  desktop_windows: "bcd10d8462083460b5025160f88f0abe6c7118d583aa4d1fd97433942617627f",
  document_blob_chunks: "9b4e27b2f795c697b67747b55e388b8d42afb3d5b8f66e6f9ec36f9564028b16",
  document_runbooks: "50b126b168c2fbf148da6b8693bbf455f6124c1b798a19e48aaaf5174acc9b7b",
  document_versions: "fca6df9bfa1d0d27f93d41cb7685fd08dacbf9f4843b7c1d95142b4cbe157738",
  documents: "600e0a73160dfaa480dd0ff8b833c85cec8aa60d41a9982a1ecd971e8a291ec1",
  interview_meetings: "e3e829b1b8b8ab4e5c2f9e3e7af82061125ffadc33cf484858dace36b5e2c62a",
  interview_scorecards: "126212aa5d37811134a96ca150fd07304eca1242d8de13d534458b9a48bf827a",
  iot_agent_status: "c719592fcc4274060d12567b09013cff8dc11b605b790b349e8efac88cfb6ccd",
  iot_agents: "0bf0fed6ea33be5d475e88b7b913fb1675bb1bf5d4361cc3c5eb6befec6480f8",
  iot_alarms: "978c527550ceb781393bba6e9e886714f7c66f60bc2f7b98be55896bb2ccb149",
  iot_asset_types: "5aebcc5fb39fe783d5364ce21c6f50dc929935ad1cef4964ad1ae996221064d3",
  iot_assets: "b56ee809bbf974a07d1a6423753bedc195e49f7ea4a9f0f4077afa54486ff93e",
  iot_attributes: "35a1c2494238fffedd2b6006ff5269bc7183a5ac60e2cd4a4c12ed17a9acabcb",
  iot_dashboards: "29a0875c3214b50bce0198608dd4a44e969b51900ea0b76128a53a4fffd25d49",
  iot_datapoints: "6313f3c8671e3406d789877aca842f8bf5b6a7fa2b63a8458dece314a2f55a80",
  iot_realms: "42ff4cfc74268c51602dd3873df95127f9070068aa5d7c1994e80f5275f78ada",
  iot_rulesets: "0232a7ef9501f87ff583848bf29489aff7105d79ea7a1740dbfc357476f799f1",
  iot_widgets: "acd9a9b1bdaabe7118403bd998190ac785cbf3133c2352386f14f1a4579eb66e",
  knowledge_items: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  knowledge_runbooks: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  knowledge_tables: "33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300",
  matching_objects: "31ff9b1fce039239cf0684e1cf246b9e5d3a222abd8ca4b0c9f3c837dfeb55e1",
  matching_requirements: "7a57a57784d58c9898d135a519a8789380742cb5a0de055f19e8f6a279035b50",
  matching_results: "a5260077a1b4e9d5881ff3b265daf8651b8c6be3158cb5eff0d4f78bed21137c",
  notes: "9c02d9c9f4362f7cb9739b5b401eb59528254534fdfd807050a941041304854d",
  offers: "ee230a74c678a29a209f08b48a98e2bb7d2ddca64c606f89f4cb2af3de7382d3",
  outbound_account_limits: "35d7a40e3e485447e234f72ec898ce57b7f2b7ebc4f01bb748a7e9ea5a3fc68e",
  outbound_approvals: "f7be2c8526ffc3df85e92a56c8e808adebbcd8944be95bd05658bc6f9d7b143a",
  outbound_campaigns: "194e3748c589a9cfc50ed63dccab525028e9bdbd006f20b73c10e29aa865e58d",
  outbound_companies: "1d79eb4b67d84826ed2016b0385224600d51c334d5b91d4adb77e62e916d0bbf",
  outbound_engagements: "f310db7ac3c7abdc78b40b227866ce673f5871601d594b00853000f7c4e088c2",
  outbound_letter_templates: "9839d58ede05148b48b2a7e494fc29d4aa94611034a11bc4c73b32de866a7466",
  outbound_meeting_requests: "f04c3249c3a3d8cf7ca6c2a4b51fbb15729035bca707668fbef3988242e69aa2",
  outbound_messages: "93b8e2cea0670112b6499a86a774dafef3cbd289d11725bf57d4e0941ad13006",
  outbound_pipeline_items: "d128a88597977a96b0b2572c0eaeb7c2e5da7d21ae691ff0b0a18e4824fd378c",
  outbound_research_adapters: "97ca18afb680d7103173ab5ac08178644998b9c8681ed1a9cc3738736b4c59e1",
  outbound_research_runs: "46573b72d1bd75daf105265b179af2b0b5d9fae5a61e15cf1198e0dc2604a372",
  outbound_sender_assignments: "d57aeee6946976bd082044147591d648583a6493c6c1c320359b0949c3405c78",
  outbound_sequences: "9368f8c42dc026c94549485d230d01ea511358313b64de0100b5f7706bae251b",
  outbound_skillbooks: "a896fd1593614940aa223831a949fbda53e8714c9b5086a4f1949db1ace83c35",
  outbound_sources: "241a2673630fb51c06a4e3155465855f299cb56ceeb8ce09ab1ba0d4c460c29a",
  outbound_suppression_entries: "2a894fbfc598d41b81ad7c76466e531d6771c7a9f6e5aa34389dba0e5f2cb329",
  placements: "638132ba63acb16782721b0d0b0469cf44de077f50e367d1287f5ef27e8a3df0",
  planning_absences: "20263440e5b0fa1d7a3a8c0d95f0753f6f5a30da517dcc208fafe5467ef1870b",
  planning_employees: "36852db8c0acb2b48b653592aeefa1af483843e22a2f400cf411178d7e8377c7",
  planning_projects: "fc558898d1dfe2d9f8cfb925b5fbd304133fcfad7b2e63069770d5f8325e9b6f",
  planning_shifts: "3e5a629a3dd83035c59f23ece1074478bc37afbdea14a7c02dc262cb47813804",
  planning_time_records: "2674badebb2a9b2133f5053b651ec7723b197869c6e32db59153cf0c227c4829",
  research_notes: "d078cd9b657f5eeb66281eb33e8b912c772fac447a5e60b580901fd4ef82c6dd",
  research_runs: "ba19ca3daec5cd92154b75faa056bbfab95383769dd69b77ce663656d18c282c",
  research_tasks: "502aa089a7498cf17db0bad1bba2d4bda864261b99488a07e783f6c107dc0dd0",
  signature_requests: "878b66f65173aa1e28f69866ef6f1562a1e564028604c8af68d67616618156bd",
  spreadsheet_blob_chunks: "dc97cfb4feca43442477d88da04528ecda56ab7cb52b38a19306270eddf26168",
  spreadsheet_runbooks: "08bf33d949370df78a4598cc97208212df6944c4feefe291787dad75e8b0d985",
  spreadsheet_versions: "5c569a9152b65e943b047a0419afea200a7c43e83e6c07eb0a0c667282e45842",
  spreadsheets: "1dfe54101a8efe6ad4d127bc9ac102c74d6b211cda716b1fa5411fc473c24367",
  submissions: "30b927ed9ba7168ef4c911db5450e862771dd00cf510eb75a867497010ce2c78",
  support_agent_requests: "a031d246fc60f1beaf6df2c94336f18ef5e341e176f4a57e06c3d0c410df6407",
  support_agent_suggestions: "bca59edd6f3d0ffb6e5518d7644f0f84919df4b7fab2d2155663658a9d1ed357",
  support_applied_slas: "d50e338b13e2fcb0af8a4547f6d1c30891ec6608954ed2bac60107eba7b8cefd",
  support_assignment_events: "fae28885fb0333ddc9439c894d38cdd9c58dfd67fda6eba60f8a76399ff13dc4",
  support_assignment_policies: "fba6729228ad5004a2d315cf66680d5bb189578903cd9ffdc69ae24b49cc4361",
  support_automation_rules: "fc18c94d6401eb07d96cf6c1f7b74fa1bb11c03341adbe94c6c41134d136529c",
  support_conversation_events: "351e19ffb42335f5de669d8a7f006fdae0ce710fdb9d97cca796d60fd534a1d0",
  support_conversations: "64aa01a2b74975dcfd468b70127ec0f8617347b539d9c201186f40fea40af625",
  support_identity_links: "a79004a14487ad13bf09813ab33d685cfafdd1b04afbfc15310c4467ae0eb42a",
  support_inboxes: "848bac741efde32519bba9f58007bde04f756e361eb2c79eab071c318418b073",
  support_label_assignments: "62bcae9d0547e22e9628814d15d73512b4c583dcefd93860d537a3c0762cc5ad",
  support_labels: "2fe1e3f3fe4c27240cfaf3fab66e331841e3a3aa80e03b1362b791342479113a",
  support_macros: "e60286afb22495b48efe1991d9efddbaa5244374f55c80f67d0b4efd43005b30",
  support_notes: "746ca7eaf4ce15fbc65d18cddaa9fe2fd6fdc4eb862e1d559877bda261638a2c",
  support_reporting_events: "7152cfb7d95ebec7c650911034afb0a398c9ce394fd0de6244ec142fc1fec431",
  support_reporting_rollups: "722f0ac0dcf53f3cf1465b8ef07e3471f4cbf33b746748511b407eb3acb81ade",
  support_sla_events: "f88c8c62da253ca76afddc746e38d8b7c3b8a0fe01f1d28058387431da1651dc",
  support_sla_policies: "c8f69d71947f117259d132c02e7b513d20a9467d400c2c08b573618177724953",
  support_thread_links: "c144074785a1e22697f7f2ebc30b297d404fd3ff2bfca797b78371e9f205a8be",
  support_view_filters: "e8988877eef64c437758f90f5d6868d8310122bb5f78e854fad31d256d3cafe5",
  support_views: "10ac9212258aef30b798d1d4e6d58712b9f59ee725966a8c7bd0fa49f72c1033",
  user_notifications: "28593fbad81de44fc2218886d67284cc140ca4b657bf75267412859a32753e5b",
  user_thread_links: "cc911076015a884b58fda2b28b5e8d840b048e78d958081429db31d573916129",
  user_thread_messages: "3e9ac54c218496245fdeaa9e8cd6f2f649455448703bada2ac290a1de4fd7646",
  user_thread_states: "71e70b8a2e44bd2b851b24fde40a5b4cd42cd9e0b6158525055a9c04743de9eb",
  user_threads: "97a226600a64559f18c795e6a6c39b56e478d455bc5ce1485b714e1d13c2e5cb"
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
  normalized.version = Number.isFinite(normalized.version) ? normalized.version : 0;
  normalized.type = typeof normalized.type === "string" && normalized.type ? normalized.type : "object";
  normalized.properties = normalized.properties && typeof normalized.properties === "object" ? normalized.properties : {};
  normalized.required = Array.isArray(normalized.required) ? normalized.required.map(String) : [];
  normalized.indexes = Array.isArray(normalized.indexes) ? normalized.indexes.map(normalizeSchemaIndex) : [];
  normalized.encrypted = Array.isArray(normalized.encrypted) ? normalized.encrypted.map(String) : [];
  normalized.keyCompression = normalized.keyCompression === true;
  normalized.additionalProperties = false;
  normalized.properties._rev = { type: "string", minLength: 1 };
  normalized.properties._attachments = { type: "object" };
  normalized.properties._deleted = { type: "boolean" };
  normalized.properties._meta = rxMetaSchema();
  for (const field of ["_deleted", "_rev", "_meta", "_attachments"]) {
    if (!normalized.required.includes(field)) normalized.required.push(field);
  }
  normalized.required.push(...finalSchemaFields(normalized));
  const requiredSeen = /* @__PURE__ */ new Set();
  normalized.required = normalized.required.filter((field) => {
    if (field.includes(".") || requiredSeen.has(field)) return false;
    requiredSeen.add(field);
    return true;
  });
  const primaryPath = primaryFieldOfPrimaryKey(normalized.primaryKey);
  const indexes = normalized.indexes.map((index) => {
    const next = index.slice();
    if (!next.includes(primaryPath)) next.push(primaryPath);
    if (next[0] !== "_deleted") next.unshift("_deleted");
    return next;
  });
  if (indexes.length === 0) indexes.push(["_deleted", primaryPath]);
  indexes.push(["_meta.lwt", primaryPath]);
  if (Array.isArray(normalized.internalIndexes)) {
    for (const index of normalized.internalIndexes) indexes.push(normalizeSchemaIndex(index));
  }
  const indexSeen = /* @__PURE__ */ new Set();
  normalized.indexes = indexes.filter((index) => {
    const key = index.join(",");
    if (indexSeen.has(key)) return false;
    indexSeen.add(key);
    return true;
  });
  return normalized;
}
function primaryFieldOfPrimaryKey(primaryKey) {
  if (typeof primaryKey === "string" && primaryKey) return primaryKey;
  if (primaryKey && typeof primaryKey === "object" && typeof primaryKey.key === "string" && primaryKey.key) {
    return primaryKey.key;
  }
  return "id";
}
function normalizeSchemaIndex(index) {
  if (Array.isArray(index)) return index.map(String);
  return [String(index)];
}
function finalSchemaFields(schema) {
  const fields = [];
  for (const [name, property] of Object.entries(schema.properties || {})) {
    if (property && typeof property === "object" && property.final === true) fields.push(name);
  }
  fields.push(primaryFieldOfPrimaryKey(schema.primaryKey));
  if (schema.primaryKey && typeof schema.primaryKey === "object" && Array.isArray(schema.primaryKey.fields)) {
    for (const field of schema.primaryKey.fields) fields.push(String(field));
  }
  return fields;
}
function rxMetaSchema() {
  return {
    type: "object",
    properties: {
      lwt: {
        type: "number",
        minimum: 1,
        maximum: 1e15,
        multipleOf: 0.01
      }
    },
    required: ["lwt"],
    additionalProperties: true
  };
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
  // #12c: the browser's CTOX capability token, so the native (master) peer can
  // bind this peer to its server-authenticated role and authorize per-collection
  // reads. Omitted when absent so the legacy handshake stays byte-identical.
  capabilityToken = null,
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
  collectionCheckpoints = null,
  storageGeneration = null,
  nativeTimeMs = null
} = {}) {
  const checkpointEvidence = checkpoint || null;
  const peerSession = {
    role,
    sessionId: peerSessionId || null,
    generation: Number.isFinite(peerGeneration) ? peerGeneration : null
  };
  const cleanCapabilityToken = typeof capabilityToken === "string" ? capabilityToken.trim() : "";
  if (cleanCapabilityToken) {
    peerSession.capabilityToken = cleanCapabilityToken;
  }
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
    storageGeneration: typeof storageGeneration === "string" && storageGeneration.trim() ? storageGeneration.trim() : null,
    nativeTimeMs: Number.isFinite(nativeTimeMs) ? Math.trunc(nativeTimeMs) : null,
    peerSession,
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

// src/apps/business-os/rxdb/src/conflict-merge.mjs
var SYSTEM_FIELD_PREFIX = "_";
function deepEqualJson(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== typeof b) return false;
  if (typeof a !== "object") return false;
  const aIsArray = Array.isArray(a);
  if (aIsArray !== Array.isArray(b)) return false;
  if (aIsArray) {
    if (a.length !== b.length) return false;
    for (let index = 0; index < a.length; index += 1) {
      if (!deepEqualJson(a[index], b[index])) return false;
    }
    return true;
  }
  const aKeys = Object.keys(a);
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) return false;
  for (const key of aKeys) {
    if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
    if (!deepEqualJson(a[key], b[key])) return false;
  }
  return true;
}
function businessFieldKeys(...docs) {
  const keys = /* @__PURE__ */ new Set();
  for (const doc of docs) {
    if (!doc || typeof doc !== "object") continue;
    for (const key of Object.keys(doc)) {
      if (!key.startsWith(SYSTEM_FIELD_PREFIX)) keys.add(key);
    }
  }
  return keys;
}
function threeWayMergeDocuments(base, local, master, { primaryPath = "id" } = {}) {
  const safeBase = base && typeof base === "object" ? base : {};
  const safeLocal = local && typeof local === "object" ? local : {};
  const safeMaster = master && typeof master === "object" ? master : {};
  if (safeMaster._deleted) {
    return { merged: safeMaster, identicalToMaster: true };
  }
  if (safeLocal._deleted) {
    return { merged: safeLocal, identicalToMaster: false };
  }
  const merged = {};
  for (const key of Object.keys(safeMaster)) {
    if (key.startsWith(SYSTEM_FIELD_PREFIX)) merged[key] = safeMaster[key];
  }
  merged[primaryPath] = safeMaster[primaryPath] ?? safeLocal[primaryPath];
  let localOnlyChange = false;
  const unsafeStructuredConflictFields = [];
  for (const key of businessFieldKeys(safeBase, safeLocal, safeMaster)) {
    if (key === primaryPath) continue;
    const baseValue = safeBase[key];
    const localValue = safeLocal[key];
    const masterValue = safeMaster[key];
    const localChanged = !deepEqualJson(localValue, baseValue);
    const masterChanged = !deepEqualJson(masterValue, baseValue);
    if (localChanged && masterChanged && !deepEqualJson(localValue, masterValue) && (isStructuredValue(localValue) || isStructuredValue(masterValue))) {
      unsafeStructuredConflictFields.push(key);
    }
    const winner = localChanged ? localValue : masterValue;
    if (localChanged && !deepEqualJson(localValue, masterValue)) {
      localOnlyChange = true;
    }
    if (winner !== void 0) {
      merged[key] = winner;
    }
  }
  return {
    merged,
    identicalToMaster: !localOnlyChange,
    requiresManualResolution: unsafeStructuredConflictFields.length > 0,
    conflictFields: unsafeStructuredConflictFields
  };
}
function isStructuredValue(value) {
  return value !== null && typeof value === "object";
}
function normalizeConflictStrategy(value) {
  return value === "field-merge" ? "field-merge" : "lww";
}
function normalizeDeleteStrategy(value) {
  return value === "final" ? "final" : "default";
}

// src/apps/business-os/rxdb/src/hybrid-logical-clock.mjs
var HLC_NODE_STORAGE_KEY = "ctox.businessOs.hlcNodeId.v1";
var cachedNodeId = null;
var nativeClockOffsetMs = 0;
var nativeClockObservedAtMs = null;
var clockSkewDetected = false;
var CLOCK_SKEW_LIMIT_MS = 5 * 60 * 1e3;
function setHybridLogicalClockTimeAnchor(nativeTimeMs, observedAtMs = Date.now()) {
  if (!Number.isFinite(nativeTimeMs) || !Number.isFinite(observedAtMs)) return hybridLogicalClockStatus();
  nativeClockOffsetMs = Math.trunc(nativeTimeMs) - Math.trunc(observedAtMs);
  nativeClockObservedAtMs = Math.trunc(observedAtMs);
  clockSkewDetected = Math.abs(nativeClockOffsetMs) > CLOCK_SKEW_LIMIT_MS;
  return hybridLogicalClockStatus();
}
function correctedHybridLogicalClockNowMs(nowMs = Date.now()) {
  return Math.max(0, Math.trunc(Number(nowMs) || 0) + nativeClockOffsetMs);
}
function hybridLogicalClockStatus() {
  return {
    code: clockSkewDetected ? "clock_skew_detected" : null,
    clockSkewDetected,
    nativeClockOffsetMs,
    nativeClockObservedAtMs
  };
}
function isFutureHybridLogicalClock(value, nowMs = correctedHybridLogicalClockNowMs()) {
  const parsed = parseHybridLogicalClock(value);
  return Boolean(parsed && parsed.physicalMs > nowMs + CLOCK_SKEW_LIMIT_MS);
}
function hybridLogicalClockNodeId() {
  if (cachedNodeId) return cachedNodeId;
  try {
    const stored = globalThis.localStorage?.getItem?.(HLC_NODE_STORAGE_KEY);
    if (stored) return cachedNodeId = sanitizeNodeId(stored);
  } catch {
  }
  const generated = sanitizeNodeId(
    globalThis.crypto?.randomUUID?.() || `browser-${Math.random().toString(36).slice(2, 14)}`
  );
  cachedNodeId = generated;
  try {
    globalThis.localStorage?.setItem?.(HLC_NODE_STORAGE_KEY, generated);
  } catch {
  }
  return generated;
}
function nextHybridLogicalClock(previous, {
  nowMs = null,
  nodeId = hybridLogicalClockNodeId()
} = {}) {
  const prior = parseHybridLogicalClock(previous);
  const wall = nowMs === null || nowMs === void 0 ? correctedHybridLogicalClockNowMs() : Math.max(0, Math.floor(Number(nowMs) || 0));
  const physicalMs = Math.max(wall, prior?.physicalMs || 0);
  const logical = prior && physicalMs === prior.physicalMs ? prior.logical + 1 : 0;
  return formatHybridLogicalClock({ physicalMs, logical, nodeId });
}
function compareHybridLogicalClocks(left, right) {
  const a = parseHybridLogicalClock(left);
  const b = parseHybridLogicalClock(right);
  if (!a && !b) return 0;
  if (!a) return -1;
  if (!b) return 1;
  if (a.physicalMs !== b.physicalMs) return a.physicalMs < b.physicalMs ? -1 : 1;
  if (a.logical !== b.logical) return a.logical < b.logical ? -1 : 1;
  return a.nodeId.localeCompare(b.nodeId);
}
function parseHybridLogicalClock(value) {
  const match = /^([0-9a-z]+):([0-9a-z]+):([0-9a-z_-]+)$/i.exec(String(value || ""));
  if (!match) return null;
  const physicalMs = Number.parseInt(match[1], 36);
  const logical = Number.parseInt(match[2], 36);
  if (!Number.isSafeInteger(physicalMs) || !Number.isSafeInteger(logical)) return null;
  return { physicalMs, logical, nodeId: sanitizeNodeId(match[3]) };
}
function formatHybridLogicalClock({ physicalMs, logical = 0, nodeId = "native" }) {
  return `${Math.max(0, Math.floor(physicalMs)).toString(36)}:${Math.max(0, Math.floor(logical)).toString(36)}:${sanitizeNodeId(nodeId)}`;
}
function sanitizeNodeId(value) {
  return String(value || "unknown").toLowerCase().replace(/[^0-9a-z_-]/g, "").slice(0, 48) || "unknown";
}

// src/apps/business-os/rxdb/src/recovery-crypto.mjs
var RECOVERY_EXPORT_SCHEMA = "ctox.browser-recovery.v2";
var RECOVERY_CRYPTO_SCHEMA = "ctox.browser-recovery.crypto.v1";
var PBKDF2_ITERATIONS = 6e5;
async function encryptRecoveryArtifact(value, passphrase) {
  requirePassphrase(passphrase);
  const subtle = requireSubtle();
  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = await deriveRecoveryKey(subtle, passphrase, salt, ["encrypt"]);
  const plaintext = new TextEncoder().encode(JSON.stringify(value));
  const ciphertext = await subtle.encrypt({ name: "AES-GCM", iv }, key, plaintext);
  return {
    schema: RECOVERY_CRYPTO_SCHEMA,
    contentSchema: RECOVERY_EXPORT_SCHEMA,
    kdf: { name: "PBKDF2", hash: "SHA-256", iterations: PBKDF2_ITERATIONS, saltBase64: bytesToBase64(salt) },
    cipher: { name: "AES-GCM", ivBase64: bytesToBase64(iv) },
    ciphertextBase64: bytesToBase64(new Uint8Array(ciphertext))
  };
}
async function decryptRecoveryArtifact(envelope, passphrase) {
  requirePassphrase(passphrase);
  if (envelope?.schema !== RECOVERY_CRYPTO_SCHEMA) {
    throw recoveryCryptoError("recovery_integrity_failed", "Unsupported recovery encryption envelope.");
  }
  try {
    const subtle = requireSubtle();
    const salt = base64ToBytes(envelope.kdf?.saltBase64 || "");
    const iv = base64ToBytes(envelope.cipher?.ivBase64 || "");
    const ciphertext = base64ToBytes(envelope.ciphertextBase64 || "");
    const iterations = Number(envelope.kdf?.iterations || 0);
    if (envelope.kdf?.name !== "PBKDF2" || envelope.kdf?.hash !== "SHA-256" || envelope.cipher?.name !== "AES-GCM" || iterations < 1e5 || salt.byteLength !== 16 || iv.byteLength !== 12 || ciphertext.byteLength === 0) {
      throw new Error("invalid recovery encryption parameters");
    }
    const key = await deriveRecoveryKey(subtle, passphrase, salt, ["decrypt"], iterations);
    const plaintext = await subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext);
    const parsed = JSON.parse(new TextDecoder().decode(plaintext));
    if (parsed?.schema !== RECOVERY_EXPORT_SCHEMA) throw new Error("unexpected recovery content schema");
    return parsed;
  } catch (cause) {
    throw recoveryCryptoError("recovery_integrity_failed", "Recovery export could not be decrypted or failed integrity validation.", cause);
  }
}
async function sha256Json(value) {
  const bytes = new TextEncoder().encode(canonicalJson2(value));
  const digest = await requireSubtle().digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
function canonicalJson2(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson2).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson2(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
async function deriveRecoveryKey(subtle, passphrase, salt, usages, iterations = PBKDF2_ITERATIONS) {
  const base = await subtle.importKey(
    "raw",
    new TextEncoder().encode(String(passphrase)),
    "PBKDF2",
    false,
    ["deriveKey"]
  );
  return subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations },
    base,
    { name: "AES-GCM", length: 256 },
    false,
    usages
  );
}
function requireSubtle() {
  if (!globalThis.crypto?.subtle) throw recoveryCryptoError("recovery_integrity_failed", "WebCrypto is required for recovery export encryption.");
  return globalThis.crypto.subtle;
}
function requirePassphrase(passphrase) {
  if (String(passphrase || "").length < 8) {
    throw recoveryCryptoError("recovery_integrity_failed", "Recovery export passphrase must contain at least eight characters.");
  }
}
function randomBytes(length) {
  const bytes = new Uint8Array(length);
  globalThis.crypto.getRandomValues(bytes);
  return bytes;
}
function bytesToBase64(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return globalThis.btoa(binary);
}
function base64ToBytes(value) {
  const binary = globalThis.atob(String(value || ""));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
function recoveryCryptoError(code, message, cause = null) {
  const error = new Error(message, cause ? { cause } : void 0);
  error.code = code;
  error.retryable = false;
  return error;
}
var recoveryCryptoTestInternals = Object.freeze({
  RECOVERY_EXPORT_SCHEMA,
  RECOVERY_CRYPTO_SCHEMA,
  PBKDF2_ITERATIONS,
  canonicalJson: canonicalJson2
});

// src/apps/business-os/rxdb/src/recovery-journal.mjs
var JOURNAL_VERSION = 3;
var BATCH_STORE = "batches";
var BATCH_STATE_COLLECTION_INDEX = "stateCollection";
var CONFLICT_STORE = "conflicts";
var META_STORE = "meta";
var ACKED_RETENTION_MS = 24 * 60 * 60 * 1e3;
var previews = /* @__PURE__ */ new Map();
async function openRecoveryJournal({ databaseName, instanceId = databaseName, quotaCoordinator = null } = {}) {
  if (!databaseName) throw new TypeError("Recovery journal requires databaseName");
  const db = await openJournalDatabase(`${databaseName}__recovery_v2`);
  return new CtoxRecoveryJournal(db, { databaseName, instanceId, quotaCoordinator });
}
var CtoxRecoveryJournal = class {
  constructor(db, { databaseName, instanceId, quotaCoordinator }) {
    this.db = db;
    this.databaseName = databaseName;
    this.instanceId = instanceId;
    this.quotaCoordinator = quotaCoordinator;
    this.replayers = /* @__PURE__ */ new Map();
  }
  registerCollection(collection, { schemaHash: schemaHash2 = "", applyBatch, resolveConflict = null, applyMaster = null } = {}) {
    if (!collection || typeof applyBatch !== "function") return;
    this.replayers.set(collection, { schemaHash: schemaHash2, applyBatch, resolveConflict, applyMaster });
  }
  async appendBatch({ collection, schemaHash: schemaHash2 = "", primaryPath = "id", operation = "write", rows = [], baseById = null }) {
    const normalizedRows = structuredCloneSafe2(rows);
    const batch = {
      batchId: globalThis.crypto?.randomUUID?.() || `batch-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sequence: 0,
      schema: "ctox.indexeddb.recovery-journal.v2",
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      collection,
      schemaHash: schemaHash2,
      primaryPath,
      operation,
      rows: normalizedRows,
      baseById: structuredCloneSafe2(baseById),
      documentIds: normalizedRows.map((row) => documentId(row?.document || row, primaryPath)).filter(Boolean),
      committedDocs: {},
      ackedIds: [],
      payloadHash: await sha256Json({ collection, schemaHash: schemaHash2, primaryPath, operation, rows: normalizedRows, baseById }),
      state: "pending",
      createdAtMs: Date.now(),
      primaryCommittedAtMs: 0,
      masterAckedAtMs: 0
    };
    await this.withQuotaRecovery(() => putSequencedBatch(this.db, batch));
    await this.publishStatus();
    return batch.batchId;
  }
  async commitBatch(batchId, success = {}) {
    await updateRecord(this.db, BATCH_STORE, batchId, (batch) => ({
      ...batch,
      committedDocs: structuredCloneSafe2(success),
      primaryCommittedAtMs: Date.now()
    }));
    await this.publishStatus();
  }
  async markMasterAcknowledged(collection, documents = {}) {
    const batches = await this.listBatches("pending", collection);
    for (const batch of batches) {
      if (batch.collection !== collection) continue;
      const acked = new Set(batch.ackedIds || []);
      for (const id of batch.documentIds || []) {
        const master = documents[id];
        const local = batch.committedDocs?.[id];
        if (master && local && masterAcknowledgesLocal(master, local, collection)) acked.add(id);
      }
      const complete = (batch.documentIds || []).every((id) => acked.has(id));
      await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
        ...current,
        ackedIds: [...acked],
        state: complete ? "master_acked" : "pending",
        masterAckedAtMs: complete ? Date.now() : 0
      }));
    }
    await this.gc();
    await this.publishStatus();
  }
  // SYNC-40: force-acknowledge local writes the native peer terminally REJECTED
  // (authz/schema), not ones it accepted. `markMasterAcknowledged` only clears a
  // WAL entry when the master row acknowledges the local content — a denied
  // write never round-trips, so without this its batch stays pending and
  // replays as a fresh pushable write on every restart (re-push → re-deny →
  // re-journal). The rejected version is preserved in the conflict store; here
  // we drop it from the pending write-ahead log so it stops being re-pushed.
  async markReconciled(collection, ids = []) {
    const idSet = new Set((Array.isArray(ids) ? ids : []).map((id) => String(id)));
    if (!idSet.size) return;
    const batches = await this.listBatches("pending", collection);
    for (const batch of batches) {
      if (batch.collection !== collection) continue;
      const relevant = (batch.documentIds || []).filter((id) => idSet.has(String(id)));
      if (!relevant.length) continue;
      const acked = new Set(batch.ackedIds || []);
      for (const id of relevant) acked.add(id);
      const complete = (batch.documentIds || []).every((id) => acked.has(id));
      await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
        ...current,
        ackedIds: [...acked],
        state: complete ? "master_acked" : "pending",
        masterAckedAtMs: complete ? Date.now() : current.masterAckedAtMs || 0
      }));
    }
    await this.gc();
    await this.publishStatus();
  }
  async replayRegisteredCollections(collection = null) {
    const batches = await this.listBatches("pending", collection);
    const outcomes = [];
    for (const batch of batches) {
      if (Number(batch.primaryCommittedAtMs || 0) > 0) continue;
      const replayer = this.replayers.get(batch.collection);
      if (!replayer) continue;
      if (batch.schemaHash && replayer.schemaHash && batch.schemaHash !== replayer.schemaHash) {
        await this.recordConflict({
          code: "recovery_schema_mismatch",
          collection: batch.collection,
          batchId: batch.batchId,
          base: batch.baseById,
          local: batch.rows,
          master: null
        });
        await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
          ...current,
          state: "conflict",
          conflictAtMs: Date.now()
        }));
        outcomes.push({ batchId: batch.batchId, status: "conflict" });
        continue;
      }
      try {
        const result = await replayer.applyBatch(batch);
        await this.commitBatch(batch.batchId, result?.success || {});
        outcomes.push({ batchId: batch.batchId, status: "replayed" });
      } catch (error) {
        await this.recordConflict({
          code: error?.code || "recovery_replay_failed",
          collection: batch.collection,
          batchId: batch.batchId,
          base: batch.baseById,
          local: batch.rows,
          master: null,
          message: error?.message || String(error)
        });
        await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
          ...current,
          state: "conflict",
          conflictAtMs: Date.now()
        }));
        outcomes.push({ batchId: batch.batchId, status: "conflict" });
      }
    }
    await this.publishStatus();
    return outcomes;
  }
  async recordConflict(conflict = {}) {
    const record = {
      conflictId: conflict.conflictId || globalThis.crypto?.randomUUID?.() || `conflict-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      schema: "ctox.indexeddb.recovery-conflict.v1",
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      state: "pending",
      createdAtMs: Date.now(),
      ...structuredCloneSafe2(conflict)
    };
    await this.withQuotaRecovery(() => putRecord(this.db, CONFLICT_STORE, record));
    await this.publishStatus();
    return record;
  }
  async listConflicts() {
    return getAllRecords(this.db, CONFLICT_STORE).then((rows) => rows.filter((row) => row.state === "pending"));
  }
  async resolveConflict(conflictId, resolution) {
    if (!["keep_local", "keep_master", "restore_as_copy"].includes(resolution)) {
      throw recoveryError("structured_conflict_requires_resolution", `Unsupported conflict resolution ${resolution}`);
    }
    const conflict = await getRecord(this.db, CONFLICT_STORE, conflictId);
    if (!conflict) return false;
    if (conflict.conflictType === "delete_vs_update" && resolution === "keep_local") {
      throw recoveryError(
        "structured_conflict_requires_resolution",
        "A native tombstone is authoritative. Restore the local version as a copy instead."
      );
    }
    const replayer = this.replayers.get(conflict.collection);
    if ((resolution === "keep_local" || resolution === "restore_as_copy") && !replayer) {
      throw recoveryError("structured_conflict_requires_resolution", `Collection ${conflict.collection} is not registered.`);
    }
    if (resolution === "keep_local") {
      const rows = normalizeConflictRows(conflict.local);
      await (replayer.resolveConflict || replayer.applyBatch)({
        operation: "write",
        rows,
        baseById: conflictBaseById(conflict, rows)
      });
    } else if (resolution === "restore_as_copy") {
      const rows = normalizeConflictRows(conflict.local).map((row) => restoreAsCopy(row?.document || row));
      await (replayer.resolveConflict || replayer.applyBatch)({ operation: "write", rows, baseById: null });
    } else if (resolution === "keep_master") {
      const master = conflict.master;
      if (master && replayer?.applyMaster) {
        await replayer.applyMaster({
          operation: "write",
          rows: [{ document: structuredCloneSafe2(master) }],
          baseById: null
        });
      }
    }
    await updateRecord(this.db, CONFLICT_STORE, conflictId, (current) => ({
      ...current,
      state: "resolved",
      resolution,
      resolvedAtMs: Date.now()
    }));
    await this.publishStatus();
    return true;
  }
  async getStatus() {
    const batches = await this.listBatches("pending");
    const conflicts = await this.listConflicts();
    const bytes = estimateBytes(batches) + estimateBytes(conflicts);
    return {
      schema: "ctox.browser-recovery.status.v2",
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      pendingBatches: batches.length,
      pendingWrites: batches.reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0),
      pendingBytes: bytes,
      oldestPendingAtMs: batches.reduce((oldest, batch) => Math.min(oldest, batch.createdAtMs || oldest), Number.MAX_SAFE_INTEGER) === Number.MAX_SAFE_INTEGER ? 0 : batches.reduce((oldest, batch) => Math.min(oldest, batch.createdAtMs || oldest), Number.MAX_SAFE_INTEGER),
      unresolvedConflicts: conflicts.length,
      lastExportAtMs: Number((await getRecord(this.db, META_STORE, "lastExport"))?.value || 0),
      updatedAtMs: Date.now()
    };
  }
  async export(passphrase) {
    const pendingBatches = await this.listBatches("pending");
    const conflicts = await this.listConflicts();
    const content = {
      schema: "ctox.browser-recovery.v2",
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      createdAtMs: Date.now(),
      pendingBatches,
      conflicts
    };
    content.contentHash = await sha256Json(content);
    const encrypted = await encryptRecoveryArtifact(content, passphrase);
    const text = JSON.stringify(encrypted, null, 2);
    await putRecord(this.db, META_STORE, { key: "lastExport", value: Date.now() });
    await this.publishStatus();
    return {
      filename: `ctox-recovery-${this.instanceId}-${(/* @__PURE__ */ new Date()).toISOString().replace(/[:.]/g, "-")}.ctox-recovery`,
      blob: new Blob([text], { type: "application/vnd.ctox.recovery+json" }),
      pendingWrites: content.pendingBatches.reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0)
    };
  }
  async previewImport(file, passphrase) {
    const text = typeof file === "string" ? file : await file.text();
    const content = await decryptRecoveryArtifact(JSON.parse(text), passphrase);
    const expectedHash = content.contentHash;
    const hashInput = { ...content };
    delete hashInput.contentHash;
    if (!expectedHash || await sha256Json(hashInput) !== expectedHash) {
      throw recoveryError("recovery_integrity_failed", "Recovery content hash does not match.");
    }
    if (content.instanceId !== this.instanceId || content.databaseName !== this.databaseName) {
      throw recoveryError("recovery_instance_mismatch", "Recovery export belongs to a different CTOX instance or database.");
    }
    const previewId = globalThis.crypto?.randomUUID?.() || `preview-${Date.now()}`;
    const schemaMismatches = (content.pendingBatches || []).filter((batch) => {
      const replayer = this.replayers.get(batch.collection);
      return Boolean(batch.schemaHash && replayer?.schemaHash && batch.schemaHash !== replayer.schemaHash);
    }).map((batch) => ({
      batchId: batch.batchId,
      collection: batch.collection,
      artifactSchemaHash: batch.schemaHash,
      localSchemaHash: this.replayers.get(batch.collection)?.schemaHash || null
    }));
    previews.set(previewId, { journal: this, content, expiresAtMs: Date.now() + 10 * 60 * 1e3 });
    return {
      previewId,
      pendingBatches: content.pendingBatches?.length || 0,
      pendingWrites: (content.pendingBatches || []).reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0),
      conflicts: content.conflicts?.length || 0,
      schemaMismatches,
      createdAtMs: content.createdAtMs
    };
  }
  async applyImport(previewId) {
    const preview = previews.get(previewId);
    if (!preview || preview.journal !== this || preview.expiresAtMs < Date.now()) {
      throw recoveryError("recovery_integrity_failed", "Recovery import preview is missing or expired.");
    }
    previews.delete(previewId);
    for (const batch of preview.content.pendingBatches || []) {
      const existing = await getRecord(this.db, BATCH_STORE, batch.batchId);
      if (!existing) await putRecord(this.db, BATCH_STORE, { ...batch, state: "pending" });
    }
    for (const conflict of preview.content.conflicts || []) {
      const existing = await getRecord(this.db, CONFLICT_STORE, conflict.conflictId);
      if (!existing) await putRecord(this.db, CONFLICT_STORE, { ...conflict, state: "pending" });
    }
    const replay = await this.replayRegisteredCollections();
    await this.publishStatus();
    return { imported: true, replay };
  }
  async listBatches(state = null, collection = null) {
    const rows = (state && collection ? await getAllRecordsByIndex(this.db, BATCH_STORE, BATCH_STATE_COLLECTION_INDEX, [state, collection]) : await getAllRecords(this.db, BATCH_STORE)).sort((left, right) => Number(left.sequence || 0) - Number(right.sequence || 0));
    return rows.filter((row) => (!state || row.state === state) && (!collection || row.collection === collection));
  }
  async gc(now = Date.now()) {
    const rows = await this.listBatches("master_acked");
    for (const row of rows) {
      if (now - Number(row.masterAckedAtMs || 0) >= ACKED_RETENTION_MS) {
        await deleteRecord(this.db, BATCH_STORE, row.batchId);
      }
    }
    await this.gcConflicts(now);
  }
  // SYNC-53: resolved conflict records hold full local+master+base documents
  // (~3x a document each) and were never reclaimed — `resolveConflict` only
  // flips state to 'resolved'. Every collision event (update_vs_update,
  // delete_vs_update, clock_skew_detected) therefore grew IndexedDB forever
  // with zero live rows. Prune conflicts that are RESOLVED and older than the
  // same 24h retention window used for master-acked batches. PENDING /
  // unresolved conflicts are user-recoverable state and are never touched
  // here; neither are unsynced WRITE batches (handled by the master_acked
  // path above and protected by §9). Returns the number of records pruned.
  async gcConflicts(now = Date.now()) {
    const rows = await getAllRecords(this.db, CONFLICT_STORE);
    let pruned = 0;
    for (const row of rows) {
      if (row.state !== "resolved") continue;
      const resolvedAt = Number(row.resolvedAtMs || 0);
      if (!resolvedAt) {
        await updateRecord(this.db, CONFLICT_STORE, row.conflictId, (current) => ({
          ...current,
          resolvedAtMs: now
        }));
        continue;
      }
      if (now - resolvedAt >= ACKED_RETENTION_MS) {
        await deleteRecord(this.db, CONFLICT_STORE, row.conflictId);
        pruned += 1;
      }
    }
    return pruned;
  }
  async publishStatus() {
    try {
      const status = await this.getStatus();
      globalThis.dispatchEvent?.(new CustomEvent("ctox-indexeddb-recovery-status", { detail: status }));
      globalThis.localStorage?.setItem?.(
        `ctox.businessOs.recoveryStatus.${this.databaseName}`,
        JSON.stringify(status)
      );
    } catch {
    }
  }
  async withQuotaRecovery(operation) {
    try {
      return await operation();
    } catch (error) {
      if (!isQuotaExceeded(error) || !this.quotaCoordinator?.recover) {
        if (isQuotaExceeded(error)) throw recoveryError("indexeddb_journal_unavailable", "Recovery journal is out of storage space.", error);
        throw error;
      }
      await this.quotaCoordinator.recover({ source: "recovery-journal" });
      try {
        return await operation();
      } catch (retryError) {
        throw recoveryError("indexeddb_journal_unavailable", "Recovery journal could not commit after quota recovery.", retryError);
      }
    }
  }
  close() {
    this.db.close();
  }
};
function openJournalDatabase(name) {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(name, JOURNAL_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      const batches = db.objectStoreNames.contains(BATCH_STORE) ? request.transaction.objectStore(BATCH_STORE) : db.createObjectStore(BATCH_STORE, { keyPath: "batchId" });
      if (!batches.indexNames.contains(BATCH_STATE_COLLECTION_INDEX)) {
        batches.createIndex(BATCH_STATE_COLLECTION_INDEX, ["state", "collection"], { unique: false });
      }
      if (!db.objectStoreNames.contains(CONFLICT_STORE)) db.createObjectStore(CONFLICT_STORE, { keyPath: "conflictId" });
      if (!db.objectStoreNames.contains(META_STORE)) db.createObjectStore(META_STORE, { keyPath: "key" });
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => db.close();
      resolve(db);
    };
    request.onerror = () => reject(request.error || new Error(`Failed to open recovery journal ${name}`));
    request.onblocked = () => reject(recoveryError("indexeddb_journal_unavailable", `Recovery journal ${name} is blocked.`));
  });
}
function transact(db, storeName, mode, run) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(storeName, mode);
    const store = tx.objectStore(storeName);
    let result;
    try {
      Promise.resolve(run(store)).then((value) => {
        result = value;
      }, (error) => {
        try {
          tx.abort();
        } catch {
        }
        reject(error);
      });
    } catch (error) {
      try {
        tx.abort();
      } catch {
      }
      reject(error);
    }
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error || new Error(`IndexedDB ${storeName} transaction failed`));
    tx.onabort = () => reject(tx.error || new Error(`IndexedDB ${storeName} transaction aborted`));
  });
}
function requestResult(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error || new Error("IndexedDB request failed"));
  });
}
function putRecord(db, storeName, value) {
  return transact(db, storeName, "readwrite", (store) => requestResult(store.put(value)));
}
function putSequencedBatch(db, batch) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction([META_STORE, BATCH_STORE], "readwrite");
    const meta = tx.objectStore(META_STORE);
    const batches = tx.objectStore(BATCH_STORE);
    const request = meta.get("journalSequence");
    let sequence = 0;
    request.onsuccess = () => {
      sequence = Math.max(0, Number(request.result?.value || 0)) + 1;
      batch.sequence = sequence;
      meta.put({ key: "journalSequence", value: sequence });
      batches.put(batch);
    };
    request.onerror = () => {
      try {
        tx.abort();
      } catch {
      }
      reject(request.error || new Error("Failed to allocate recovery journal sequence"));
    };
    tx.oncomplete = () => resolve(sequence);
    tx.onerror = () => reject(tx.error || new Error("Recovery journal batch transaction failed"));
    tx.onabort = () => reject(tx.error || new Error("Recovery journal batch transaction aborted"));
  });
}
function getRecord(db, storeName, key) {
  return transact(db, storeName, "readonly", (store) => requestResult(store.get(key)));
}
function getAllRecords(db, storeName) {
  return transact(db, storeName, "readonly", (store) => requestResult(store.getAll()));
}
function getAllRecordsByIndex(db, storeName, indexName, key) {
  return transact(db, storeName, "readonly", (store) => requestResult(store.index(indexName).getAll(key)));
}
function deleteRecord(db, storeName, key) {
  return transact(db, storeName, "readwrite", (store) => requestResult(store.delete(key)));
}
async function updateRecord(db, storeName, key, update) {
  return transact(db, storeName, "readwrite", async (store) => {
    const current = await requestResult(store.get(key));
    if (!current) return false;
    await requestResult(store.put(update(current)));
    return true;
  });
}
function documentId(doc = {}, primaryPath = "id") {
  return String(valueAtPath(doc, primaryPath) || doc.id || doc._id || doc.key || doc.uuid || "");
}
function valueAtPath(value, path) {
  return String(path || "").split(".").filter(Boolean).reduce((current, segment) => current?.[segment], value);
}
function masterAcknowledgesLocal(master, local, collection = "") {
  const masterHlc = String(master?._meta?.ctoxHlc || "");
  const localHlc = String(local?._meta?.ctoxHlc || "");
  if (collection === "business_commands" && serverAcknowledgesCommand(master, local)) return true;
  if (masterHlc && localHlc) return masterHlc === localHlc;
  return comparableDocument(master) === comparableDocument(local);
}
function serverAcknowledgesCommand(master, local) {
  const terminalOrAccepted = /* @__PURE__ */ new Set([
    "accepted",
    "completed",
    "failed",
    "rejected",
    "cancelled",
    "canceled",
    "blocked"
  ]);
  if (!terminalOrAccepted.has(String(master?.status || "").toLowerCase())) return false;
  return String(master?.id || "") === String(local?.id || "") && String(master?.command_id || "") === String(local?.command_id || "") && String(master?.command_type || "") === String(local?.command_type || "") && String(master?.module || "") === String(local?.module || "") && isJsonSubset(local?.payload ?? null, master?.payload ?? null);
}
function isJsonSubset(expected, actual) {
  if (Object.is(expected, actual)) return true;
  if (Array.isArray(expected)) {
    return Array.isArray(actual) && expected.length === actual.length && expected.every((value, index) => isJsonSubset(value, actual[index]));
  }
  if (!expected || typeof expected !== "object" || !actual || typeof actual !== "object") return false;
  return Object.entries(expected).every(([key, value]) => Object.prototype.hasOwnProperty.call(actual, key) && isJsonSubset(value, actual[key]));
}
function comparableDocument(doc) {
  const copy = structuredCloneSafe2(doc) || {};
  if (copy._meta) delete copy._meta.ctoxReplicationOrigin;
  return JSON.stringify(copy);
}
function normalizeConflictRows(value) {
  return Array.isArray(value) ? value : value ? [value] : [];
}
function conflictBaseById(conflict, rows) {
  if (!conflict?.base) return null;
  const baseById = {};
  for (const row of rows) {
    const id = documentId(row?.document || row, conflict.primaryPath || "id");
    if (id) baseById[id] = structuredCloneSafe2(conflict.base);
  }
  return Object.keys(baseById).length ? baseById : null;
}
function restoreAsCopy(doc) {
  const copy = structuredCloneSafe2(doc) || {};
  const id = documentId(copy);
  copy.id = `${id || "recovered"}-recovered-${Date.now().toString(36)}`;
  delete copy._rev;
  if (copy._meta) delete copy._meta.ctoxReplicationOrigin;
  return copy;
}
function estimateBytes(value) {
  try {
    return new TextEncoder().encode(JSON.stringify(value)).byteLength;
  } catch {
    return 0;
  }
}
function structuredCloneSafe2(value) {
  if (value == null) return value;
  if (typeof structuredClone === "function") return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}
function isQuotaExceeded(error) {
  return error?.name === "QuotaExceededError" || String(error?.message || "").toLowerCase().includes("quota");
}
function recoveryError(code, message, cause = null) {
  const error = new Error(message, cause ? { cause } : void 0);
  error.code = code;
  error.retryable = code === "indexeddb_journal_unavailable";
  return error;
}
var recoveryJournalTestInternals = Object.freeze({
  BATCH_STORE,
  CONFLICT_STORE,
  META_STORE,
  ACKED_RETENTION_MS,
  masterAcknowledgesLocal
});

// src/apps/business-os/rxdb/src/query-meta-backend-memory.mjs
function createMemoryMetaBackend() {
  const queryWindows = /* @__PURE__ */ new Map();
  const queryWindowRefsByDocument = /* @__PURE__ */ new Map();
  const queryWindowRefsByWindow = /* @__PURE__ */ new Map();
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
      const normalizedKey = stringKey(key);
      queryWindows.delete(normalizedKey);
      deleteQueryWindowRefs2(normalizedKey);
    },
    async scanQueryWindows() {
      return Array.from(queryWindows.values(), (record) => ({ ...record }));
    },
    async replaceQueryWindowDocumentRefs(record) {
      const windowKey = queryWindowKey(record);
      deleteQueryWindowRefs2(windowKey);
      const documentKeys = /* @__PURE__ */ new Set();
      for (const id of normalizeDocumentIds([...record.documentIds || [], ...record.selectorRefIds || []])) {
        const documentKey = `${record.collection}|${id}`;
        documentKeys.add(documentKey);
        const refs = queryWindowRefsByDocument.get(documentKey) || /* @__PURE__ */ new Set();
        refs.add(windowKey);
        queryWindowRefsByDocument.set(documentKey, refs);
      }
      queryWindowRefsByWindow.set(windowKey, documentKeys);
    },
    async getQueryWindowKeysByDocumentIds(collection, ids) {
      const keys = /* @__PURE__ */ new Set();
      for (const id of normalizeDocumentIds(ids)) {
        const refs = queryWindowRefsByDocument.get(`${collection}|${id}`);
        if (!refs) continue;
        for (const key of refs) keys.add(key);
      }
      return Array.from(keys);
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
      queryWindowRefsByDocument.clear();
      queryWindowRefsByWindow.clear();
      documentAccess.clear();
      cacheStats.clear();
    },
    async close() {
    }
  };
  function deleteQueryWindowRefs2(windowKey) {
    const documentKeys = queryWindowRefsByWindow.get(windowKey);
    if (!documentKeys) return;
    for (const documentKey of documentKeys) {
      const refs = queryWindowRefsByDocument.get(documentKey);
      if (!refs) continue;
      refs.delete(windowKey);
      if (!refs.size) queryWindowRefsByDocument.delete(documentKey);
    }
    queryWindowRefsByWindow.delete(windowKey);
  }
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
function normalizeDocumentIds(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || "")).filter(Boolean)));
}

// src/apps/business-os/rxdb/src/query-meta-storage.mjs
var SIDECAR_DATABASE_NAME = "ctox_business_os_v1_5_meta";
var SIDECAR_PIN_RECENT_READ_TTL_MS = 6e4;
var PIN_RECENT_READ = "recently-read";
var evictionSchedulerGroups = /* @__PURE__ */ new Map();
var QueryMetaStorage = class {
  constructor(backend, {
    databaseName,
    schedulerKey = databaseName,
    clock = Date.now,
    primaryDelete = null
  } = {}) {
    if (!backend) throw new TypeError("QueryMetaStorage requires a backend");
    if (!databaseName) throw new TypeError("QueryMetaStorage requires a databaseName");
    this.backend = backend;
    this.databaseName = databaseName;
    this.schedulerKey = schedulerKey;
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
  async upsertQueryWindow({ collection, queryFingerprint: queryFingerprint2, offset, limit, documentIds, complete, authoritativeRevision, queryShape = null }) {
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
      // Sticky marker: once a window has been complete, its member documents
      // exist in the primary store (and replication keeps them fresh). The
      // demand loader serves such windows local-first while it revalidates
      // in the background, and the reconnect-abort path must NOT tombstone
      // their members as partial orphans.
      everCompleted: Boolean(complete) || Boolean(existing?.everCompleted),
      authoritativeRevision: authoritativeRevision ?? null,
      queryShape: queryShape && typeof queryShape === "object" ? structuredCloneSafe3(queryShape) : null,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
      lastAccessedAt: now
    };
    await this.backend.putQueryWindow(record);
    await this.backend.replaceQueryWindowDocumentRefs?.({
      ...record,
      selectorRefIds: computeSelectorRefIds(queryShape)
    });
    return record;
  }
  async invalidateQueryWindow(key) {
    const stringified = stringKey2(key);
    const existing = await this.backend.getQueryWindow(stringified);
    if (!existing) return;
    existing.everCompleted = Boolean(existing.everCompleted) || Boolean(existing.complete);
    existing.complete = false;
    existing.updatedAt = this.clock();
    await this.backend.putQueryWindow(existing);
  }
  async touchDocuments(collection, ids, { estimatedBytes = 0, pinReason = PIN_RECENT_READ } = {}) {
    const now = this.clock();
    const normalizedIds = Array.isArray(ids) ? ids.filter(Boolean) : [];
    if (!normalizedIds.length) return;
    const perDocumentBytes = normalizeEstimatedBytes(estimatedBytes);
    let deltaBytes = 0;
    for (const id of normalizedIds) {
      const previous = await this.backend.getDocumentAccess(collection, id) || {};
      const nextEstimatedBytes = perDocumentBytes || previous.estimatedBytes || 0;
      deltaBytes += nextEstimatedBytes - (previous.estimatedBytes || 0);
      await this.backend.putDocumentAccess({
        collection,
        id,
        lastAccessedAt: now,
        pinReason: previous.dirty ? "dirty" : pinReason,
        dirty: Boolean(previous.dirty),
        estimatedBytes: nextEstimatedBytes
      });
    }
    if (deltaBytes !== 0) {
      const stats = await this.getCacheStats();
      stats.estimatedBytes = Math.max(0, (stats.estimatedBytes || 0) + deltaBytes);
      await this.backend.putCacheStats(stats);
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
    stats.estimatedBytes = await this.estimateWorkingSetBytes();
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
  async invalidateQueryWindowsForDocuments(collection, ids) {
    const normalizedIds = normalizeDocumentIds2(ids);
    if (!collection || !normalizedIds.length) return 0;
    const windowKeys = typeof this.backend.getQueryWindowKeysByDocumentIds === "function" ? await this.backend.getQueryWindowKeysByDocumentIds(collection, normalizedIds) : await this.scanQueryWindowKeysForDocuments(collection, normalizedIds);
    let invalidated = 0;
    const seen = /* @__PURE__ */ new Set();
    for (const key of windowKeys) {
      const stringified = stringKey2(key);
      if (seen.has(stringified)) continue;
      seen.add(stringified);
      const window2 = await this.backend.getQueryWindow(stringified);
      if (!window2 || window2.collection !== collection) continue;
      await this.invalidateQueryWindow([
        window2.collection,
        window2.queryFingerprint,
        window2.offset,
        window2.limit
      ]);
      invalidated += 1;
    }
    return invalidated;
  }
  async invalidateQueryWindowsForChanges(collection, documents, primaryPath = "id") {
    const changes = Array.isArray(documents) ? documents.filter(Boolean) : [];
    if (!collection || !changes.length) return 0;
    if (typeof this.backend.getQueryWindowKeysByDocumentIds !== "function") {
      return this.scanInvalidateQueryWindowsForChanges(collection, changes, primaryPath);
    }
    const lookupIds = /* @__PURE__ */ new Set(["$nonsimple"]);
    for (const document2 of changes) {
      const id = valueAtPath2(document2, primaryPath);
      if (id != null && id !== "") lookupIds.add(String(id));
      for (const path of documentLeafPaths(document2)) lookupIds.add(`$field|${path}`);
    }
    const windowKeys = await this.backend.getQueryWindowKeysByDocumentIds(collection, [...lookupIds]);
    let invalidated = 0;
    const seen = /* @__PURE__ */ new Set();
    for (const key of windowKeys) {
      const stringified = stringKey2(key);
      if (seen.has(stringified)) continue;
      seen.add(stringified);
      const window2 = await this.backend.getQueryWindow(stringified);
      if (!window2 || window2.collection !== collection) continue;
      if (!changeAffectsWindow(window2, changes, primaryPath)) continue;
      await this.invalidateQueryWindow([
        window2.collection,
        window2.queryFingerprint,
        window2.offset,
        window2.limit
      ]);
      invalidated += 1;
    }
    return invalidated;
  }
  // Whole-store fallback for backends without the document-ref index. Keeps the
  // original semantics for the in-memory/degraded path.
  async scanInvalidateQueryWindowsForChanges(collection, changes, primaryPath = "id") {
    const all = await this.backend.scanQueryWindows();
    let invalidated = 0;
    for (const window2 of all) {
      if (window2.collection !== collection) continue;
      if (!changeAffectsWindow(window2, changes, primaryPath)) continue;
      await this.invalidateQueryWindow([
        window2.collection,
        window2.queryFingerprint,
        window2.offset,
        window2.limit
      ]);
      invalidated += 1;
    }
    return invalidated;
  }
  async scanQueryWindowKeysForDocuments(collection, ids) {
    const idSet = new Set(ids);
    const all = await this.backend.scanQueryWindows();
    const keys = [];
    for (const window2 of all) {
      if (window2.collection !== collection) continue;
      const documentIds = Array.isArray(window2.documentIds) ? window2.documentIds : [];
      if (!documentIds.some((id) => idSet.has(String(id || "")))) continue;
      keys.push([
        window2.collection,
        window2.queryFingerprint,
        window2.offset,
        window2.limit
      ]);
    }
    return keys;
  }
  async close() {
    await this.backend.close();
  }
  /// Evicts LRU document access entries until the working set fits the budget.
  /// Skips dirty docs and unexpired recently-read pins. Returns the number of
  /// document records removed.
  async runEvictionIfOverBudget({ forceRecount = false } = {}) {
    const stats = await this.getCacheStats();
    if (!stats.budgetBytes) {
      return 0;
    }
    if (!forceRecount && (stats.estimatedBytes || 0) <= stats.budgetBytes) {
      return 0;
    }
    const all = await this.backend.scanDocumentAccess();
    const workingSetBytes = sumEstimatedDocumentAccessBytes(all);
    if (stats.estimatedBytes !== workingSetBytes) {
      stats.estimatedBytes = workingSetBytes;
      await this.backend.putCacheStats(stats);
    }
    if (workingSetBytes <= stats.budgetBytes) {
      return 0;
    }
    const now = this.clock();
    const candidates = all.filter((record) => !record.dirty).filter((record) => {
      if (record.pinReason !== "recently-read") return true;
      return now - record.lastAccessedAt >= SIDECAR_PIN_RECENT_READ_TTL_MS;
    }).sort((a, b) => a.lastAccessedAt - b.lastAccessedAt);
    let removed = 0;
    let remainingBytes = workingSetBytes;
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
      if (!isQuotaExceeded2(err)) throw err;
      const stats = await this.getCacheStats();
      const tighten = Math.max(1024, Math.floor((stats.budgetBytes || stats.estimatedBytes || 65536) / 2));
      await this.setBudgetBytes(tighten);
      await this.runEvictionIfOverBudget({ forceRecount: true });
      try {
        const result = await writeFn();
        if (stats.budgetBytes) await this.setBudgetBytes(stats.budgetBytes);
        return result;
      } catch (retryErr) {
        if (stats.budgetBytes) await this.setBudgetBytes(stats.budgetBytes);
        throw retryErr;
      }
    }
  }
  /// Starts a periodic eviction scheduler. The handle returned has a
  /// `stop()` method. Idempotent: calling twice with the same handle is
  /// safe. Default interval: 30s.
  startEvictionScheduler({
    intervalMs = 3e4,
    globalBudgetBytes = 0,
    shareBudgetBytes = 0
  } = {}) {
    if (this._evictionSchedulerGroupKey) {
      return { stop: () => this.stopEvictionScheduler() };
    }
    const key = String(this.schedulerKey || this.databaseName);
    let group = evictionSchedulerGroups.get(key);
    if (!group) {
      group = {
        storages: /* @__PURE__ */ new Set(),
        timer: null,
        intervalMs,
        globalBudgetBytes: Math.max(0, Number(globalBudgetBytes) || 0)
      };
      evictionSchedulerGroups.set(key, group);
    }
    group.globalBudgetBytes = Math.max(
      group.globalBudgetBytes,
      Math.max(0, Number(globalBudgetBytes) || 0)
    );
    this._configuredShareBudgetBytes = Math.max(0, Number(shareBudgetBytes) || 0);
    this._evictionSchedulerGroupKey = key;
    group.storages.add(this);
    rebalanceEvictionSchedulerGroup(group).catch(() => {
    });
    if (!group.timer) {
      group.timer = setInterval(() => runEvictionSchedulerGroup(group), group.intervalMs);
      if (typeof group.timer.unref === "function") group.timer.unref();
    }
    return { stop: () => this.stopEvictionScheduler() };
  }
  stopEvictionScheduler() {
    const key = this._evictionSchedulerGroupKey;
    if (!key) return;
    this._evictionSchedulerGroupKey = null;
    const group = evictionSchedulerGroups.get(key);
    if (!group) return;
    group.storages.delete(this);
    if (group.storages.size === 0) {
      if (group.timer) clearInterval(group.timer);
      evictionSchedulerGroups.delete(key);
      return;
    }
    rebalanceEvictionSchedulerGroup(group).catch(() => {
    });
  }
  /// Orphan-window GC: hard-delete sidecar query-window entries (and their
  /// document/selector refs — `deleteQueryWindow` cascades) that have aged out.
  /// Two thresholds, because one-off queries with a varying selector value mint
  /// a fresh window on every exec:
  ///   - complete / ever-completed windows: `maxAgeMs` (default 7 days). These
  ///     are served local-first while revalidating (the `everCompleted` flag is
  ///     load-bearing for stale-while-revalidate, see correctness-reconnect),
  ///     so they get the full window.
  ///   - windows that were invalidated/minted but NEVER completed (pure
  ///     tombstones, no local-first value): `staleIncompleteMaxAgeMs` (default
  ///     1 hour). This is the "short grace" hard-delete for sticky tombstones.
  /// Wired into the production eviction scheduler via `runSchedulerMaintenance`.
  async runWindowGc({
    maxAgeMs = 7 * 24 * 60 * 60 * 1e3,
    staleIncompleteMaxAgeMs = 60 * 60 * 1e3
  } = {}) {
    const now = this.clock();
    const all = await this.backend.scanQueryWindows();
    let removed = 0;
    for (const window2 of all) {
      const age = now - (window2.lastAccessedAt ?? window2.updatedAt ?? window2.createdAt ?? now);
      const servesLocalFirst = Boolean(window2.complete) || Boolean(window2.everCompleted);
      const threshold = servesLocalFirst ? maxAgeMs : staleIncompleteMaxAgeMs;
      if (age >= threshold) {
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
  /// Periodic maintenance run by the eviction scheduler timer (and callable
  /// directly in tests). Evicts over-budget documents AND reclaims aged-out
  /// query windows — the latter is the only production caller of `runWindowGc`,
  /// which previously ran from tests alone while the sidecar grew unbounded.
  async runSchedulerMaintenance() {
    const evicted = await this.runEvictionIfOverBudget().catch(() => 0);
    const windowsReclaimed = await this.runWindowGc().catch(() => 0);
    return { evicted, windowsReclaimed };
  }
};
function changeAffectsWindow(window2, changes, primaryPath) {
  const members = new Set((window2.documentIds || []).map(String));
  const simple = simpleEqualitySelector(window2.queryShape);
  if (!simple) return true;
  return changes.some((document2) => {
    const id = valueAtPath2(document2, primaryPath);
    return members.has(String(id ?? "")) || matchesSimpleEquality(document2, simple);
  });
}
function computeSelectorRefIds(queryShape) {
  const simple = simpleEqualitySelector(queryShape);
  if (!simple) return ["$nonsimple"];
  return simple.map(([field]) => `$field|${field}`);
}
function documentLeafPaths(document2, prefix = "", out = /* @__PURE__ */ new Set(), depth = 0) {
  if (!document2 || typeof document2 !== "object" || Array.isArray(document2) || depth > 6) return out;
  for (const [key, value] of Object.entries(document2)) {
    if (key === "_meta") continue;
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      out.add(path);
      documentLeafPaths(value, path, out, depth + 1);
    } else {
      out.add(path);
    }
  }
  return out;
}
function simpleEqualitySelector(queryShape) {
  if (!queryShape || typeof queryShape !== "object") return null;
  if (Array.isArray(queryShape.sort) && queryShape.sort.length > 0) return null;
  const selector = queryShape.selector;
  if (!selector || typeof selector !== "object" || Array.isArray(selector)) return null;
  const entries = Object.entries(selector);
  if (!entries.length) return null;
  const equalities = [];
  for (const [field, condition] of entries) {
    if (!field || field.startsWith("$")) return null;
    if (condition && typeof condition === "object") {
      const keys = Object.keys(condition);
      if (keys.length !== 1 || keys[0] !== "$eq") return null;
      equalities.push([field, condition.$eq]);
    } else {
      equalities.push([field, condition]);
    }
  }
  return equalities;
}
function matchesSimpleEquality(document2, equalities) {
  if (!equalities || document2?._deleted) return false;
  return equalities.every(([field, expected]) => Object.is(valueAtPath2(document2, field), expected));
}
function valueAtPath2(value, path) {
  return String(path || "").split(".").filter(Boolean).reduce((current, segment) => current?.[segment], value);
}
function structuredCloneSafe3(value) {
  try {
    return globalThis.structuredClone?.(value) ?? JSON.parse(JSON.stringify(value));
  } catch {
    return null;
  }
}
async function rebalanceEvictionSchedulerGroup(group) {
  const storages = [...group.storages];
  if (!storages.length) return;
  const globalShare = group.globalBudgetBytes > 0 ? Math.max(1, Math.floor(group.globalBudgetBytes / storages.length)) : Number.POSITIVE_INFINITY;
  await Promise.all(storages.map(async (storage) => {
    const configured = storage._configuredShareBudgetBytes || globalShare;
    const effective = Math.max(1, Math.floor(Math.min(configured, globalShare)));
    await storage.setBudgetBytes(effective);
  }));
}
async function runEvictionSchedulerGroup(group) {
  await rebalanceEvictionSchedulerGroup(group);
  await Promise.all(
    // SYNC-52: run full maintenance (eviction AND window GC) on each tick so
    // orphan query windows are actually reclaimed in production.
    [...group.storages].map((storage) => storage.runSchedulerMaintenance().catch(() => 0))
  );
}
async function recoverQueryMetaQuota(schedulerKey) {
  const group = evictionSchedulerGroups.get(String(schedulerKey || ""));
  if (!group?.storages?.size) return { evicted: 0, storages: 0 };
  const storages = [...group.storages];
  const previous = await Promise.all(storages.map((storage) => storage.getCacheStats()));
  let evicted = 0;
  try {
    for (let index = 0; index < storages.length; index += 1) {
      const storage = storages[index];
      const stats = previous[index];
      const tightened = Math.max(1024, Math.floor((stats.budgetBytes || stats.estimatedBytes || 65536) / 2));
      await storage.setBudgetBytes(tightened);
      evicted += await storage.runEvictionIfOverBudget({ forceRecount: true });
    }
  } finally {
    await rebalanceEvictionSchedulerGroup(group);
  }
  return { evicted, storages: storages.length };
}
function normalizeEstimatedBytes(estimatedBytes) {
  const bytes = Math.max(0, Number(estimatedBytes) || 0);
  return bytes > 0 ? Math.max(1, Math.ceil(bytes)) : 0;
}
function sumEstimatedDocumentAccessBytes(records) {
  return (Array.isArray(records) ? records : []).reduce(
    (sum, record) => sum + (record.estimatedBytes || 0),
    0
  );
}
function normalizeDocumentIds2(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || "")).filter(Boolean)));
}
function isQuotaExceeded2(err) {
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

// src/apps/business-os/rxdb/src/storage-indexeddb.mjs
var DB_VERSION = 3;
var DOCUMENT_STORE = "documents";
var SCHEMA_INDEX_ENTRIES = "schemaIndexEntries";
var PUSHABLE_LWT_INDEX = "collectionPushableLwtId";
var OPEN_DATABASE_TIMEOUT_MS = 4e3;
var REPLICATION_SCAN_MULTIPLIER = 50;
var REPLICATION_MIN_SCAN_LIMIT = 1;
var REPLICATION_MAX_SCAN_LIMIT = 5e3;
var INDEX_HIGH_KEY = "\uFFFF";
var unsyncedCountScheduled = /* @__PURE__ */ new WeakSet();
async function openCtoxIndexedDbStorage({ databaseName = "ctox_business_os_js_v1" } = {}) {
  if (!globalThis.indexedDB) {
    throw new Error("indexedDB is required for ctox-rxdb-js storage");
  }
  const db = await openDatabase(databaseName);
  const quotaCoordinator = {
    recover: (context = {}) => recoverQueryMetaQuota(databaseName, context)
  };
  const recoveryJournal = await openRecoveryJournal({
    databaseName,
    instanceId: databaseName,
    quotaCoordinator
  });
  return new CtoxIndexedDbStorage(db, { recoveryJournal, quotaCoordinator });
}
var CtoxIndexedDbStorage = class {
  constructor(db, { recoveryJournal = null, quotaCoordinator = null } = {}) {
    this.db = db;
    this.recoveryJournal = recoveryJournal;
    this.quotaCoordinator = quotaCoordinator;
  }
  collection(name, { schema = null, conflictStrategy = "lww", deleteStrategy = "default" } = {}) {
    if (!name || typeof name !== "string") {
      throw new TypeError("collection name must be a non-empty string");
    }
    return new CtoxIndexedDbCollection(this.db, name, {
      schema,
      conflictStrategy,
      deleteStrategy,
      recoveryJournal: this.recoveryJournal,
      quotaCoordinator: this.quotaCoordinator
    });
  }
  async unsyncedWriteSummary() {
    return countUnsyncedWrites(this.db);
  }
  close() {
    this.recoveryJournal?.close?.();
    this.db.close();
  }
};
var CtoxIndexedDbCollection = class {
  constructor(db, name, {
    schema = null,
    conflictStrategy = "lww",
    deleteStrategy = "default",
    recoveryJournal = null,
    quotaCoordinator = null
  } = {}) {
    this.db = db;
    this.name = name;
    this.schema = schema || {};
    this.conflictStrategy = normalizeConflictStrategy(conflictStrategy);
    this.deleteStrategy = normalizeDeleteStrategy(deleteStrategy);
    this.primaryPath = primaryPathFromSchema(schema);
    this.indexes = normalizeSchemaIndexes(schema, this.primaryPath);
    this.indexSignature = schemaIndexSignature(this.indexes);
    this.schemaIndexReady = null;
    this.queryPerformancePolicy = { rejectAllDocumentsFallback: false };
    this.queryPerformanceStats = createQueryPerformanceStats();
    this.mergeStats = { pullFieldMerges: 0, pushConflictMerges: 0 };
    this.events = new CtoxEventEmitter();
    this.recoveryJournal = recoveryJournal;
    this.quotaCoordinator = quotaCoordinator;
    this.recoverySchemaHash = "";
    this.recoveryReady = null;
    this.externalChangeListener = (event) => {
      const detail = event?.detail || {};
      if (detail.databaseName !== this.db.name || detail.collection !== this.name) return;
      this.events.emit("change", {
        collection: this.name,
        external: true,
        ids: Array.isArray(detail.ids) ? detail.ids : [],
        at: Date.now()
      });
    };
    globalThis.addEventListener?.("ctox-rxdb-external-change", this.externalChangeListener);
  }
  close() {
    globalThis.removeEventListener?.("ctox-rxdb-external-change", this.externalChangeListener);
  }
  async initializeRecovery() {
    if (!this.recoveryJournal) return;
    if (!this.recoveryReady) {
      this.recoveryReady = (async () => {
        this.recoverySchemaHash = await schemaHash(this.schema || {}, this.name);
        this.recoveryJournal.registerCollection(this.name, {
          schemaHash: this.recoverySchemaHash,
          applyBatch: (batch) => this.runWithQuotaRecovery(
            () => batch.operation === "upsert" ? this._bulkUpsertOnce(batch.rows || [], {}) : this._bulkWriteOnce(batch.rows || [], { baseById: batch.baseById || null }),
            { source: "recovery-replay" }
          ),
          // A user resolution is a NEW pushable local write, not recovery
          // replay. Route it through the public path so it receives a fresh
          // HLC and durable WAL entry before the primary row changes.
          resolveConflict: (batch) => this.bulkWrite(batch.rows || [], {
            baseById: batch.baseById || null
          }),
          // SYNC-42: keep_master resolution of a quarantined conflict. The
          // pull checkpoint already advanced past the master row, so it will
          // not be re-delivered — apply the journaled master state
          // authoritatively (origin-stamped, non-pushable, base cleared). Force
          // bypasses the LWW gate (the local edit may outrank the master lwt)
          // and `authoritativeMaster` bypasses the three-way merge that would
          // otherwise just re-throw the same structured conflict. `reconciled`
          // suppresses the delete_vs_update journaling for a master tombstone.
          applyMaster: (batch) => this.bulkWrite(batch.rows || [], {
            replicationOrigin: { role: "ctox_instance", reconciled: true },
            force: true,
            authoritativeMaster: true,
            skipJournal: true
          })
        });
        await this.acknowledgePersistedMasterRecovery();
        await this.recoveryJournal.replayRegisteredCollections(this.name);
      })();
    }
    return this.recoveryReady;
  }
  async acknowledgePersistedMasterRecovery() {
    const batches = await this.recoveryJournal?.listBatches?.("pending", this.name) || [];
    const ids = [...new Set(batches.filter((batch) => batch.collection === this.name).flatMap((batch) => batch.documentIds || []))];
    if (!ids.length) return;
    const documents = {};
    for (const id of ids) {
      const record = await this.getStoredRecord(id);
      if (record?.replicationOriginRole && record.doc) documents[id] = record.doc;
    }
    if (Object.keys(documents).length) {
      await this.recoveryJournal.markMasterAcknowledged(this.name, documents);
    }
  }
  observe(listener) {
    return this.events.on("change", (event) => listener(event?.detail || event));
  }
  // Raw stored record (doc + base + replication flags) for one id. Used by
  // the replication push path to fetch the merge base on masterWrite
  // conflicts; not part of the query surface.
  async getStoredRecord(id) {
    if (!id) return null;
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const store = tx.objectStore(DOCUMENT_STORE);
    const record = await idbRequest(store.get([this.name, id]));
    return record || null;
  }
  // Field-merge + merge-base tracking for one incoming write that already
  // passed `shouldAcceptDocumentWrite`. Decides what actually gets stored:
  //
  //   - LOCAL write on a field-merge collection: carry the merge base along
  //     (the last master-confirmed doc, surviving consecutive local writes).
  //   - Replication write over an UNSYNCED LOCAL row on a field-merge
  //     collection: three-way merge. If local field changes survive, the
  //     result is stored as a LOCAL (pushable) write — deliberately WITHOUT
  //     the replication-origin stamp, because it still carries state the
  //     master has not seen — with the incoming master doc as the new base.
  //   - Everything else: unchanged pass-through (whole-doc LWW semantics).
  resolveIncomingWrite({ previous, doc, lwt, replicationOrigin, explicitBase }) {
    const mergeEnabled = this.conflictStrategy === "field-merge";
    if (!replicationOrigin?.role) {
      const base = explicitBase !== void 0 ? mergeEnabled ? explicitBase : void 0 : mergeEnabled && previous ? previous.replicationOriginRole ? previous.doc : previous.base : void 0;
      return { doc, lwt, replicationOrigin, base };
    }
    const existingIsLocalWrite = Boolean(previous) && !previous.doc?._meta?.ctoxReplicationOrigin;
    if (!mergeEnabled || !existingIsLocalWrite) {
      return { doc, lwt, replicationOrigin, base: void 0 };
    }
    if (doc?._deleted) {
      return { doc, lwt, replicationOrigin, base: void 0 };
    }
    const { merged, identicalToMaster, requiresManualResolution, conflictFields } = threeWayMergeDocuments(
      previous.base,
      previous.doc,
      doc,
      { primaryPath: this.primaryPath }
    );
    if (requiresManualResolution) {
      const error = new Error(`Structured conflict requires native/manual resolution for ${this.name}: ${conflictFields.join(", ")}`);
      error.code = "structured_conflict_requires_resolution";
      error.collection = this.name;
      error.fields = conflictFields;
      error.base = previous.base;
      error.local = previous.doc;
      error.master = doc;
      throw error;
    }
    if (identicalToMaster) {
      return { doc, lwt, replicationOrigin, base: void 0 };
    }
    this.mergeStats.pullFieldMerges += 1;
    const mergedLwt = Math.max(Number(lwt) || 0, Number(previous.lwt) || 0) + 1;
    return { doc: merged, lwt: mergedLwt, replicationOrigin: null, base: doc };
  }
  async upsert(doc) {
    const id = documentId2(doc);
    if (!id) {
      throw new Error(`Cannot upsert ${this.name} document without primary key`);
    }
    const { success } = await this.bulkUpsert([doc]);
    return success[id] || null;
  }
  async bulkUpsert(docs, {
    now = Date.now(),
    replicationOrigin = null,
    skipJournal = false,
    recoveryReplay = false
  } = {}) {
    await this.initializeRecovery();
    const journalWrite = Boolean(this.recoveryJournal) && !skipJournal && !replicationOrigin?.role && Array.isArray(docs);
    const prepared = journalWrite ? await this.prepareJournalRows(docs) : { rows: docs, baseById: null };
    const writeDocs = prepared.rows;
    const validDocs = Array.isArray(writeDocs) ? writeDocs.filter((doc) => documentId2(doc)) : writeDocs;
    const journalBaseById = prepared.baseById;
    const batchId = !skipJournal && !replicationOrigin?.role && Array.isArray(validDocs) && validDocs.length ? await this.recoveryJournal?.appendBatch({
      collection: this.name,
      schemaHash: this.recoverySchemaHash,
      primaryPath: this.primaryPath,
      operation: "upsert",
      rows: validDocs,
      baseById: journalBaseById
    }) : null;
    let result;
    try {
      await this.persistDeleteUpdateConflicts(validDocs, replicationOrigin);
      result = await this.runWithQuotaRecovery(
        () => this._bulkUpsertOnce(writeDocs, { now, replicationOrigin }),
        { source: recoveryReplay ? "recovery-replay" : "bulk-upsert" }
      );
    } catch (error) {
      await this.persistStructuredConflict(error);
      throw error;
    }
    await this.persistOverwrittenLocalConflicts(result);
    await this.persistStructuredConflictQuarantine(result);
    if (batchId) await this.recoveryJournal.commitBatch(batchId, result.success);
    if (replicationOrigin?.role) await this.recoveryJournal?.markMasterAcknowledged(this.name, result.success);
    return result;
  }
  async _bulkUpsertOnce(docs, { now = Date.now(), replicationOrigin = null } = {}) {
    if (!Array.isArray(docs)) {
      throw new TypeError("bulkUpsert docs must be an array");
    }
    const tx = this.db.transaction(DOCUMENT_STORE, "readwrite");
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];
    const overwrittenLocalConflicts = [];
    const structuredConflicts = [];
    let localWriteLwtFloor = null;
    if (!replicationOrigin?.role) {
      localWriteLwtFloor = await latestCollectionLwtInTransaction(store, this.name) + 1;
    }
    try {
      for (const doc of docs) {
        const id = documentId2(doc);
        if (!id) {
          error.push({ document: doc, error: "missing primary key" });
          continue;
        }
        const previous = await idbRequest(store.get([this.name, id]));
        const nextDocument = { ...previous?.doc || {}, ...doc };
        let lwt = documentLwt(nextDocument, now);
        if (localWriteLwtFloor !== null) {
          lwt = Math.max(lwt, localWriteLwtFloor);
          localWriteLwtFloor = lwt + 1;
        }
        if (!shouldAcceptDocumentWrite(previous, lwt, replicationOrigin, nextDocument, this.name, this.conflictStrategy, this.deleteStrategy)) {
          const rejectedUpdate = finalDeleteRejectedUpdateConflict({
            previous,
            incomingDocument: nextDocument,
            collectionName: this.name,
            deleteStrategy: this.deleteStrategy,
            replicationOrigin
          });
          if (rejectedUpdate) overwrittenLocalConflicts.push(rejectedUpdate);
          if (previous?.doc) success[id] = previous.doc;
          continue;
        }
        const overwriteConflict = lwwOverwriteConflict({
          previous,
          incomingDocument: nextDocument,
          collectionName: this.name,
          conflictStrategy: this.conflictStrategy,
          replicationOrigin
        });
        if (overwriteConflict) overwrittenLocalConflicts.push(overwriteConflict);
        let resolved;
        try {
          resolved = this.resolveIncomingWrite({
            previous,
            doc: nextDocument,
            lwt,
            replicationOrigin
          });
        } catch (conflictError) {
          if (conflictError?.code !== "structured_conflict_requires_resolution") throw conflictError;
          structuredConflicts.push(quarantineConflictRecord(conflictError, this.name, id, this.primaryPath));
          continue;
        }
        if (resolved.replicationOrigin?.role && previous && Number(previous.lwt || 0) >= resolved.lwt) {
          resolved.lwt = Number(previous.lwt) + 1;
        }
        const stored = storedRecordForWrite({
          collection: this.name,
          id,
          doc: resolved.doc,
          lwt: resolved.lwt,
          indexes: this.indexes,
          indexSignature: this.indexSignature,
          replicationOrigin: resolved.replicationOrigin,
          base: resolved.base,
          previous
        });
        await idbRequest(store.put(stored));
        success[id] = stored.doc;
      }
      await done;
    } catch (error2) {
      try {
        tx.abort();
      } catch {
      }
      try {
        await done;
      } catch {
      }
      throw error2;
    }
    schedulePersistUnsyncedWriteCount(this.db);
    if (Object.keys(success).length) {
      this.events.emit("change", {
        collection: this.name,
        success,
        at: now
      });
      dispatchStorageChange(this.db.name, this.name, success, replicationOrigin);
    }
    return { success, error, overwrittenLocalConflicts, structuredConflicts };
  }
  async bulkWrite(rows, {
    now = Date.now(),
    replicationOrigin = null,
    baseById = null,
    skipJournal = false,
    recoveryReplay = false,
    force = false,
    authoritativeMaster = false
  } = {}) {
    await this.initializeRecovery();
    const journalWrite = Boolean(this.recoveryJournal) && !skipJournal && !replicationOrigin?.role && Array.isArray(rows);
    const prepared = journalWrite ? await this.prepareJournalRows(rows) : { rows, baseById: null };
    const writeRows = prepared.rows;
    const validRows = Array.isArray(writeRows) ? writeRows.filter((row) => documentId2(row?.document || row)) : writeRows;
    const journalBaseById = baseById || prepared.baseById;
    const batchId = !skipJournal && !replicationOrigin?.role && Array.isArray(validRows) && validRows.length ? await this.recoveryJournal?.appendBatch({
      collection: this.name,
      schemaHash: this.recoverySchemaHash,
      primaryPath: this.primaryPath,
      operation: "write",
      rows: validRows,
      baseById: journalBaseById
    }) : null;
    let result;
    try {
      await this.persistDeleteUpdateConflicts(validRows, replicationOrigin);
      result = await this.runWithQuotaRecovery(
        () => this._bulkWriteOnce(writeRows, { now, replicationOrigin, baseById: journalBaseById, force, authoritativeMaster }),
        { source: recoveryReplay ? "recovery-replay" : "bulk-write" }
      );
    } catch (error) {
      await this.persistStructuredConflict(error);
      throw error;
    }
    await this.persistOverwrittenLocalConflicts(result);
    await this.persistStructuredConflictQuarantine(result);
    if (batchId) await this.recoveryJournal.commitBatch(batchId, result.success);
    if (replicationOrigin?.role) await this.recoveryJournal?.markMasterAcknowledged(this.name, result.success);
    return result;
  }
  async _bulkWriteOnce(rows, { now = Date.now(), replicationOrigin = null, baseById = null, force = false, authoritativeMaster = false } = {}) {
    if (!Array.isArray(rows)) {
      throw new TypeError("bulkWrite rows must be an array");
    }
    const tx = this.db.transaction(DOCUMENT_STORE, "readwrite");
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];
    const overwrittenLocalConflicts = [];
    const structuredConflicts = [];
    let localWriteLwtFloor = null;
    if (!replicationOrigin?.role) {
      localWriteLwtFloor = await latestCollectionLwtInTransaction(store, this.name) + 1;
    }
    try {
      for (const row of rows) {
        const doc = row?.document || row;
        const id = documentId2(doc);
        if (!id) {
          error.push({ row, error: "missing primary key" });
          continue;
        }
        let lwt = documentLwt(doc, now);
        if (localWriteLwtFloor !== null) {
          lwt = Math.max(lwt, localWriteLwtFloor);
          localWriteLwtFloor = lwt + 1;
        }
        const previous = await idbRequest(store.get([this.name, id]));
        if (!force && !shouldAcceptDocumentWrite(previous, lwt, replicationOrigin, doc, this.name, this.conflictStrategy, this.deleteStrategy)) {
          const rejectedUpdate = finalDeleteRejectedUpdateConflict({
            previous,
            incomingDocument: doc,
            collectionName: this.name,
            deleteStrategy: this.deleteStrategy,
            replicationOrigin
          });
          if (rejectedUpdate) overwrittenLocalConflicts.push(rejectedUpdate);
          continue;
        }
        const overwriteConflict = force ? null : lwwOverwriteConflict({
          previous,
          incomingDocument: doc,
          collectionName: this.name,
          conflictStrategy: this.conflictStrategy,
          replicationOrigin
        });
        if (overwriteConflict) overwrittenLocalConflicts.push(overwriteConflict);
        let resolved;
        if (authoritativeMaster) {
          resolved = { doc, lwt, replicationOrigin, base: void 0 };
        } else {
          try {
            resolved = this.resolveIncomingWrite({
              previous,
              doc,
              lwt,
              replicationOrigin,
              explicitBase: baseById && Object.prototype.hasOwnProperty.call(baseById, id) ? baseById[id] : void 0
            });
          } catch (conflictError) {
            if (conflictError?.code !== "structured_conflict_requires_resolution") throw conflictError;
            structuredConflicts.push(quarantineConflictRecord(conflictError, this.name, id, this.primaryPath));
            continue;
          }
        }
        if (resolved.replicationOrigin?.role && previous && Number(previous.lwt || 0) >= resolved.lwt) {
          resolved.lwt = Number(previous.lwt) + 1;
        }
        const stored = storedRecordForWrite({
          collection: this.name,
          id,
          doc: resolved.doc,
          lwt: resolved.lwt,
          indexes: this.indexes,
          indexSignature: this.indexSignature,
          replicationOrigin: resolved.replicationOrigin,
          base: resolved.base,
          previous
        });
        await idbRequest(store.put(stored));
        success[id] = stored.doc;
      }
      await done;
    } catch (error2) {
      try {
        tx.abort();
      } catch {
      }
      try {
        await done;
      } catch {
      }
      throw error2;
    }
    schedulePersistUnsyncedWriteCount(this.db);
    if (Object.keys(success).length) {
      this.events.emit("change", {
        collection: this.name,
        success,
        at: now
      });
      dispatchStorageChange(this.db.name, this.name, success, replicationOrigin);
    }
    return { success, error, overwrittenLocalConflicts, structuredConflicts };
  }
  async runWithQuotaRecovery(operation, context = {}) {
    try {
      return await operation();
    } catch (error) {
      if (!isQuotaExceededError(error)) throw error;
      await this.quotaCoordinator?.recover?.(context);
      try {
        return await operation();
      } catch (retryError) {
        const quotaError = new Error("IndexedDB write failed after safe cache eviction and one retry.", { cause: retryError });
        quotaError.code = "indexeddb_quota_exceeded";
        quotaError.retryable = true;
        throw quotaError;
      }
    }
  }
  async persistStructuredConflict(error) {
    if (error?.code !== "structured_conflict_requires_resolution") return;
    await this.recoveryJournal?.recordConflict?.({
      code: error.code,
      collection: this.name,
      fields: error.fields || [],
      base: error.base || null,
      local: error.local || null,
      master: error.master || null,
      message: error.message || String(error)
    });
  }
  async persistDeleteUpdateConflicts(rows, replicationOrigin) {
    if (!replicationOrigin?.role || !Array.isArray(rows)) return;
    if (replicationOrigin?.reconciled) return;
    for (const row of rows) {
      const master = row?.document || row;
      if (!master?._deleted) continue;
      const id = documentId2(master, this.primaryPath);
      const previous = id ? await this.getStoredRecord(id) : null;
      if (!previous || previous.replicationOriginRole || previous.doc?._deleted) continue;
      await this.recoveryJournal?.recordConflict?.({
        code: "structured_conflict_requires_resolution",
        conflictType: "delete_vs_update",
        collection: this.name,
        base: previous.base || null,
        local: previous.doc,
        master,
        message: "The native tombstone is authoritative; the local update remains recoverable here."
      });
    }
  }
  // SYNC-11: journal unsynced local writes that lost the whole-document LWW
  // gate to an accepted master row (`update_vs_update`, mirroring the
  // `delete_vs_update` entries above). The losing rows are collected inside
  // the storage transaction and journaled here AFTER it commits, so a
  // quota-retried transaction never journals the same loss twice.
  async persistOverwrittenLocalConflicts(result) {
    const conflicts = result?.overwrittenLocalConflicts;
    if (Array.isArray(result?.overwrittenLocalConflicts)) delete result.overwrittenLocalConflicts;
    if (!Array.isArray(conflicts)) return;
    for (const conflict of conflicts) {
      await this.recoveryJournal?.recordConflict?.(conflict);
    }
  }
  // SYNC-42: journal the documents QUARANTINED by the batch apply (an
  // unmergeable structured field conflict). Collected inside the storage
  // transaction and journaled here AFTER it commits — so a quota-retried
  // transaction never journals twice, and the good documents in the same batch
  // are already durable. Each record carries a deterministic conflict id, so an
  // idempotent re-delivery of the same batch (e.g. a crash between the primary
  // commit and the pull checkpoint advancing) OVERWRITES the same record
  // instead of appending a duplicate. The record captures local + master + base
  // so db.conflicts.resolve() stays fully functional after the checkpoint has
  // moved past the master row.
  async persistStructuredConflictQuarantine(result) {
    const conflicts = result?.structuredConflicts;
    if (Array.isArray(result?.structuredConflicts)) delete result.structuredConflicts;
    if (!Array.isArray(conflicts) || !conflicts.length) return;
    for (const conflict of conflicts) {
      await this.recoveryJournal?.recordConflict?.(conflict);
    }
  }
  // SYNC-40: reconcile local writes the native peer TERMINALLY rejected
  // (authz/schema) — not a conflict, not a transient transport error. Commands
  // already do native-authoritative accept-master rollback; ordinary data
  // writes used to leave the denied doc in the store, re-pushed and re-denied
  // forever (a silently divergent local mirror). For each still-pending local
  // write in the rejected batch we:
  //   1. journal the rejected local version into the conflict store so the
  //      user's data stays recoverable and surfaced (like delete_vs_update /
  //      update_vs_update), then
  //   2. roll the local mirror back to the master's last-confirmed state — the
  //      stored merge base if we have one, else a tombstone (the doc was never
  //      accepted by the master) — as a FORCED origin write that clears the
  //      pushable flag and overrides the LWW gate, and
  //   3. drop the write from the recovery WAL so it stops being re-pushed.
  // Already-synced/origin rows (a concurrent master pull won the id) are skipped.
  async reconcileRejectedLocalWrites(documents = [], { origin = null, code = "authz_rejected", message = "" } = {}) {
    await this.initializeRecovery();
    const role = String(origin?.role || "ctox_instance").slice(0, 64) || "ctox_instance";
    const reconciledOrigin = { role, peerId: origin?.peerId || "", sessionId: origin?.sessionId || "", reconciled: true };
    const rollbackRows = [];
    const reconciledIds = [];
    for (const doc of Array.isArray(documents) ? documents : []) {
      const source = doc?.newDocumentState || doc?.document || doc;
      const id = documentId2(source);
      if (!id) continue;
      const record = await this.getStoredRecord(id);
      if (!record || record.replicationOriginRole || !record.doc) continue;
      await this.recoveryJournal?.recordConflict?.({
        code: "structured_conflict_requires_resolution",
        conflictType: "authz_rejected",
        collection: this.name,
        base: record.base || null,
        local: record.doc,
        master: record.base || null,
        message: message || "The native peer refused this write (authorization/schema). The rejected local version remains recoverable here.",
        rejectionCode: String(code || "authz_rejected")
      });
      const masterState = record.base ? record.base : { ...record.doc, _deleted: true };
      rollbackRows.push({ document: masterState });
      reconciledIds.push(id);
    }
    if (rollbackRows.length) {
      await this.bulkWrite(rollbackRows, {
        replicationOrigin: reconciledOrigin,
        force: true,
        skipJournal: true
      });
    }
    if (reconciledIds.length) {
      await this.recoveryJournal?.markReconciled?.(this.name, reconciledIds);
    }
    return reconciledIds;
  }
  async prepareJournalRows(rows) {
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const bases = {};
    const prepared = [];
    const lastHlcById = /* @__PURE__ */ new Map();
    for (const row of rows || []) {
      const document2 = row?.document || row;
      const id = documentId2(document2);
      if (!id) {
        prepared.push(row);
        continue;
      }
      const previous = await idbRequest(store.get([this.name, id]));
      if (previous && !Object.prototype.hasOwnProperty.call(bases, id)) {
        bases[id] = previous.replicationOriginRole ? previous.doc : previous.base || previous.doc;
      }
      const nextDocument = structuredClone(document2);
      const nextHlc = nextHybridLogicalClock(
        lastHlcById.get(id) || previous?.doc?._meta?.ctoxHlc || nextDocument?._meta?.ctoxHlc
      );
      lastHlcById.set(id, nextHlc);
      nextDocument._meta = {
        ...nextDocument._meta || {},
        ctoxHlc: nextHlc
      };
      prepared.push(row?.document ? { ...row, document: nextDocument } : nextDocument);
    }
    await done;
    return { rows: prepared, baseById: bases };
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
    const stats = this.queryPerformanceStats;
    stats.allDocumentsCalls += 1;
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collection");
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    let rowsRead = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      rowsRead += 1;
      if (withDeleted || !record.deleted) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    stats.allDocumentsRowsRead += rowsRead;
    stats.lastAllDocumentsRowsRead = rowsRead;
    return documents;
  }
  async queryDocuments(query = {}, helpers = {}) {
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    if (primaryIds) {
      const byId = await this.findDocumentsById(primaryIds);
      const docs2 = primaryIds.map((id) => byId[id]).filter(Boolean);
      return applyQueryToDocuments(docs2, query, helpers);
    }
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    if (schemaIndexPlan) {
      return this.queryDocumentsBySchemaIndex(schemaIndexPlan, query, helpers);
    }
    if (canUseCollectionLwtQuery(query)) {
      return this.queryDocumentsByLwt(query, helpers);
    }
    if (canUseBoundedCollectionCursor(query)) {
      return this.queryDocumentsByCollectionCursor(query, helpers);
    }
    const fallback = this.recordAllDocumentsFallback(query);
    if (this.queryPerformancePolicy.rejectAllDocumentsFallback) {
      throw createAllDocumentsFallbackError(this.name, query, fallback);
    }
    const docs = await this.allDocuments();
    fallback.rowsRead = this.queryPerformanceStats.lastAllDocumentsRowsRead || docs.length;
    this.queryPerformanceStats.allDocumentsFallbackRowsRead += fallback.rowsRead;
    return applyQueryToDocuments(docs, query, helpers);
  }
  async queryDocumentsBySchemaIndex(plan, query = {}, helpers = {}) {
    await this.ensureSchemaIndexEntries();
    const { matchesSelector: matchesSelector2 = () => true, sortDocuments: sortDocuments2 = (docs) => docs } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const maxMatches = plan.canStopAtLimit && Number.isFinite(limit) ? skip + limit : Number.POSITIVE_INFINITY;
    const documents = [];
    const seen = /* @__PURE__ */ new Set();
    for (const entryRange of plan.ranges) {
      const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
      const index = tx.objectStore(DOCUMENT_STORE).index(SCHEMA_INDEX_ENTRIES);
      const range = IDBKeyRange.bound(
        [this.name, plan.index.name, ...entryRange.lower],
        [this.name, plan.index.name, ...entryRange.upper],
        Boolean(entryRange.lowerOpen),
        Boolean(entryRange.upperOpen)
      );
      await iterateCursor(index.openCursor(range, plan.direction), (cursor) => {
        if (!cursor || documents.length >= maxMatches) return false;
        const record = cursor.value;
        if (!record || seen.has(record.id)) return true;
        seen.add(record.id);
        if (!record.deleted && matchesSelector2(record.doc, selector)) {
          documents.push(record.doc);
        }
        return documents.length < maxMatches;
      });
      await idbTransactionDone(tx);
      if (documents.length >= maxMatches) break;
    }
    let sorted = plan.sortCovered ? documents : sortDocuments2(documents, query?.sort || []);
    if (skip > 0) sorted = sorted.slice(skip);
    if (Number.isFinite(limit)) sorted = sorted.slice(0, limit);
    return sorted;
  }
  async queryDocumentsByCollectionCursor(query = {}, helpers = {}) {
    const { matchesSelector: matchesSelector2 = () => true } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collection");
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    let skipped = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || documents.length >= limit) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector2(record.doc, selector)) {
        if (skipped < skip) {
          skipped += 1;
        } else {
          documents.push(record.doc);
        }
      }
      return documents.length < limit;
    });
    await idbTransactionDone(tx);
    return documents;
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
  async countDocuments(query = {}, helpers = {}) {
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    if (primaryIds) {
      const byId = await this.findDocumentsById(primaryIds);
      const docs = primaryIds.map((id) => byId[id]).filter(Boolean);
      return applyQueryToDocuments(docs, query, helpers).length;
    }
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    if (schemaIndexPlan) {
      return (await this.queryDocumentsBySchemaIndex(schemaIndexPlan, query, helpers)).length;
    }
    if (canUseCollectionLwtQuery(query)) {
      return (await this.queryDocumentsByLwt(query, helpers)).length;
    }
    const { matchesSelector: matchesSelector2 = () => true } = helpers || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index("collection");
    const range = IDBKeyRange.only(this.name);
    let skipped = 0;
    let count = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || count >= limit) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector2(record.doc, query?.selector || {})) {
        if (skipped < skip) {
          skipped += 1;
        } else {
          count += 1;
        }
      }
      return count < limit;
    });
    await idbTransactionDone(tx);
    return count;
  }
  async getChangedDocumentsSince(checkpoint = null, limit = 100, options = {}) {
    const fromLwt = Number(checkpoint?.lwt || 0);
    const fromId = String(checkpoint?.id || "");
    const excludedOriginRole = String(options?.excludeReplicationOriginRole || "").trim();
    const usePushableIndex = shouldUsePushableReplicationIndex(excludedOriginRole);
    const scanLimit = replicationScanLimit(limit);
    const tx = this.db.transaction(DOCUMENT_STORE, "readonly");
    const index = tx.objectStore(DOCUMENT_STORE).index(usePushableIndex ? PUSHABLE_LWT_INDEX : "collectionLwtId");
    const range = usePushableIndex ? IDBKeyRange.bound([this.name, 1, fromLwt, fromId], [this.name, 1, Number.MAX_SAFE_INTEGER, "\uFFFF"], true, false) : IDBKeyRange.bound([this.name, fromLwt, fromId], [this.name, Number.MAX_SAFE_INTEGER, "\uFFFF"], true, false);
    const documents = [];
    let nextCheckpoint = checkpoint || null;
    let scanned = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || documents.length >= limit || scanned >= scanLimit) {
        return false;
      }
      scanned += 1;
      const record = cursor.value;
      nextCheckpoint = { lwt: record.lwt, id: record.id };
      if (!documentMatchesReplicationOrigin(record.doc, excludedOriginRole)) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    return {
      documents,
      checkpoint: nextCheckpoint,
      scanned,
      scanLimit,
      scanLimitReached: scanned >= scanLimit
    };
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
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    const strategy = primaryIds ? "primary-key" : schemaIndexPlan ? "schema-index" : canUseCollectionLwtQuery(query) ? "collection-lwt" : canUseBoundedCollectionCursor(query) ? "bounded-collection" : "all-documents";
    return {
      collection: this.name,
      selectorFields,
      sortFields,
      selectedIndex: schemaIndexPlan?.index || null,
      candidateIndex: schemaIndexPlan ? null : selectBestIndex(this.indexes, selectorFields, sortFields),
      strategy,
      indexed: strategy === "primary-key" || strategy === "schema-index" || strategy === "collection-lwt",
      schemaIndexed: Boolean(schemaIndexPlan),
      sortCovered: Boolean(schemaIndexPlan?.sortCovered),
      allDocumentsFallback: strategy === "all-documents"
    };
  }
  setQueryPerformancePolicy(policy = {}) {
    this.queryPerformancePolicy = {
      ...this.queryPerformancePolicy,
      rejectAllDocumentsFallback: policy.rejectAllDocumentsFallback === true
    };
  }
  getQueryPerformanceStats() {
    return cloneJson(this.queryPerformanceStats);
  }
  resetQueryPerformanceStats() {
    this.queryPerformanceStats = createQueryPerformanceStats();
  }
  recordAllDocumentsFallback(query = {}) {
    const plan = this.queryPlanFor(query);
    const fingerprint = queryFingerprintForStats(query);
    const fallback = {
      at: Date.now(),
      collection: this.name,
      fingerprint,
      selectorFields: plan.selectorFields || [],
      sortFields: plan.sortFields || [],
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
      rowsRead: 0
    };
    this.queryPerformanceStats.allDocumentsFallbackCalls += 1;
    this.queryPerformanceStats.lastAllDocumentsFallback = fallback;
    return fallback;
  }
  ensureSchemaIndexEntries() {
    if (!this.indexes.length) return Promise.resolve(0);
    if (!this.schemaIndexReady) {
      this.schemaIndexReady = this.rebuildMissingSchemaIndexEntries();
    }
    return this.schemaIndexReady;
  }
  async rebuildMissingSchemaIndexEntries() {
    const tx = this.db.transaction(DOCUMENT_STORE, "readwrite");
    const index = tx.objectStore(DOCUMENT_STORE).index("collection");
    const range = IDBKeyRange.only(this.name);
    let updated = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (record?.schemaIndexSignature !== this.indexSignature) {
        const next = {
          ...record,
          indexValues: indexValuesFor(this.indexes, record.doc || {}),
          schemaIndexSignature: this.indexSignature,
          schemaIndexEntries: schemaIndexEntriesFor(this.indexes, record.doc || {}, record.id, this.name)
        };
        cursor.update(next);
        updated += 1;
      }
      return true;
    });
    await idbTransactionDone(tx);
    return updated;
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
      let store = null;
      if (!db.objectStoreNames.contains(DOCUMENT_STORE)) {
        store = db.createObjectStore(DOCUMENT_STORE, { keyPath: ["collection", "id"] });
        store.createIndex("collection", "collection", { unique: false });
        store.createIndex("collectionLwtId", ["collection", "lwt", "id"], { unique: false });
      } else {
        store = request.transaction.objectStore(DOCUMENT_STORE);
      }
      if (store && !store.indexNames.contains(SCHEMA_INDEX_ENTRIES)) {
        store.createIndex(SCHEMA_INDEX_ENTRIES, SCHEMA_INDEX_ENTRIES, {
          unique: false,
          multiEntry: true
        });
      }
      if (store && !store.indexNames.contains(PUSHABLE_LWT_INDEX)) {
        store.createIndex(PUSHABLE_LWT_INDEX, ["collection", "pushable", "lwt", "id"], { unique: false });
        migrateStoredReplicationFlags(store);
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => {
        try {
          db.close();
        } catch {
        }
        globalThis.dispatchEvent?.(new CustomEvent("ctox-indexeddb-versionchange", {
          detail: { databaseName, oldVersion: db.version }
        }));
      };
      if (!finish(resolve, db)) {
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
function schedulePersistUnsyncedWriteCount(db) {
  if (unsyncedCountScheduled.has(db)) return;
  unsyncedCountScheduled.add(db);
  const timer = setTimeout(async () => {
    try {
      const summary = await countUnsyncedWrites(db);
      globalThis.localStorage?.setItem?.(
        `ctox.businessOs.unsyncedWrites.${db.name}`,
        JSON.stringify({ ...summary, capturedAtMs: Date.now() })
      );
      const recoveryKey = `ctox.businessOs.rxdbRecoveryJournal.${db.name}`;
      const recovery = JSON.parse(globalThis.localStorage?.getItem?.(recoveryKey) || "null");
      if (recovery) {
        if (summary.total === 0) {
          globalThis.localStorage?.removeItem?.(recoveryKey);
        } else {
          globalThis.localStorage?.setItem?.(recoveryKey, JSON.stringify({
            ...recovery,
            uniqueUnsyncedWrites: summary.total,
            unsyncedByCollection: summary.byCollection,
            updatedAtMs: Date.now()
          }));
        }
      }
    } catch {
    } finally {
      unsyncedCountScheduled.delete(db);
    }
  }, 1e3);
  timer?.unref?.();
}
async function countUnsyncedWrites(db) {
  const tx = db.transaction(DOCUMENT_STORE, "readonly");
  const store = tx.objectStore(DOCUMENT_STORE);
  const byCollection = {};
  let total = 0;
  await iterateCursor(store.openCursor(), (cursor) => {
    if (!cursor) return false;
    const record = cursor.value;
    if (Number(record?.pushable || 0) === 1) {
      total += 1;
      const collection = String(record.collection || "unknown");
      byCollection[collection] = (byCollection[collection] || 0) + 1;
    }
    return true;
  });
  await idbTransactionDone(tx);
  return { total, byCollection };
}
function documentId2(doc) {
  if (!doc || typeof doc !== "object") {
    return "";
  }
  return String(doc.id || doc._id || doc.document_id || doc.documentId || "");
}
function normalizeDocument(doc, lwt, replicationOrigin = null, previous = null) {
  const normalized = { ...doc };
  const id = documentId2(doc);
  if (!normalized.id) {
    normalized.id = id;
  }
  normalized._meta = { ...normalized._meta || {}, lwt };
  if (replicationOrigin?.role) {
    normalized._meta.ctoxHlc = normalized._meta.ctoxHlc || formatHybridLogicalClock({ physicalMs: lwt, nodeId: "native" });
    normalized._meta.ctoxReplicationOrigin = sanitizeReplicationOrigin(replicationOrigin);
  } else {
    const suppliedHlc = String(normalized._meta.ctoxHlc || "");
    const previousHlc = String(previous?._meta?.ctoxHlc || "");
    normalized._meta.ctoxHlc = suppliedHlc && suppliedHlc !== previousHlc ? suppliedHlc : nextHybridLogicalClock(previousHlc || suppliedHlc, {
      nowMs: Math.max(Date.now(), Number(lwt) || 0)
    });
    delete normalized._meta.ctoxReplicationOrigin;
  }
  normalized._deleted = Boolean(normalized._deleted);
  return normalized;
}
function storedRecordForWrite({ collection, id, doc, lwt, indexes, indexSignature, replicationOrigin = null, base = void 0, previous = null }) {
  const normalizedDoc = normalizeDocument(doc, lwt, replicationOrigin, previous?.doc || null);
  const replicationOriginRole = String(replicationOrigin?.role || "").slice(0, 64);
  const record = {
    collection,
    id,
    lwt,
    deleted: Boolean(doc._deleted),
    replicationOriginRole,
    pushable: replicationOriginRole ? 0 : 1,
    indexValues: indexValuesFor(indexes, doc),
    schemaIndexSignature: indexSignature,
    schemaIndexEntries: schemaIndexEntriesFor(indexes, doc, id, collection),
    doc: normalizedDoc
  };
  if (base !== void 0 && base !== null) {
    record.base = base;
  }
  return record;
}
function migrateStoredReplicationFlags(store) {
  const request = store.openCursor();
  request.onsuccess = () => {
    const cursor = request.result;
    if (!cursor) return;
    const next = normalizeStoredReplicationFlags(cursor.value);
    if (next !== cursor.value) {
      cursor.update(next);
    }
    cursor.continue();
  };
}
function normalizeStoredReplicationFlags(record) {
  if (!record || typeof record !== "object") return record;
  const role = String(record.doc?._meta?.ctoxReplicationOrigin?.role || "").slice(0, 64);
  const pushable = role ? 0 : 1;
  if (record.replicationOriginRole === role && record.pushable === pushable) {
    return record;
  }
  return {
    ...record,
    replicationOriginRole: role,
    pushable
  };
}
function shouldAcceptDocumentWrite(existingRecord, incomingLwt, replicationOrigin = null, incomingDocument = null, collectionName = "", conflictStrategy = "lww", deleteStrategy = "default") {
  if (!existingRecord) return true;
  const existingLwt = Number(existingRecord.lwt || existingRecord.doc?._meta?.lwt || 0);
  const nextLwt = Number(incomingLwt || 0);
  if (!Number.isFinite(existingLwt) || !Number.isFinite(nextLwt)) return true;
  if (replicationOrigin?.role) {
    if (collectionName === "business_commands" && isStaleReplicatedBusinessCommandState(existingRecord.doc, incomingDocument)) {
      return false;
    }
    if (collectionName === "business_commands" && isForwardReplicatedBusinessCommandState(existingRecord.doc, incomingDocument)) {
      return true;
    }
    const existingIsLocalWrite = !existingRecord.doc?._meta?.ctoxReplicationOrigin;
    if (!existingIsLocalWrite) return true;
    if (deleteStrategy === "final") {
      const incomingIsTombstone = Boolean(incomingDocument?._deleted);
      const existingIsTombstone = Boolean(existingRecord.doc?._deleted);
      if (incomingIsTombstone && !existingIsTombstone) return true;
      if (existingIsTombstone && !incomingIsTombstone) return false;
    }
    if (conflictStrategy === "field-merge") return nextLwt >= existingLwt;
    const localHlc = existingRecord.doc?._meta?.ctoxHlc;
    const masterHlc = incomingDocument?._meta?.ctoxHlc;
    if (parseHybridLogicalClock(localHlc) && parseHybridLogicalClock(masterHlc)) {
      if (isFutureHybridLogicalClock(localHlc)) return true;
      return compareHybridLogicalClocks(masterHlc, localHlc) >= 0;
    }
  }
  return nextLwt >= existingLwt;
}
var NATIVE_AUTHORITATIVE_COLLECTIONS = /* @__PURE__ */ new Set(["business_commands", "ctox_queue_tasks"]);
function lwwOverwriteConflict({
  previous,
  incomingDocument,
  collectionName = "",
  conflictStrategy = "lww",
  replicationOrigin = null
}) {
  if (!replicationOrigin?.role || !previous?.doc || !incomingDocument) return null;
  if (conflictStrategy === "field-merge") return null;
  if (NATIVE_AUTHORITATIVE_COLLECTIONS.has(collectionName)) return null;
  if (previous.replicationOriginRole || previous.doc?._meta?.ctoxReplicationOrigin) return null;
  if (previous.doc?._deleted) return null;
  if (incomingDocument._deleted) return null;
  if (masterAcknowledgesLocal(incomingDocument, previous.doc, collectionName)) return null;
  const localHlc = String(previous.doc?._meta?.ctoxHlc || "");
  const skewed = Boolean(localHlc) && isFutureHybridLogicalClock(localHlc);
  return {
    code: skewed ? "clock_skew_detected" : "structured_conflict_requires_resolution",
    conflictType: "update_vs_update",
    collection: collectionName,
    base: previous.base || null,
    local: previous.doc,
    master: incomingDocument,
    message: skewed ? "A local HLC is more than five minutes ahead of the native time reference; the master row won and the local update remains recoverable here." : "The master row won the whole-document LWW gate; the concurrent local update remains recoverable here.",
    ...skewed ? { clock: hybridLogicalClockStatus() } : {}
  };
}
function finalDeleteRejectedUpdateConflict({
  previous,
  incomingDocument,
  collectionName = "",
  deleteStrategy = "default",
  replicationOrigin = null
}) {
  if (deleteStrategy !== "final") return null;
  if (!replicationOrigin?.role || !previous?.doc || !incomingDocument) return null;
  if (NATIVE_AUTHORITATIVE_COLLECTIONS.has(collectionName)) return null;
  if (previous.replicationOriginRole || previous.doc?._meta?.ctoxReplicationOrigin) return null;
  if (!previous.doc?._deleted) return null;
  if (incomingDocument._deleted) return null;
  return {
    code: "structured_conflict_requires_resolution",
    conflictType: "delete_vs_update",
    collection: collectionName,
    base: previous.base || null,
    local: previous.doc,
    master: incomingDocument,
    message: "The local delete is authoritative (finalDelete); the concurrent master update remains recoverable here."
  };
}
function quarantineConflictRecord(error, collection, id, primaryPath = "id") {
  return {
    conflictId: `structured:${collection}:${id}`,
    code: "structured_conflict_requires_resolution",
    conflictType: "structured_field_conflict",
    collection,
    primaryPath,
    fields: Array.isArray(error?.fields) ? error.fields : [],
    base: error?.base ?? null,
    local: error?.local ?? null,
    master: error?.master ?? null,
    message: error?.message || `Concurrent structured edits to ${collection}/${id} require manual resolution.`
  };
}
function isForwardReplicatedBusinessCommandState(existingDocument, incomingDocument) {
  const rank = (document2) => {
    const status = String(document2?.terminal_status || document2?.status || "").trim().toLowerCase();
    if (document2?.execution_phase === "terminal" || ["completed", "failed", "rejected", "cancelled", "canceled", "blocked"].includes(status)) {
      return 2;
    }
    if (document2?.replication_phase === "native_observed" || ["accepted", "queued", "running", "in_progress"].includes(status)) {
      return 1;
    }
    return 0;
  };
  return rank(incomingDocument) > rank(existingDocument);
}
function isStaleReplicatedBusinessCommandState(existingDocument, incomingDocument) {
  const existingStatus = String(existingDocument?.status || "").trim().toLowerCase();
  const incomingStatus = String(incomingDocument?.status || "").trim().toLowerCase();
  if (!existingStatus || !incomingStatus || existingStatus === incomingStatus) return false;
  if (incomingStatus === "pending_sync" && existingStatus !== "pending_sync") return true;
  const terminal = /* @__PURE__ */ new Set([
    "completed",
    "failed",
    "rejected",
    "cancelled",
    "canceled",
    "blocked"
  ]);
  return terminal.has(existingStatus) && !terminal.has(incomingStatus);
}
function documentLwt(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0)
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
}
function isQuotaExceededError(error) {
  if (!error) return false;
  if (error.name === "QuotaExceededError") return true;
  if (typeof error.code === "number" && error.code === 22) return true;
  const message = String(error.message || "").toLowerCase();
  return message.includes("quota") || message.includes("storage full");
}
function dispatchStorageChange(databaseName, collection, success, replicationOrigin) {
  globalThis.dispatchEvent?.(new CustomEvent("ctox-rxdb-storage-change", {
    detail: {
      databaseName,
      collection,
      ids: Object.keys(success || {}),
      replicationOriginRole: String(replicationOrigin?.role || ""),
      atMs: Date.now()
    }
  }));
}
async function latestCollectionLwtInTransaction(store, collection) {
  const index = store.index("collectionLwtId");
  const range = IDBKeyRange.bound(
    [collection, 0, ""],
    [collection, Number.MAX_SAFE_INTEGER, "\uFFFF"],
    false,
    false
  );
  const record = await firstCursorValue(index.openCursor(range, "prev"));
  const latest = Number(record?.lwt || 0);
  return Number.isFinite(latest) && latest > 0 ? latest : 0;
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
function shouldUsePushableReplicationIndex(excludedOriginRole) {
  return excludedOriginRole === "ctox_instance";
}
function replicationScanLimit(limit) {
  const batchLimit = Number.isFinite(limit) && limit > 0 ? limit : 100;
  return Math.max(
    REPLICATION_MIN_SCAN_LIMIT,
    Math.min(REPLICATION_MAX_SCAN_LIMIT, Math.ceil(batchLimit * REPLICATION_SCAN_MULTIPLIER))
  );
}
function primaryPathFromSchema(schema = {}) {
  const primary = schema?.primaryKey;
  if (typeof primary === "string") return primary;
  if (primary?.key) return primary.key;
  return "id";
}
function normalizeSchemaIndexes(schema = {}, primaryPath = primaryPathFromSchema(schema)) {
  const indexes = Array.isArray(schema?.indexes) ? schema.indexes : [];
  const normalized = indexes.map((index) => normalizeSchemaIndexFields(index, primaryPath));
  if (!normalized.length) normalized.push(["_deleted", primaryPath]);
  normalized.push(["_meta.lwt", primaryPath]);
  if (Array.isArray(schema?.internalIndexes)) {
    for (const index of schema.internalIndexes) {
      normalized.push(normalizeSchemaIndexFields(index, primaryPath, { preservePrefix: true }));
    }
  }
  const seen = /* @__PURE__ */ new Set();
  return normalized.map((fields, position) => {
    const key = fields.join(",");
    if (seen.has(key)) return null;
    seen.add(key);
    return { name: `idx_${position}_${fields.join("_")}`, fields };
  }).filter(Boolean);
}
function normalizeSchemaIndexFields(index, primaryPath, { preservePrefix = false } = {}) {
  const fields = Array.isArray(index) ? index : [index];
  const normalizedFields = fields.map((field) => String(field || "").trim()).filter(Boolean);
  if (!normalizedFields.length) return ["_deleted", primaryPath];
  const next = normalizedFields.slice();
  if (!next.includes(primaryPath)) next.push(primaryPath);
  if (!preservePrefix && next[0] !== "_deleted") next.unshift("_deleted");
  return next;
}
function primaryKeyCandidateIds(query = {}, primaryPath = "id") {
  const selector = query?.selector || {};
  for (const field of ["id", "_id", primaryPath].filter(Boolean)) {
    if (!Object.prototype.hasOwnProperty.call(selector, field)) continue;
    const value = selector[field];
    if (value == null) return [];
    if (typeof value === "string" || typeof value === "number") {
      return [String(value)];
    }
    if (value && typeof value === "object" && !Array.isArray(value)) {
      if ("$eq" in value && value.$eq != null) return [String(value.$eq)];
      if ("$in" in value && Array.isArray(value.$in)) {
        return [...new Set(value.$in.filter((id) => id != null).map((id) => String(id)))];
      }
    }
    return null;
  }
  return null;
}
function indexValuesFor(indexes, doc) {
  const values = {};
  for (const index of indexes || []) {
    values[index.name] = index.fields.map((field) => valueAtPath3(doc, field));
  }
  return values;
}
function schemaIndexEntriesFor(indexes, doc, id, collection) {
  const entries = [];
  for (const index of indexes || []) {
    const components = [];
    let usable = true;
    for (const field of index.fields) {
      const encoded = encodeIndexValue(valueAtPath3(doc, field));
      if (!encoded) {
        usable = false;
        break;
      }
      components.push(...encoded);
    }
    if (usable) {
      entries.push([collection, index.name, ...components, String(id || documentId2(doc))]);
    }
  }
  return entries;
}
function schemaIndexSignature(indexes = []) {
  return indexes.map((index) => `${index.name}:${index.fields.join(",")}`).join("|");
}
function selectBestIndex(indexes, selectorFields = [], sortFields = []) {
  const wanted = [...selectorFields, ...sortFields].filter(Boolean);
  if (!wanted.length) return null;
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    let score = 0;
    const fields = index.fields[0] === "_deleted" ? index.fields.slice(1) : index.fields;
    for (const field of fields) {
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
function schemaIndexQueryPlanFor(query = {}, indexes = []) {
  const selector = query?.selector || {};
  if (Object.keys(selector).some((field) => field.startsWith("$"))) return null;
  const sortEntries = normalizeSortEntries(query?.sort);
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    const plan = schemaIndexPlanForIndex(index, selector, sortEntries, query);
    if (!plan) continue;
    const score = plan.constrainedFields * 10 + (plan.sortCovered ? 4 : 0) + (plan.canStopAtLimit ? 2 : 0) - Math.max(0, plan.ranges.length - 1);
    if (score > bestScore) {
      best = plan;
      bestScore = score;
    }
  }
  return best;
}
function schemaIndexPlanForIndex(index, selector, sortEntries, query) {
  if (!index?.fields?.length) return null;
  let ranges = [{ lower: [], upper: [], upperComplete: false }];
  let constrainedFields = 0;
  let lastEqualityFieldIndex = -1;
  let rangeFieldIndex = -1;
  let stoppedAtFieldIndex = index.fields.length;
  for (let fieldIndex = 0; fieldIndex < index.fields.length; fieldIndex += 1) {
    const field = index.fields[fieldIndex];
    const constraint = field === "_deleted" ? { kind: "eq", values: [false], implicit: true } : selectorConstraintFor(selector, field);
    if (constraint.kind === "none") {
      stoppedAtFieldIndex = fieldIndex;
      break;
    }
    if (constraint.kind === "unsupported") return null;
    if (constraint.kind === "eq") {
      const encodedValues = constraint.values.map((value) => encodeIndexValue(value)).filter(Boolean);
      if (!encodedValues.length || encodedValues.length !== constraint.values.length) return null;
      if (encodedValues.length > 32) return null;
      ranges = ranges.flatMap((range) => encodedValues.map((encoded) => ({
        lower: [...range.lower, ...encoded],
        upper: [...range.upper, ...encoded],
        upperComplete: false
      })));
      if (!constraint.implicit) constrainedFields += 1;
      lastEqualityFieldIndex = fieldIndex;
      continue;
    }
    if (constraint.kind === "range") {
      const lowerEncoded = constraint.lower !== void 0 ? encodeIndexValue(constraint.lower) : null;
      const upperEncoded = constraint.upper !== void 0 ? encodeIndexValue(constraint.upper) : null;
      if (constraint.lower !== void 0 && !lowerEncoded || constraint.upper !== void 0 && !upperEncoded) {
        return null;
      }
      ranges = ranges.map((range) => ({
        lower: lowerEncoded ? [...range.lower, ...lowerEncoded, ...constraint.lowerOpen ? [INDEX_HIGH_KEY] : []] : [...range.lower],
        upper: upperEncoded ? [...range.upper, ...upperEncoded, ...constraint.upperOpen ? [] : [INDEX_HIGH_KEY]] : [...range.upper, INDEX_HIGH_KEY],
        upperComplete: true
      }));
      constrainedFields += 1;
      rangeFieldIndex = fieldIndex;
      stoppedAtFieldIndex = fieldIndex + 1;
      break;
    }
  }
  const hasSelectorConstraint = constrainedFields > 0;
  const orderStart = Math.max(
    0,
    rangeFieldIndex >= 0 ? rangeFieldIndex : lastEqualityFieldIndex + 1
  );
  const sortCovered = isSortCoveredByIndex(index.fields, orderStart, sortEntries);
  const hasSortOnlyPlan = !hasSelectorConstraint && sortEntries.length > 0 && sortCovered && Number.isFinite(query?.limit);
  if (!hasSelectorConstraint && !hasSortOnlyPlan) return null;
  if (sortEntries.length && !sortCovered && !hasSelectorConstraint) return null;
  ranges = ranges.map((range) => ({
    lower: range.lower,
    upper: range.upperComplete || range.upper.length > range.lower.length ? range.upper : [...range.upper, INDEX_HIGH_KEY]
  }));
  const direction = sortCovered && sortEntries[0]?.direction === "desc" ? "prev" : "next";
  return {
    index,
    ranges,
    direction,
    sortCovered,
    canStopAtLimit: sortCovered,
    constrainedFields,
    stoppedAtFieldIndex
  };
}
function selectorConstraintFor(selector, field) {
  if (!Object.prototype.hasOwnProperty.call(selector, field)) return { kind: "none" };
  const value = selector[field];
  if (isIndexComparableValue(value)) return { kind: "eq", values: [value] };
  if (!value || typeof value !== "object" || Array.isArray(value)) return { kind: "unsupported" };
  const keys = Object.keys(value);
  if (keys.length === 1 && keys[0] === "$eq" && isIndexComparableValue(value.$eq)) {
    return { kind: "eq", values: [value.$eq] };
  }
  if (keys.length === 1 && keys[0] === "$in" && Array.isArray(value.$in)) {
    const values = [...new Set(value.$in.filter(isIndexComparableValue))];
    return values.length === value.$in.length ? { kind: "eq", values } : { kind: "unsupported" };
  }
  const rangeKeys = /* @__PURE__ */ new Set(["$gt", "$gte", "$lt", "$lte"]);
  if (keys.length && keys.every((key) => rangeKeys.has(key))) {
    const lower = "$gt" in value ? value.$gt : value.$gte;
    const upper = "$lt" in value ? value.$lt : value.$lte;
    if (lower !== void 0 && !isIndexComparableValue(lower) || upper !== void 0 && !isIndexComparableValue(upper)) {
      return { kind: "unsupported" };
    }
    return {
      kind: "range",
      lower,
      upper,
      lowerOpen: "$gt" in value,
      upperOpen: "$lt" in value
    };
  }
  return { kind: "unsupported" };
}
function isSortCoveredByIndex(indexFields, orderStart, sortEntries) {
  if (!sortEntries.length) return true;
  const directions = new Set(sortEntries.map((entry) => entry.direction));
  if (directions.size > 1) return false;
  const orderedFields = indexFields.slice(orderStart).filter((field) => field !== "_deleted");
  return sortEntries.every((entry, offset) => orderedFields[offset] === entry.field);
}
function normalizeSortEntries(sort = []) {
  if (!sort) return [];
  const entries = typeof sort === "string" ? [sort] : Array.isArray(sort) ? sort : [];
  return entries.map((entry) => {
    if (typeof entry === "string") return { field: entry, direction: "asc" };
    const [field, rawDirection] = Object.entries(entry || {})[0] || [];
    if (!field) return null;
    const direction = rawDirection === -1 || String(rawDirection).toLowerCase() === "desc" ? "desc" : "asc";
    return { field, direction };
  }).filter(Boolean);
}
function encodeIndexValue(value) {
  if (typeof value === "boolean") return ["b", value ? 1 : 0];
  if (typeof value === "number" && Number.isFinite(value)) return ["n", value];
  if (typeof value === "string") return ["s", value];
  if (value instanceof Date && Number.isFinite(value.getTime())) return ["n", value.getTime()];
  return null;
}
function isIndexComparableValue(value) {
  return Boolean(encodeIndexValue(value));
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
function canUseBoundedCollectionCursor(query = {}) {
  if (!Number.isFinite(query?.limit)) return false;
  return normalizeSortFields(query?.sort).length === 0;
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
function createQueryPerformanceStats() {
  return {
    allDocumentsCalls: 0,
    allDocumentsRowsRead: 0,
    allDocumentsFallbackCalls: 0,
    allDocumentsFallbackRowsRead: 0,
    lastAllDocumentsRowsRead: 0,
    lastAllDocumentsFallback: null
  };
}
function createAllDocumentsFallbackError(collection, query, fallback) {
  const error = new Error(`IndexedDB query for ${collection} would use allDocuments() fallback.`);
  error.name = "CtoxIndexedDbQueryPlanError";
  error.code = "CTOX_INDEXEDDB_ALL_DOCUMENTS_FALLBACK";
  error.collection = collection;
  error.query = query;
  error.fallback = fallback;
  return error;
}
function queryFingerprintForStats(query = {}) {
  try {
    return JSON.stringify({
      selector: query?.selector || {},
      sort: query?.sort || [],
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null
    });
  } catch {
    return String(Date.now());
  }
}
function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}
function normalizeSortFields(sort = []) {
  if (!Array.isArray(sort)) return typeof sort === "string" ? [sort] : [];
  return sort.map((entry) => {
    if (typeof entry === "string") return entry;
    return Object.keys(entry || {})[0] || "";
  }).filter(Boolean);
}
function valueAtPath3(doc, path) {
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
  createAllDocumentsFallbackError,
  createQueryPerformanceStats,
  documentMatchesReplicationOrigin,
  indexValuesFor,
  lwwOverwriteConflict,
  finalDeleteRejectedUpdateConflict,
  normalizeDocument,
  normalizeStoredReplicationFlags,
  normalizeSchemaIndexes,
  canUseBoundedCollectionCursor,
  encodeIndexValue,
  primaryKeyCandidateIds,
  replicationScanLimit,
  schemaIndexEntriesFor,
  schemaIndexQueryPlanFor,
  selectBestIndex,
  shouldUsePushableReplicationIndex,
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
var SEND_BUFFER_STALL_TIMEOUT_MS = 3e4;
var MAX_PEER_SEND_QUEUE_FRAMES = 1024;
var MAX_PEER_SEND_QUEUE_BYTES = 16 * 1024 * 1024;
var FAIR_SEND_SCHEDULE = ["high", "high", "high", "high", "normal", "normal", "low"];
var MAX_SERIALIZED_FRAME_BYTES = 16384;
var FRAME_ACK_TIMEOUT_MS = 3e4;
var FRAME_RESUME_TIMEOUT_MS = 1e3;
var COMPLETED_FRAME_ACK_TTL_MS = 6e4;
var MAX_GLOBAL_RTC_PEER_CONNECTIONS = 64;
var RTC_CONNECTION_QUEUE_TIMEOUT_MS = 45e3;
var RTC_HANDSHAKE_TIMEOUT_MS = 6e4;
var GLOBAL_RTC_CONNECTION_POOL_KEY = /* @__PURE__ */ Symbol.for("ctox.rxdb.webrtc-rtc-pool.v1");
var RECENT_RTC_EVENT_LIMIT = 40;
var TERMINAL_SIGNALING_REJECTION_CODES = /* @__PURE__ */ new Set([
  "protocol_missing",
  "protocol_mismatch",
  "instance_mismatch",
  "peer_revoked",
  "role_mismatch",
  "token_invalid",
  "token_signature_invalid",
  "credentials_revoked"
]);
var RETRYABLE_SIGNALING_REJECTION_CODES = /* @__PURE__ */ new Set([
  "control_plane_token_expired",
  "temporary_unavailable"
]);
var ICE_DISCONNECTED_GRACE_MS = 3e4;
var SIGNALING_RECONNECT_BASE_MS = 1e3;
var SIGNALING_RECONNECT_MAX_MS = 3e4;
var ICE_SERVERS_REFRESH_SKEW_MS = 12e4;
var ICE_SERVERS_REFRESH_MIN_INTERVAL_MS = 6e4;
var TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS = 250;
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
    iceServersRefreshUrl = "",
    refreshIceServers = null,
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
      iceServersRefreshUrl,
      refreshIceServers: typeof refreshIceServers === "function" ? refreshIceServers : null,
      storageToken,
      expectedNativePeerId,
      protocolPayload,
      requestHandlers
    };
    this.iceServersRefreshInFlight = null;
    this.lastIceServersRefreshAtMs = 0;
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
      backpressureStallCount: 0,
      queuedFrames: 0,
      sentScheduledFrames: 0,
      priorityQueueDepth: 0,
      highPriorityQueueDepth: 0,
      normalPriorityQueueDepth: 0,
      lowPriorityQueueDepth: 0,
      queuedBytes: 0,
      rejectedFrames: 0,
      oldestQueuedAgeMs: 0,
      turnCredentialExpiresAtMs: turnCredentialExpiryMs(iceServers),
      lastSendPriority: "normal",
      lastAckLagMs: 0,
      lastBufferedAmount: 0,
      updatedAtMs: Date.now()
    };
    this.lastControlPlaneError = null;
    this.recentConnectionEvents = [];
    this.recentMessages = [];
    this.transportStatusEmitTimer = null;
    this.lastTransportStatusEmitAtMs = 0;
    this.connectionRequests = /* @__PURE__ */ new Map();
    this.forceInitiatorPeers = /* @__PURE__ */ new Set();
    this.closed = false;
    this.signalingReconnectTimer = null;
    this.disconnectedGraceTimers = /* @__PURE__ */ new Map();
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
    const delay4 = this.signalingReconnectDelayMs;
    this.signalingReconnectDelayMs = Math.min(delay4 * 2, SIGNALING_RECONNECT_MAX_MS);
    this.signalingReconnectTimer = setTimeout(() => {
      this.signalingReconnectTimer = null;
      if (this.closed) return;
      this.events.emit("signaling-reconnect", { delayMs: delay4 });
      this.connect();
    }, delay4);
  }
  close() {
    this.closed = true;
    if (this.signalingReconnectTimer) {
      clearTimeout(this.signalingReconnectTimer);
      this.signalingReconnectTimer = null;
    }
    if (this.transportStatusEmitTimer) {
      clearTimeout(this.transportStatusEmitTimer);
      this.transportStatusEmitTimer = null;
    }
    for (const timer of this.disconnectedGraceTimers.values()) clearTimeout(timer);
    this.disconnectedGraceTimers.clear();
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
    return this.enqueueSendFrame(connection, {
      payload,
      text,
      inline: encodedSize(text) <= MAX_INLINE_FRAME_BYTES,
      priority: classifySendPriority(payload, text)
    });
  }
  enqueueSendFrame(connection, item) {
    if (!connection.sendQueue) {
      connection.sendQueue = createSendQueue();
    }
    const queue = connection.sendQueue;
    const itemBytes = encodedSize(item.text);
    const queuedFrames = queue.high.length + queue.normal.length + queue.low.length;
    if (queuedFrames >= MAX_PEER_SEND_QUEUE_FRAMES || queue.queuedBytes + itemBytes > MAX_PEER_SEND_QUEUE_BYTES) {
      this.recordTransportStatus({ rejectedFrames: this.transportStats.rejectedFrames + 1 });
      this.events.emit("error", {
        code: "ctox_webrtc_send_queue_budget_exceeded",
        peerId: connection.remotePeerId,
        queuedFrames,
        queuedBytes: queue.queuedBytes,
        maxFrames: MAX_PEER_SEND_QUEUE_FRAMES,
        maxBytes: MAX_PEER_SEND_QUEUE_BYTES
      });
      this.removeConnection(connection.remotePeerId, "send-queue-budget-exceeded");
      return false;
    }
    queue[item.priority].push({
      ...item,
      byteLength: itemBytes,
      queuedAtMs: Date.now(),
      sequence: queue.nextSequence++
    });
    queue.queuedBytes += itemBytes;
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
    return true;
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
          await this.waitForSendBuffer(connection.channel, connection);
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
    const chunks = splitFrameChunks(text, transferId);
    const totalFrames = chunks.length;
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
            await this.waitForSendBuffer(channel, connection);
            if (this.connections.get(connection.remotePeerId) !== connection || channel?.readyState !== "open") {
              throw createPeerClosedError(connection.remotePeerId, "frame-send-channel-closed");
            }
            const chunkFrame = {
              ctoxFrame: CTOX_FRAME_PROTOCOL,
              kind: "chunk",
              transferId,
              attempt,
              seq,
              data: chunks[seq]
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
      const result2 = await Promise.race([
        wrapped,
        delay(50).then(() => null)
      ]);
      if (result2) {
        if (result2.ok) return result2.value;
        throw result2.error;
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
      await this.waitForSendBuffer(connection.channel, connection);
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
  waitForSendBuffer(channel, connection = null) {
    if (Number(channel.bufferedAmount || 0) <= SEND_BUFFER_HIGH_WATER) {
      return Promise.resolve();
    }
    this.recordTransportStatus({
      backpressureWaitCount: this.transportStats.backpressureWaitCount + 1,
      lastBufferedAmount: Number(channel.bufferedAmount || 0)
    });
    return new Promise((resolve, reject) => {
      const previousThreshold = channel.bufferedAmountLowThreshold;
      channel.bufferedAmountLowThreshold = SEND_BUFFER_LOW_WATER;
      let timer = null;
      const cleanup = () => {
        channel.removeEventListener?.("bufferedamountlow", done);
        channel.bufferedAmountLowThreshold = previousThreshold || 0;
        if (timer) clearTimeout(timer);
      };
      const done = () => {
        cleanup();
        resolve();
      };
      channel.addEventListener?.("bufferedamountlow", done, { once: true });
      timer = setTimeout(() => {
        cleanup();
        this.recordTransportStatus({
          backpressureStallCount: this.transportStats.backpressureStallCount + 1,
          rejectedFrames: this.transportStats.rejectedFrames + 1
        });
        const error = new Error("WebRTC send buffer remained above the high-water mark.");
        error.code = "ctox_webrtc_send_buffer_stalled";
        error.retryable = true;
        error.peerId = connection?.remotePeerId || null;
        this.events.emit("error", error);
        if (connection?.remotePeerId) {
          this.removeConnection(
            connection.remotePeerId,
            "send-buffer-stalled",
            error,
            { reconnect: false }
          );
        }
        reject(error);
      }, SEND_BUFFER_STALL_TIMEOUT_MS);
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
      if (message.yourPeerId && message.yourPeerId !== this.options.clientId) {
        this.options.clientId = String(message.yourPeerId);
      }
      if (message.type === "joined") {
        this.signalingReconnectDelayMs = SIGNALING_RECONNECT_BASE_MS;
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
      if (error.retryable === false) {
        this.closed = true;
        if (this.signalingReconnectTimer) clearTimeout(this.signalingReconnectTimer);
        this.signalingReconnectTimer = null;
        this.rejectAllPending(error);
      }
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
  // SYNC-30: whether a (re)connect should first refresh the ICE server list.
  // True only when a refresh callback exists, the current TURN credential is
  // within the skew of its expiry, and we have not just attempted a refresh —
  // the min-interval guard is what lets a deferred connect re-drive exactly
  // once (a failed refresh advances `lastIceServersRefreshAtMs`, so the retry
  // sees `false` here and proceeds with the existing servers instead of
  // deferring forever).
  shouldRefreshIceServersBeforeConnect() {
    if (typeof this.options.refreshIceServers !== "function") return false;
    if (!this.turnCredentialsNearExpiry(ICE_SERVERS_REFRESH_SKEW_MS)) return false;
    return Date.now() - this.lastIceServersRefreshAtMs >= ICE_SERVERS_REFRESH_MIN_INTERVAL_MS;
  }
  turnCredentialsNearExpiry(skewMs = 0) {
    const expiresAt = Number(this.transportStats.turnCredentialExpiresAtMs || 0);
    if (!(expiresAt > 0)) return false;
    return Date.now() >= expiresAt - Math.max(0, Number(skewMs) || 0);
  }
  // Refresh the ICE server list (fresh minted TURN credentials) from the
  // shell-supplied control-plane callback. Deduplicates concurrent calls; on
  // failure keeps the existing servers so sync degrades rather than wedges.
  refreshIceServersIfExpiring() {
    if (this.iceServersRefreshInFlight) return this.iceServersRefreshInFlight;
    if (typeof this.options.refreshIceServers !== "function") return Promise.resolve(false);
    if (!this.turnCredentialsNearExpiry(ICE_SERVERS_REFRESH_SKEW_MS)) return Promise.resolve(false);
    const attempt = (async () => {
      try {
        const fresh = await this.options.refreshIceServers();
        if (Array.isArray(fresh) && fresh.length) {
          this.options.iceServers = fresh;
          this.recordTransportStatus({ turnCredentialExpiresAtMs: turnCredentialExpiryMs(fresh) });
          this.events.emit("ice-servers-refreshed", {
            iceServersConfigured: fresh.length,
            turnCredentialExpiresAtMs: turnCredentialExpiryMs(fresh)
          });
          return true;
        }
        this.events.emit("error", { code: "ctox_ice_servers_refresh_empty", phase: "signaling-control-plane" });
        return false;
      } catch (error) {
        this.events.emit("error", {
          code: "ctox_ice_servers_refresh_failed",
          phase: "signaling-control-plane",
          message: error?.message || String(error)
        });
        return false;
      } finally {
        this.lastIceServersRefreshAtMs = Date.now();
        this.iceServersRefreshInFlight = null;
      }
    })();
    this.iceServersRefreshInFlight = attempt;
    return attempt;
  }
  // Defer a peer (re)connect until an ICE refresh completes, then re-drive
  // exactly once. A failed refresh still re-drives (with the existing servers),
  // so the connection is never wedged waiting on the control plane.
  deferConnectForIceRefresh(remotePeerId) {
    this.refreshIceServersIfExpiring().catch(() => {
    }).then(() => {
      if (this.closed || this.connections.has(remotePeerId)) return;
      if (!this.shouldConnectToRemotePeer(remotePeerId)) return;
      try {
        this.ensureConnection(remotePeerId);
      } catch (error) {
        this.events.emit("error", normalizePeerSignalError(error, remotePeerId));
      }
    });
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
    if (this.shouldRefreshIceServersBeforeConnect()) {
      this.deferConnectForIceRefresh(remotePeerId);
      return void 0;
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
      if (state === "disconnected") {
        const existing = this.disconnectedGraceTimers.get(remotePeerId);
        if (existing) clearTimeout(existing);
        this.disconnectedGraceTimers.set(remotePeerId, setTimeout(() => {
          this.disconnectedGraceTimers.delete(remotePeerId);
          const live = this.connections.get(remotePeerId);
          const liveState = live?.peer?.connectionState || "";
          if (live === connection && ["disconnected", "failed"].includes(liveState)) {
            this.removeConnection(remotePeerId, "peer-disconnected-grace-expired");
          }
        }, ICE_DISCONNECTED_GRACE_MS));
        return;
      }
      const graceTimer = this.disconnectedGraceTimers.get(remotePeerId);
      if (graceTimer) {
        clearTimeout(graceTimer);
        this.disconnectedGraceTimers.delete(remotePeerId);
      }
      if (["closed", "failed"].includes(state)) {
        this.removeConnection(remotePeerId, `peer-${state}`);
      } else if (state === "connected") {
        updateSelectedCandidatePair(connection).then(() => {
          this.recordConnectionEvent(connection, "selected-candidate-pair", {
            localCandidateType: connection.signalStats.selectedLocalCandidateType,
            remoteCandidateType: connection.signalStats.selectedRemoteCandidateType,
            protocol: connection.signalStats.selectedCandidateProtocol
          });
        }).catch(() => {
        });
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
    if (payload?.id === CTOX_PRESENCE_RPC.streamId) {
      this.events.emit("presence", {
        peerId,
        entries: Array.isArray(payload?.result?.entries) ? payload.result.entries : []
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
        contiguousSeq: -1,
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
        this.send(peerId, {
          ctoxFrame: CTOX_FRAME_PROTOCOL,
          kind: "ack",
          transferId: transferId2,
          ackSeq: Number(entry2.contiguousSeq ?? -1),
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
    const contiguousSeq = recordReceivedFrame(entry, seq, String(payload.data || ""));
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
  removeConnection(remotePeerId, reason = "closed", pendingError = null, { reconnect = true } = {}) {
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
    this.rejectPendingForPeer(peerId, pendingError || createPeerClosedError(peerId, reason));
    this.events.emit("peer-close", { peerId, reason });
    if (reconnect && reason !== "peer-close") {
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
    const liveNativePeerIds = new Set(
      descriptors.filter((descriptor) => descriptor?.peerId && this.isNativePeerCandidate(descriptor.peerId, descriptor)).map((descriptor) => descriptor.peerId)
    );
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
  getTransportStatus({ includeDiagnostics = false } = {}) {
    const base = {
      ...this.transportStats,
      collection: collectionNameFromTopic(this.options.room),
      topic: this.options.room,
      activePeerCount: this.connections.size,
      pendingAcks: this.pendingFrameAcks.size,
      pendingRequests: this.pending.size,
      incomingTransfers: this.incomingFrames.size,
      completedAckCacheSize: this.completedFrameAcks.size,
      connectionCount: this.connections.size,
      rtcConnectionPool: rtcPeerConnectionPoolCounters()
    };
    if (!includeDiagnostics) return base;
    return {
      ...base,
      pendingRequestMethods: [...this.pending.values()].map((pending) => pending.method || "").filter(Boolean).slice(-20),
      observedRequestMethods: [...this.observedRequests.keys()].map((key) => String(key).split("|").slice(1).join("|")).slice(-20),
      rtcConnectionPool: rtcPeerConnectionPoolSnapshot(),
      rtcConnections: [...this.connections.values()].map((connection) => peerConnectionSnapshot(connection)),
      recentRtcEvents: this.recentConnectionEvents.slice(-RECENT_RTC_EVENT_LIMIT),
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
    this.emitTransportStatus({ immediate: true });
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
    this.emitTransportStatus();
  }
  recordTransportStatus(patch = {}) {
    Object.assign(this.transportStats, patch, { updatedAtMs: Date.now() });
    this.emitTransportStatus();
  }
  emitTransportStatus({ immediate = false } = {}) {
    if (this.closed) return;
    const now = Date.now();
    const elapsed = now - this.lastTransportStatusEmitAtMs;
    if (immediate || elapsed >= TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS) {
      if (this.transportStatusEmitTimer) {
        clearTimeout(this.transportStatusEmitTimer);
        this.transportStatusEmitTimer = null;
      }
      this.lastTransportStatusEmitAtMs = now;
      this.events.emit("transport-status", this.getTransportStatus());
      return;
    }
    if (this.transportStatusEmitTimer) return;
    this.transportStatusEmitTimer = setTimeout(() => {
      this.transportStatusEmitTimer = null;
      if (this.closed) return;
      this.lastTransportStatusEmitAtMs = Date.now();
      this.events.emit("transport-status", this.getTransportStatus());
    }, Math.max(0, TRANSPORT_STATUS_EMIT_MIN_INTERVAL_MS - elapsed));
  }
  refreshSendQueueStatus(connection = null) {
    let high = 0;
    let normal = 0;
    let low = 0;
    let queuedBytes = 0;
    let oldestQueuedAtMs = 0;
    const connections = connection ? [connection] : this.connections.values();
    for (const entry of connections) {
      const queue = entry?.sendQueue;
      if (!queue) continue;
      high += queue.high.length;
      normal += queue.normal.length;
      low += queue.low.length;
      queuedBytes += Number(queue.queuedBytes || 0);
      for (const item of [...queue.high, ...queue.normal, ...queue.low]) {
        const queuedAtMs = Number(item?.queuedAtMs || 0);
        if (queuedAtMs > 0 && (oldestQueuedAtMs === 0 || queuedAtMs < oldestQueuedAtMs)) {
          oldestQueuedAtMs = queuedAtMs;
        }
      }
    }
    this.recordTransportStatus({
      priorityQueueDepth: high + normal + low,
      highPriorityQueueDepth: high,
      normalPriorityQueueDepth: normal,
      lowPriorityQueueDepth: low,
      queuedBytes,
      oldestQueuedAgeMs: oldestQueuedAtMs > 0 ? Math.max(0, Date.now() - oldestQueuedAtMs) : 0
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
    const retryable = RETRYABLE_SIGNALING_REJECTION_CODES.has(code) ? true : TERMINAL_SIGNALING_REJECTION_CODES.has(code) ? false : false;
    return {
      name: "CtoxSignalingControlPlaneError",
      type: payload.type,
      scope: payload.scope,
      code,
      phase: "signaling-control-plane",
      severity: "error",
      retryable,
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
function rtcPeerConnectionPoolCounters() {
  const pool = getRtcPeerConnectionPool();
  const active = pool.active.size;
  const queued = pool.queue.length;
  const activeCritical = activeCriticalRtcPeerConnectionCount(pool);
  const queuedCritical = queuedCriticalRtcPeerConnectionNames(pool).length;
  return {
    maxActive: pool.maxActive,
    active,
    queued,
    activeCritical,
    queuedCritical,
    maxConnections: pool.maxActive,
    activeConnections: active,
    queuedConnections: queued,
    criticalActiveConnections: activeCritical,
    criticalQueuedConnections: queuedCritical
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
    selectedLocalCandidateType: "",
    selectedRemoteCandidateType: "",
    selectedCandidateProtocol: "",
    lastSignalAtMs: 0
  };
}
async function updateSelectedCandidatePair(connection) {
  const report = await connection?.peer?.getStats?.();
  if (!report) return;
  const values = [];
  report.forEach?.((value) => values.push(value));
  let pair = null;
  const transport = values.find((entry) => entry?.type === "transport" && entry.selectedCandidatePairId);
  if (transport) pair = values.find((entry) => entry.id === transport.selectedCandidatePairId) || null;
  if (!pair) {
    pair = values.find((entry) => entry?.type === "candidate-pair" && entry.state === "succeeded" && (entry.nominated || entry.selected)) || null;
  }
  if (!pair) return;
  const local = values.find((entry) => entry.id === pair.localCandidateId) || null;
  const remote = values.find((entry) => entry.id === pair.remoteCandidateId) || null;
  connection.signalStats.selectedLocalCandidateType = local?.candidateType || "";
  connection.signalStats.selectedRemoteCandidateType = remote?.candidateType || "";
  connection.signalStats.selectedCandidateProtocol = local?.protocol || remote?.protocol || "";
}
function turnCredentialExpiryMs(iceServers = []) {
  const expiries = [];
  for (const server of Array.isArray(iceServers) ? iceServers : []) {
    const urls = Array.isArray(server?.urls) ? server.urls : [server?.urls];
    if (!urls.some((url) => /^turns?:/i.test(String(url || "")))) continue;
    const expirySeconds = Number.parseInt(String(server?.username || "").split(":")[0], 10);
    if (Number.isFinite(expirySeconds) && expirySeconds > 0) expiries.push(expirySeconds * 1e3);
  }
  return expiries.length ? Math.min(...expiries) : 0;
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
  const issuedAt = Number(url.searchParams.get("token_iat") || 0);
  const expiresAt = Number(url.searchParams.get("token_exp") || 0);
  if (issuedAt > 0 && expiresAt > issuedAt) {
    const ttlSeconds = expiresAt - issuedAt;
    const now = Math.floor(Date.now() / 1e3);
    url.searchParams.set("token_iat", String(now));
    url.searchParams.set("token_exp", String(now + ttlSeconds));
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
  return utf8ByteLength(String(value || ""));
}
function utf8ByteLength(text) {
  let bytes = 0;
  const value = String(text || "");
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 127) {
      bytes += 1;
    } else if (code <= 2047) {
      bytes += 2;
    } else if (code >= 55296 && code <= 56319) {
      const next = index + 1 < value.length ? value.charCodeAt(index + 1) : 0;
      if (next >= 56320 && next <= 57343) {
        bytes += 4;
        index += 1;
      } else {
        bytes += 3;
      }
    } else {
      bytes += 3;
    }
  }
  return bytes;
}
function splitFrameChunks(text, transferId) {
  const envelope = JSON.stringify({
    ctoxFrame: CTOX_FRAME_PROTOCOL,
    kind: "chunk",
    transferId,
    attempt: Number.MAX_SAFE_INTEGER,
    seq: Number.MAX_SAFE_INTEGER,
    data: ""
  });
  const overhead = encodedSize(envelope);
  const budget = Math.max(1, Math.min(MAX_CHUNK_CHARS, MAX_SERIALIZED_FRAME_BYTES - overhead - 64));
  const value = String(text || "");
  if (!value) return [""];
  const chunks = [];
  let cur = "";
  let curEscaped = 0;
  for (const ch of value) {
    const chEscaped = jsonEscapedCharLen(ch);
    if (curEscaped + chEscaped > budget && cur) {
      chunks.push(cur);
      cur = "";
      curEscaped = 0;
    }
    cur += ch;
    curEscaped += chEscaped;
  }
  if (cur || chunks.length === 0) chunks.push(cur);
  return chunks;
}
var webrtcNativeTestInternals = Object.freeze({
  splitFrameChunks,
  jsonEscapedCharLen,
  encodedSize,
  utf8ByteLength,
  recordReceivedFrame,
  MAX_SERIALIZED_FRAME_BYTES
});
function jsonEscapedCharLen(ch) {
  const code = ch.codePointAt(0);
  if (ch === '"' || ch === "\\") return 2;
  if (code === 8 || code === 9 || code === 10 || code === 12 || code === 13) return 2;
  if (code < 32) return 6;
  if (code <= 127) return 1;
  if (code <= 2047) return 2;
  if (code >= 55296 && code <= 57343) return 6;
  if (code <= 65535) return 3;
  return 4;
}
function recordReceivedFrame(entry, seq, data) {
  const hadFrame = entry.received.has(seq);
  entry.received.set(seq, data);
  if (!hadFrame && seq === Number(entry.contiguousSeq ?? -1) + 1) {
    while (entry.contiguousSeq + 1 < entry.totalFrames && entry.received.has(entry.contiguousSeq + 1)) {
      entry.contiguousSeq += 1;
    }
  }
  return Number(entry.contiguousSeq ?? -1);
}
function createSendQueue() {
  return {
    high: [],
    normal: [],
    low: [],
    draining: false,
    nextSequence: 0,
    queuedBytes: 0,
    scheduleCursor: 0
  };
}
function nextQueuedSend(queue) {
  for (let offset = 0; offset < FAIR_SEND_SCHEDULE.length; offset += 1) {
    const priority = FAIR_SEND_SCHEDULE[queue.scheduleCursor % FAIR_SEND_SCHEDULE.length];
    queue.scheduleCursor = (queue.scheduleCursor + 1) % FAIR_SEND_SCHEDULE.length;
    if (queue[priority].length) {
      const item = queue[priority].shift();
      queue.queuedBytes = Math.max(0, queue.queuedBytes - Number(item?.byteLength || 0));
      return item;
    }
  }
  return null;
}
function nextHighPriorityInlineSend(queue) {
  if (!queue?.high?.length) return null;
  const index = queue.high.findIndex((item2) => item2?.inline);
  if (index < 0) return null;
  const item = queue.high.splice(index, 1)[0] || null;
  if (item) queue.queuedBytes = Math.max(0, queue.queuedBytes - Number(item.byteLength || 0));
  return item;
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
  const bytes = base64ToBytes2(chunk.compressedBase64);
  const json = await deflateInflate(bytes);
  return JSON.parse(json);
}
function base64ToBytes2(b64) {
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
  throw new Error('DecompressionStream("deflate-raw") is required for compressed CTOX Sync Engine chunks');
}

// src/apps/business-os/rxdb/src/demand-loading-transport.mjs
var ACK_RESPONSE = Object.freeze({ ack: true });
var SERVER_QUERY_STREAM_LIMIT = Math.max(1, Number(CTOX_QUERY_RPC.maxInFlightStreams) || 4);
var CLIENT_QUERY_STREAM_LIMIT = Math.max(1, Math.min(6, SERVER_QUERY_STREAM_LIMIT - 1 || 1));
var CLIENT_QUERY_QUEUE_LIMIT = 128;
var CLIENT_QUERY_QUEUE_BUDGET_BYTES = 1024 * 1024;
var CLIENT_FILE_COLLECTOR_LIMIT = 8;
var QUERY_STREAM_LIMIT_RETRY_MS = 160;
var QUERY_STREAM_LIMIT_RETRIES = 6;
var QUERY_RATE_LIMIT_RETRY_MS = 100;
var QUERY_RATE_LIMIT_RETRIES = 16;
var QUERY_PEER_RETRY_MS = 250;
var QUERY_PEER_RETRIES = 24;
var QUERY_PEER_WAIT_TIMEOUT_MS = 8e3;
var QUERY_PEER_WAIT_POLL_MS = 100;
var QUERY_FETCH_REQUEST_TIMEOUT_MS = 45e3;
var DEFAULT_COLLECTOR_TIMEOUT_MS = Math.max(
  1e3,
  Number(CTOX_QUERY_RPC.maxQueryRuntimeMs) + 5e3 || 35e3
);
var GLOBAL_QUERY_STREAM_STATE_KEY = /* @__PURE__ */ Symbol.for("ctox.rxdb.query-stream-state.v1");
var CANCELLED_QUERY_REQUEST_LIMIT = 256;
var DEFAULT_FILE_COLLECTOR_BUDGET_BYTES = 512 * 1024;
function createDemandLoadingTransport({
  getPeerId,
  collectorTimeoutMs = DEFAULT_COLLECTOR_TIMEOUT_MS,
  fileCollectorBudgetBytes = DEFAULT_FILE_COLLECTOR_BUDGET_BYTES
} = {}) {
  if (typeof getPeerId !== "function") {
    throw new TypeError("createDemandLoadingTransport requires getPeerId");
  }
  const queryCollectors = /* @__PURE__ */ new Map();
  const fileCollectors = /* @__PURE__ */ new Map();
  const queryStreamState = getGlobalQueryStreamState();
  const cancelledQueryRequests = /* @__PURE__ */ new Map();
  const transportOwner = /* @__PURE__ */ Symbol("ctox-demand-transport-owner");
  const terminalTimeoutMs = Math.max(1, Number(collectorTimeoutMs) || DEFAULT_COLLECTOR_TIMEOUT_MS);
  const acceptedFileBudgetBytes = Math.max(
    Number(CTOX_FILE_RPC.maxBytesPerChunk) || 1,
    Number(fileCollectorBudgetBytes) || DEFAULT_FILE_COLLECTOR_BUDGET_BYTES
  );
  const metrics = {
    queryFetchRequests: 0,
    fileFetchRequests: 0,
    queryChunksReceived: 0,
    fileChunksReceived: 0,
    queryCollectorsRejected: 0,
    fileCollectorsRejected: 0,
    queryCancelRequests: 0,
    fileCancelRequests: 0,
    maxPendingQueryCollectors: 0,
    maxPendingFileCollectors: 0,
    maxQueuedQueryRequests: 0,
    maxQueuedQueryBytes: 0,
    maxBufferedQueryChunks: 0,
    maxBufferedFileChunks: 0,
    maxBufferedFileChunkBytes: 0,
    queryCollectorTimeouts: 0,
    fileCollectorTimeouts: 0,
    fileCollectorBudgetExceeded: 0
  };
  function updatePeaks() {
    metrics.maxPendingQueryCollectors = Math.max(metrics.maxPendingQueryCollectors, queryCollectors.size);
    metrics.maxPendingFileCollectors = Math.max(metrics.maxPendingFileCollectors, fileCollectors.size);
    metrics.maxQueuedQueryRequests = Math.max(metrics.maxQueuedQueryRequests, queryStreamState.queue.length);
    metrics.maxQueuedQueryBytes = Math.max(metrics.maxQueuedQueryBytes, queuedQueryBytes());
    metrics.maxBufferedQueryChunks = Math.max(metrics.maxBufferedQueryChunks, bufferedChunkCount(queryCollectors));
    metrics.maxBufferedFileChunks = Math.max(metrics.maxBufferedFileChunks, bufferedChunkCount(fileCollectors));
    metrics.maxBufferedFileChunkBytes = Math.max(metrics.maxBufferedFileChunkBytes, bufferedFileChunkBytes(fileCollectors));
  }
  function routeQueryChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = queryCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    metrics.queryChunksReceived += 1;
    updatePeaks();
    if (chunk.complete) {
      queryCollectors.delete(chunk.requestId);
      clearCollectorTimer(slot);
      slot.resolve(slot.chunks);
    }
  }
  function routeQueryError(err) {
    if (!err || !err.requestId) return;
    const slot = queryCollectors.get(err.requestId);
    if (!slot) return;
    queryCollectors.delete(err.requestId);
    clearCollectorTimer(slot);
    metrics.queryCollectorsRejected += 1;
    const e = new Error(`${err.code || "QUERY_ERROR"}: ${err.message || ""}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }
  async function routeFileChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = fileCollectors.get(chunk.requestId);
    if (!slot) return;
    const chunkBytes = typeof chunk.bytesBase64 === "string" ? chunk.bytesBase64.length : 0;
    try {
      if (typeof slot.onChunk === "function") {
        await slot.onChunk(chunk);
      } else {
        slot.bufferedBytes += chunkBytes;
        if (slot.bufferedBytes > acceptedFileBudgetBytes) {
          const error = new Error(`FILE_COLLECTOR_BUDGET_EXCEEDED: ${slot.bufferedBytes} > ${acceptedFileBudgetBytes}`);
          error.code = "FILE_COLLECTOR_BUDGET_EXCEEDED";
          error.retryable = false;
          throw error;
        }
        slot.chunks.push(chunk);
      }
      metrics.fileChunksReceived += 1;
      updatePeaks();
      if (chunk.complete) {
        fileCollectors.delete(chunk.requestId);
        clearCollectorTimer(slot);
        slot.resolve(slot.chunks);
      }
    } catch (error) {
      fileCollectors.delete(chunk.requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      if (error?.code === "FILE_COLLECTOR_BUDGET_EXCEEDED") {
        metrics.fileCollectorBudgetExceeded += 1;
      }
      slot.reject(error);
      Promise.resolve(
        peer?.request?.(slot.peerId, CTOX_FILE_RPC.cancel, [{
          requestId: chunk.requestId,
          reason: error?.code || "file-chunk-consumer-failed"
        }], 2e3)
      ).catch(() => {
      });
    }
  }
  function routeFileError(err) {
    if (!err || !err.requestId) return;
    const slot = fileCollectors.get(err.requestId);
    if (!slot) return;
    fileCollectors.delete(err.requestId);
    clearCollectorTimer(slot);
    metrics.fileCollectorsRejected += 1;
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
      await routeFileChunk(params?.[0]);
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
    return withQueryStreamSlot(envelope, () => requestQueryFetchWithRetry(envelope));
  }
  function withQueryStreamSlot(envelope, fn) {
    return new Promise((resolve, reject) => {
      const requestId = String(envelope?.requestId || "");
      const estimatedBytes = estimateEnvelopeBytes(envelope);
      const run = () => {
        queryStreamState.active += 1;
        Promise.resolve().then(fn).then(resolve, reject).finally(() => {
          queryStreamState.active = Math.max(0, queryStreamState.active - 1);
          const next = queryStreamState.queue.shift();
          if (next) queueMicrotask(typeof next === "function" ? next : next.run);
        });
      };
      if (queryStreamState.active < CLIENT_QUERY_STREAM_LIMIT) run();
      else {
        const queuedBytes = queryStreamState.queue.reduce(
          (total, entry) => total + Math.max(0, Number(entry?.estimatedBytes) || 0),
          0
        );
        if (queryStreamState.queue.length >= CLIENT_QUERY_QUEUE_LIMIT || queuedBytes + estimatedBytes > CLIENT_QUERY_QUEUE_BUDGET_BYTES) {
          const error = new Error("QUERY_QUEUE_LIMIT: queued demand requests exceed the browser count/byte budget");
          error.code = "QUERY_QUEUE_LIMIT";
          error.retryable = true;
          reject(error);
          return;
        }
        queryStreamState.queue.push({ requestId, run, reject, owner: transportOwner, estimatedBytes });
        updatePeaks();
      }
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
        const rateLimited = isRetryableQueryRateLimited(error);
        const retryLimit = peerUnavailable ? QUERY_PEER_RETRIES : rateLimited ? QUERY_RATE_LIMIT_RETRIES : QUERY_STREAM_LIMIT_RETRIES;
        if (!isRetryableQueryFetch(error) || attempt >= retryLimit) {
          throw error;
        }
        attempt += 1;
        const retryDelayMs = peerUnavailable ? QUERY_PEER_RETRY_MS : rateLimited ? QUERY_RATE_LIMIT_RETRY_MS : QUERY_STREAM_LIMIT_RETRY_MS;
        await delay4(retryDelayMs * attempt);
      }
    }
  }
  async function requestQueryFetchOnce(envelope) {
    const requestId = envelope?.requestId;
    const cancelReason = consumeQueryCancelReason(requestId);
    if (cancelReason) throw createQueryCancelError(cancelReason);
    if (!peer) throw new Error("demand transport has no peer attached");
    const peerId = await waitForPeerId();
    if (!peerId) throw new Error("PEER_UNAVAILABLE");
    const promise = new Promise((resolve, reject) => {
      queryCollectors.set(requestId, { chunks: [], resolve, reject, peerId });
      metrics.queryFetchRequests += 1;
      updatePeaks();
    });
    try {
      await peer.request(peerId, CTOX_QUERY_RPC.fetch, [envelope], QUERY_FETCH_REQUEST_TIMEOUT_MS);
    } catch (err) {
      clearCollectorTimer(queryCollectors.get(requestId));
      queryCollectors.delete(requestId);
      throw err;
    }
    armCollectorTimeout(queryCollectors, requestId, "query", peerId, CTOX_QUERY_RPC.cancel);
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
    return isRetryableQueryStreamLimit(error) || isRetryableQueryRateLimited(error) || isRetryableQueryPeerUnavailable(error);
  }
  function isRetryableQueryRateLimited(error) {
    const code = String(error?.code || "");
    const message = String(error?.message || "");
    return Boolean(error?.retryable) && (code === "RATE_LIMITED" || message.includes("RATE_LIMITED"));
  }
  function isRetryableQueryPeerUnavailable(error) {
    const message = String(error?.message || "");
    return message === "PEER_UNAVAILABLE" || /WebRTC peer .* is not open/.test(message) || message.includes("Timed out waiting for WebRTC response rxdb.query.fetch");
  }
  function delay4(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
  async function requestQueryCancel({ requestId, reason = "client-abort" }) {
    if (!requestId) return;
    metrics.queryCancelRequests += 1;
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
  async function requestFileFetch({ requestId, fileId, range, knownSequences, collectionName, onChunk }) {
    if (!peer) throw new Error("demand transport has no peer attached");
    if (fileCollectors.size >= CLIENT_FILE_COLLECTOR_LIMIT) {
      const error = new Error("FILE_COLLECTOR_LIMIT: too many active browser file collectors");
      error.code = "FILE_COLLECTOR_LIMIT";
      error.retryable = true;
      throw error;
    }
    const peerId = await waitForPeerId();
    if (!peerId) throw new Error("PEER_UNAVAILABLE");
    const promise = new Promise((resolve, reject) => {
      fileCollectors.set(requestId, {
        chunks: [],
        resolve,
        reject,
        peerId,
        onChunk: typeof onChunk === "function" ? onChunk : null,
        bufferedBytes: 0
      });
      metrics.fileFetchRequests += 1;
      updatePeaks();
    });
    try {
      await peer.request(peerId, CTOX_FILE_RPC.fetch, [{
        requestId,
        collectionName,
        fileId,
        range: range ?? null,
        knownSequences: knownSequences ?? []
      }]);
    } catch (err) {
      clearCollectorTimer(fileCollectors.get(requestId));
      fileCollectors.delete(requestId);
      throw err;
    }
    armCollectorTimeout(fileCollectors, requestId, "file", peerId, CTOX_FILE_RPC.cancel);
    const chunks = await promise;
    return chunks.map((c) => ({ sequence: c.sequence, bytesBase64: c.bytesBase64, hash: c.hash }));
  }
  async function requestFileCancel({ requestId, reason = "client-abort" } = {}) {
    if (!requestId) return false;
    metrics.fileCancelRequests += 1;
    const slot = fileCollectors.get(requestId);
    const error = createFileCancelError(reason);
    let cancelled = false;
    if (slot) {
      fileCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      slot.reject(error);
      cancelled = true;
    }
    const peerId = slot?.peerId || (peer ? resolvePeerId() : "");
    if (peer && peerId) {
      try {
        await peer.request(peerId, CTOX_FILE_RPC.cancel, [{ requestId, reason }], 2e3);
      } catch {
      }
    }
    return cancelled;
  }
  function abortPeerRequests(peerId, reason = "peer-close") {
    const queryError = createQueryCancelError(reason);
    const fileError = createFileCancelError(reason);
    let rejected = 0;
    for (const [requestId, slot] of [...queryCollectors.entries()]) {
      if (peerId && slot.peerId !== peerId) continue;
      queryCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.queryCollectorsRejected += 1;
      slot.reject(queryError);
      rejected += 1;
    }
    for (const [requestId, slot] of [...fileCollectors.entries()]) {
      if (peerId && slot.peerId !== peerId) continue;
      fileCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      slot.reject(fileError);
      rejected += 1;
    }
    rejected += rejectQueuedQueryRequestsForOwner(reason);
    return rejected;
  }
  function pendingQueryCount() {
    return queryCollectors.size + queryStreamState.queue.length;
  }
  function pendingFileCount() {
    return fileCollectors.size;
  }
  function diagnostics() {
    updatePeaks();
    return {
      schema: "ctox.rxdb.demand_transport.v1",
      pendingQueryCollectors: queryCollectors.size,
      pendingFileCollectors: fileCollectors.size,
      queuedQueryRequests: queryStreamState.queue.length,
      queuedQueryBytes: queuedQueryBytes(),
      activeQueryStreams: queryStreamState.active,
      bufferedQueryChunks: bufferedChunkCount(queryCollectors),
      bufferedFileChunks: bufferedChunkCount(fileCollectors),
      bufferedFileChunkBytes: bufferedFileChunkBytes(fileCollectors),
      fileCollectorBudgetBytes: acceptedFileBudgetBytes,
      cancelledQueryRequestCacheSize: cancelledQueryRequests.size,
      ...metrics
    };
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
    clearCollectorTimer(slot);
    metrics.queryCollectorsRejected += 1;
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
        metrics.queryCollectorsRejected += 1;
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
  function rejectQueuedQueryRequestsForOwner(reason) {
    const remaining = [];
    let rejected = 0;
    const error = createQueryCancelError(reason);
    for (const entry of queryStreamState.queue) {
      if (entry?.owner === transportOwner) {
        rejected += 1;
        metrics.queryCollectorsRejected += 1;
        entry.reject(error);
      } else {
        remaining.push(entry);
      }
    }
    if (rejected) {
      queryStreamState.queue.splice(0, queryStreamState.queue.length, ...remaining);
    }
    return rejected;
  }
  function queuedQueryRequestId(entry) {
    if (!entry || typeof entry === "function") return "";
    return String(entry.requestId || "");
  }
  function queuedQueryBytes() {
    return queryStreamState.queue.reduce(
      (total, entry) => total + Math.max(0, Number(entry?.estimatedBytes) || 0),
      0
    );
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
  function createFileCancelError(reason) {
    const error = new Error(`FILE_CANCELLED: ${reason || "client-abort"}`);
    error.code = "FILE_CANCELLED";
    error.retryable = false;
    return error;
  }
  function armCollectorTimeout(collectors, requestId, kind, peerId, cancelMethod) {
    const slot = collectors.get(requestId);
    if (!slot || slot.timer) return;
    slot.timer = setTimeout(() => {
      if (collectors.get(requestId) !== slot) return;
      collectors.delete(requestId);
      if (kind === "query") {
        metrics.queryCollectorsRejected += 1;
        metrics.queryCollectorTimeouts += 1;
      } else {
        metrics.fileCollectorsRejected += 1;
        metrics.fileCollectorTimeouts += 1;
      }
      const error = new Error(`${kind.toUpperCase()}_COLLECTOR_TIMEOUT: terminal frame missing`);
      error.code = `${kind.toUpperCase()}_COLLECTOR_TIMEOUT`;
      error.retryable = true;
      slot.reject(error);
      Promise.resolve(
        peer?.request?.(peerId, cancelMethod, [{ requestId, reason: "collector-timeout" }], 2e3)
      ).catch(() => {
      });
    }, terminalTimeoutMs);
  }
  function clearCollectorTimer(slot) {
    if (slot?.timer) {
      clearTimeout(slot.timer);
      slot.timer = null;
    }
  }
  function estimateEnvelopeBytes(envelope) {
    let encoded = "";
    try {
      encoded = JSON.stringify(envelope ?? null);
    } catch {
      return CLIENT_QUERY_QUEUE_BUDGET_BYTES + 1;
    }
    if (typeof TextEncoder === "function") return new TextEncoder().encode(encoded).byteLength;
    return encoded.length * 2;
  }
  function resolvePeerId() {
    const configured = getPeerId();
    if (configured && isPeerOpen(configured)) return configured;
    return firstOpenPeerId();
  }
  function firstOpenPeerId() {
    const entries = peer?.connections?.entries?.();
    if (!entries) return "";
    for (const [peerId, connection] of entries) {
      if (isPeerConnectionOpen(connection)) return peerId;
    }
    return "";
  }
  function isPeerOpen(peerId) {
    return Boolean(peerId && isPeerConnectionOpen(peer?.connections?.get?.(peerId)));
  }
  function isPeerConnectionOpen(connection) {
    if (!connection) return false;
    const channelState = connection?.channel?.readyState || connection?.channelReadyState || "";
    const peerState = connection?.peer?.connectionState || connection?.peerConnectionState || connection?.connectionState || "";
    return channelState === "open" && !["closed", "failed", "disconnected"].includes(String(peerState));
  }
  async function waitForPeerId(timeoutMs = QUERY_PEER_WAIT_TIMEOUT_MS) {
    const immediate = resolvePeerId();
    if (immediate) return immediate;
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      await delay4(QUERY_PEER_WAIT_POLL_MS);
      const peerId = resolvePeerId();
      if (peerId) return peerId;
    }
    return "";
  }
  return {
    requestHandlers,
    attach,
    requestQueryFetch,
    requestQueryCancel,
    requestFileFetch,
    requestFileCancel,
    abortPeerRequests,
    pendingQueryCount,
    pendingFileCount,
    diagnostics
  };
}
function bufferedChunkCount(collectors) {
  let total = 0;
  for (const slot of collectors.values()) {
    total += Array.isArray(slot?.chunks) ? slot.chunks.length : 0;
  }
  return total;
}
function bufferedFileChunkBytes(collectors) {
  let total = 0;
  for (const slot of collectors.values()) {
    if (!Array.isArray(slot?.chunks)) continue;
    for (const chunk of slot.chunks) {
      if (typeof chunk?.bytesBase64 === "string") {
        total += chunk.bytesBase64.length;
      }
    }
  }
  return total;
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
var CONTROL_PLANE_QUERY_REVALIDATE_MS = 1e3;
function createQueryDemandLoader({
  storageCollection,
  sidecar,
  collectionName,
  schemaVersion,
  requestQueryFetch,
  requestCancel = null,
  multiTabBroker = null,
  status = null,
  clock = Date.now,
  // Origin stamp (object or provider fn) for every document this loader
  // writes into the primary store. Demand-fetched documents ARE master
  // state: without the stamp they counted as unsynced LOCAL writes, so the
  // push pipeline echoed them (and cache-eviction tombstones — i.e. DELETES)
  // back to the master, and the LWW gate let them veto later master pulls.
  replicationOrigin = null
}) {
  if (!storageCollection) throw new TypeError("demand loader requires storageCollection");
  if (!sidecar) throw new TypeError("demand loader requires sidecar");
  if (!collectionName) throw new TypeError("demand loader requires collectionName");
  if (typeof requestQueryFetch !== "function") {
    throw new TypeError("demand loader requires requestQueryFetch");
  }
  const resolveReplicationOrigin = () => (typeof replicationOrigin === "function" ? replicationOrigin() : replicationOrigin) || null;
  const inflightByFingerprint = /* @__PURE__ */ new Map();
  const coordinatedByFingerprint = /* @__PURE__ */ new Map();
  const resolvingByInput = /* @__PURE__ */ new Map();
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
      const inputKey = JSON.stringify(fingerprintInput);
      const existingInvocation = resolvingByInput.get(inputKey);
      if (existingInvocation) {
        bumpStatus(status, "queryFetchDedupHitCount");
        return existingInvocation;
      }
      const invocationJob = (async () => {
        const fingerprint = await queryFingerprint(fingerprintInput);
        const sidecarKey = [collectionName, fingerprint, normalizedWindow.offset, normalizedWindow.limit];
        const cached = await sidecar.getQueryWindow(sidecarKey);
        const controlPlaneWindowStale = isControlPlaneStatusCollection(collectionName) && cached && clock() - Number(cached.updatedAt || cached.createdAt || 0) >= CONTROL_PLANE_QUERY_REVALIDATE_MS;
        if (cached && cached.complete) {
          if (query?.requireRevision && cached.authoritativeRevision !== query.requireRevision) {
          } else if (!controlPlaneWindowStale) {
            await touchSidecarAccess(sidecar, collectionName, cached.documentIds);
            return readLocalDocuments(storageCollection, query, normalizedWindow);
          }
        }
        const dedupKey = `${collectionName}|${fingerprint}|${normalizedWindow.offset}|${normalizedWindow.limit}`;
        const startFetchJob = () => {
          if (inflightByFingerprint.has(dedupKey)) {
            bumpStatus(status, "queryFetchDedupHitCount");
            return inflightByFingerprint.get(dedupKey).job;
          }
          bumpStatus(status, "queryFetchInFlight", 1);
          v15Log("fetch:start", { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
          const requestId = `${dedupKey}|${clock()}`;
          const job = (async () => {
            const startedAt = clock();
            try {
              const result = await requestQueryFetch({
                requestId,
                databaseName: storageCollection?.databaseName ?? null,
                collectionName,
                schemaVersion: schemaVersion ?? 0,
                queryFingerprint: fingerprint,
                query: {
                  selector: query?.selector ?? {},
                  sort: normalizeSort(query?.sort),
                  limit: query?.limit,
                  skip: query?.skip
                },
                window: normalizedWindow
              });
              await materializeChunks(storageCollection, result.documents || [], resolveReplicationOrigin());
              const documentIds = (result.documents || []).map(extractId).filter(Boolean);
              await sidecar.upsertQueryWindow({
                collection: collectionName,
                queryFingerprint: fingerprint,
                offset: normalizedWindow.offset,
                limit: normalizedWindow.limit,
                documentIds,
                complete: true,
                authoritativeRevision: result.authoritativeRevision ?? null,
                queryShape: {
                  selector: query?.selector ?? {},
                  sort: normalizeSort(query?.sort)
                }
              });
              await sidecar.touchDocuments(collectionName, documentIds, {
                estimatedBytes: estimateBytesPerDocument(result.documents || [])
              });
              bumpStatus(status, "queryFetchSuccessCount");
              if (status) status.lastQueryFetchMs = clock() - startedAt;
              v15Log("fetch:ok", { fingerprint, docs: documentIds.length, ms: clock() - startedAt });
              return readLocalDocuments(storageCollection, query, normalizedWindow);
            } catch (error) {
              if (isQueryCancelledError(error)) {
                bumpStatus(status, "queryFetchCancelCount");
                v15Log("fetch:cancel", { fingerprint, error: String(error?.message ?? error) });
                return readLocalDocuments(storageCollection, query, normalizedWindow);
              }
              bumpStatus(status, "queryFetchErrorCount");
              v15Log("fetch:error", { fingerprint, error: String(error?.message ?? error) });
              throw error;
            } finally {
              bumpStatus(status, "queryFetchInFlight", -1);
              inflightByFingerprint.delete(dedupKey);
            }
          })();
          inflightByFingerprint.set(dedupKey, { job, requestId });
          return job;
        };
        const runCoordinatedFetchJob = async () => {
          if (!multiTabBroker?.claim) return startFetchJob();
          if (multiTabBroker.closed) return readLocalDocuments(storageCollection, query, normalizedWindow);
          const leader = await multiTabBroker.claim(dedupKey);
          if (leader) {
            try {
              return await startFetchJob();
            } finally {
              await multiTabBroker.release?.(dedupKey, { materialized: true });
            }
          }
          await multiTabBroker.waitForRemote?.(dedupKey, 5e3);
          if (multiTabBroker.closed) return readLocalDocuments(storageCollection, query, normalizedWindow);
          const materialized = await sidecar.getQueryWindow(sidecarKey);
          if (materialized?.complete) {
            bumpStatus(status, "queryFetchDedupHitCount");
            return readLocalDocuments(storageCollection, query, normalizedWindow);
          }
          const takeover = await multiTabBroker.claim(dedupKey);
          if (!takeover) {
            if (multiTabBroker.closed) return readLocalDocuments(storageCollection, query, normalizedWindow);
            throw new Error(`Timed out waiting for multi-tab query owner ${dedupKey}`);
          }
          try {
            return await startFetchJob();
          } finally {
            await multiTabBroker.release?.(dedupKey, { materialized: true, takeover: true });
          }
        };
        const coordinatedFetchJob = () => {
          const current = coordinatedByFingerprint.get(dedupKey);
          if (current) {
            bumpStatus(status, "queryFetchDedupHitCount");
            return current;
          }
          const job = Promise.resolve().then(runCoordinatedFetchJob).finally(() => {
            if (coordinatedByFingerprint.get(dedupKey) === job) {
              coordinatedByFingerprint.delete(dedupKey);
            }
          });
          coordinatedByFingerprint.set(dedupKey, job);
          return job;
        };
        if (cached?.everCompleted && !query?.requireRevision) {
          if (controlPlaneWindowStale) {
            return coordinatedFetchJob();
          }
          coordinatedFetchJob().catch(() => {
          });
          bumpStatus(status, "queryFetchStaleServedCount");
          v15Log("fetch:stale-served", { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
          await touchSidecarAccess(sidecar, collectionName, cached.documentIds || []);
          return readLocalDocuments(storageCollection, query, normalizedWindow);
        }
        return coordinatedFetchJob();
      })();
      resolvingByInput.set(inputKey, invocationJob);
      try {
        return await invocationJob;
      } finally {
        if (resolvingByInput.get(inputKey) === invocationJob) resolvingByInput.delete(inputKey);
      }
    },
    inflightSize() {
      return Math.max(inflightByFingerprint.size, coordinatedByFingerprint.size, resolvingByInput.size);
    },
    // Wave 7: invalidation hook. When the replication layer reports that a
    // document in `collectionName` was changed remotely, call this with the
    // changed document ids — any cached query window that references those
    // ids is marked incomplete so the next exec triggers a remote refresh.
    async invalidateDocumentChange(changedDocumentIds = []) {
      if (!changedDocumentIds.length) return 0;
      if (typeof sidecar.invalidateQueryWindowsForDocuments === "function") {
        return sidecar.invalidateQueryWindowsForDocuments(collectionName, changedDocumentIds);
      }
      return invalidateByScanningQueryWindows(sidecar, collectionName, changedDocumentIds);
    },
    async invalidateDocuments(changedDocuments = []) {
      if (!changedDocuments.length) return 0;
      if (typeof sidecar.invalidateQueryWindowsForChanges === "function") {
        return sidecar.invalidateQueryWindowsForChanges(
          collectionName,
          changedDocuments,
          storageCollection?.primaryPath || "id"
        );
      }
      return this.invalidateDocumentChange(changedDocuments.map(extractId).filter(Boolean));
    },
    // Wave 7 + production hardening: reconnect-cancel. Aborts all in-flight
    // fetches and removes any partially-materialized documents from the
    // primary store so the next fetch starts from a clean slate (no orphans).
    async abortAllInFlight(reason = "reconnect") {
      const cancelled = [];
      for (const [dedupKey, entry] of inflightByFingerprint.entries()) {
        const { job, requestId } = entry;
        const [, fingerprint] = dedupKey.split("|");
        cancelled.push({ dedupKey, fingerprint });
        try {
          job.catch?.(() => {
          });
        } catch {
        }
        if (typeof requestCancel === "function") {
          try {
            await requestCancel({ requestId, fingerprint, reason });
          } catch {
          }
        }
      }
      inflightByFingerprint.clear();
      coordinatedByFingerprint.clear();
      resolvingByInput.clear();
      try {
        const allWindows = await sidecar.backend.scanQueryWindows();
        for (const { fingerprint } of cancelled) {
          const partial = allWindows.filter(
            // Ever-complete windows hold replicated (validated) documents;
            // an aborted background revalidation must not tombstone them.
            // Only never-completed windows can reference partial orphans.
            (w) => w.queryFingerprint === fingerprint && !w.complete && !w.everCompleted
          );
          for (const window2 of partial) {
            const ids = window2.documentIds || [];
            if (ids.length && typeof storageCollection.bulkWrite === "function") {
              const tombstones = ids.map((id) => ({ id, _deleted: true }));
              try {
                await storageCollection.bulkWrite(tombstones, { replicationOrigin: resolveReplicationOrigin() });
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
function isControlPlaneStatusCollection(collectionName) {
  return collectionName === "business_commands" || collectionName === "ctox_queue_tasks";
}
async function invalidateByScanningQueryWindows(sidecar, collectionName, changedDocumentIds) {
  const all = await sidecar.backend.scanQueryWindows();
  const ids = new Set(changedDocumentIds.map((id) => String(id || "")).filter(Boolean));
  let invalidated = 0;
  for (const window2 of all) {
    if (window2.collection !== collectionName) continue;
    const documentIds = Array.isArray(window2.documentIds) ? window2.documentIds : [];
    if (documentIds.some((id) => ids.has(String(id || "")))) {
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
async function materializeChunks(storageCollection, documents, replicationOrigin = null) {
  if (!documents.length) return;
  await storageCollection.bulkWrite(documents, { replicationOrigin });
}
async function touchSidecarAccess(sidecar, collectionName, documentIds) {
  if (!documentIds?.length) return;
  await sidecar.touchDocuments(collectionName, documentIds);
}
function extractId(doc) {
  if (!doc || typeof doc !== "object") return null;
  return doc.id || doc._id || null;
}
function estimateBytes2(documents) {
  try {
    return JSON.stringify(documents).length;
  } catch {
    return documents.length * 256;
  }
}
function estimateBytesPerDocument(documents) {
  if (!Array.isArray(documents) || documents.length === 0) return 0;
  return Math.max(1, Math.ceil(estimateBytes2(documents) / documents.length));
}
function bumpStatus(status, field, delta = 1) {
  if (!status) return;
  if (typeof status[field] !== "number") status[field] = 0;
  status[field] += delta;
}
function isQueryCancelledError(error) {
  return error?.code === "QUERY_CANCELLED" || String(error?.message || "").includes("QUERY_CANCELLED");
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
var DEFAULT_FILE_RETURN_BUDGET_BYTES = 32 * 1024 * 1024;
function createFileDemandLoader({
  collectionName,
  storageCollection,
  sidecarBackend,
  requestFileFetch,
  requestFileCancel = null,
  status = null,
  clock = Date.now,
  persistChunks = true,
  // Origin stamp (object or provider fn): fetched chunk rows are master
  // state, not local writes — see query-demand-loader.mjs for the failure
  // modes an unstamped write causes (push echo, LWW veto of later pulls).
  replicationOrigin = null,
  returnBudgetBytes = DEFAULT_FILE_RETURN_BUDGET_BYTES
}) {
  if (!collectionName) throw new TypeError("file loader requires collectionName");
  if (!storageCollection) throw new TypeError("file loader requires storageCollection");
  if (!sidecarBackend) throw new TypeError("file loader requires sidecarBackend");
  if (typeof requestFileFetch !== "function") {
    throw new TypeError("file loader requires requestFileFetch");
  }
  const inflight = /* @__PURE__ */ new Map();
  let requestSequence = 0;
  const resolveReplicationOrigin = () => (typeof replicationOrigin === "function" ? replicationOrigin() : replicationOrigin) || null;
  return {
    async fetchFile(fileId, { range = null } = {}) {
      const inflightKey = fileInflightKey(fileId, range);
      if (inflight.has(inflightKey)) {
        bump(status, "fileStreamDedupHits");
        return inflight.get(inflightKey).promise;
      }
      const startedAt = clock();
      const requestId = `file-${fileId}-${startedAt}-${++requestSequence}`;
      const job = (async () => {
        bump(status, "activeFileStreams", 1);
        try {
          const presence = persistChunks ? await getPresence(sidecarBackend, collectionName, fileId) : null;
          const validChunks = [];
          const consumedSequences = /* @__PURE__ */ new Set();
          let returnedBytes = 0;
          const consumeChunk = async (chunk) => {
            if (!chunk || typeof chunk !== "object" || chunk.complete && chunk.sequence == null) return;
            const sequence = Number(chunk.sequence);
            if (!Number.isFinite(sequence) || consumedSequences.has(sequence)) return;
            const bytesBase64 = String(chunk.bytesBase64 || "");
            const decodedBytes = Math.floor(bytesBase64.length * 3 / 4);
            returnedBytes += decodedBytes;
            if (returnedBytes > returnBudgetBytes) {
              const error = new Error(`FILE_RETURN_BUDGET_EXCEEDED: ${returnedBytes} > ${returnBudgetBytes}; request a byte range`);
              error.code = "FILE_RETURN_BUDGET_EXCEEDED";
              error.retryable = false;
              throw error;
            }
            consumedSequences.add(sequence);
            const normalized = {
              sequence,
              bytesBase64,
              hash: chunk.hash || null
            };
            validChunks.push(normalized);
            bump(status, "fileBytesReceived", bytesBase64.length);
            if (persistChunks) {
              await storageCollection.bulkWrite([{
                id: `${fileId}-${sequence}`,
                file_id: fileId,
                sequence,
                bytes_base64: bytesBase64,
                hash: normalized.hash
              }], { replicationOrigin: resolveReplicationOrigin() });
            }
          };
          const chunks = await requestFileFetch({
            requestId,
            collectionName,
            fileId,
            range,
            knownSequences: presence?.presentSequences || [],
            onChunk: consumeChunk
          });
          if (!Array.isArray(chunks)) {
            throw new TypeError("requestFileFetch must return an array of chunks");
          }
          for (const chunk of chunks) {
            await consumeChunk(chunk);
          }
          const sequences = validChunks.map((c) => c.sequence).sort((a, b) => a - b);
          if (persistChunks) {
            const highestSequence = sequences.length ? Math.max(...sequences) : -1;
            const expectedTotal = Math.max(
              highestSequence,
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
          }
          if (status) status.lastFileFetchMs = clock() - startedAt;
          return validChunks.sort((left, right) => left.sequence - right.sequence);
        } catch (error) {
          bump(status, "fileStreamErrors");
          throw error;
        } finally {
          bump(status, "activeFileStreams", -1);
          if (inflight.get(inflightKey)?.requestId === requestId) {
            inflight.delete(inflightKey);
          }
        }
      })();
      inflight.set(inflightKey, { promise: job, requestId, fileId, range });
      return job;
    },
    inflightSize() {
      return inflight.size;
    },
    async abortAllInFlight(reason = "reconnect") {
      const slots = [...inflight.values()];
      inflight.clear();
      for (const slot of slots) {
        try {
          slot.promise?.catch?.(() => {
          });
        } catch {
        }
        if (typeof requestFileCancel === "function") {
          try {
            await requestFileCancel({
              requestId: slot.requestId,
              fileId: slot.fileId,
              range: slot.range,
              reason
            });
          } catch {
          }
        }
      }
      return slots.length;
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
function fileInflightKey(fileId, range) {
  return `${String(fileId || "")}|${canonicalRangeKey(range)}`;
}
function canonicalRangeKey(range) {
  if (range == null) return "full";
  if (Array.isArray(range)) return `[${range.map(canonicalRangeKey).join(",")}]`;
  if (typeof range === "object") {
    return `{${Object.keys(range).sort().map((key) => `${key}:${canonicalRangeKey(range[key])}`).join(",")}}`;
  }
  return JSON.stringify(range);
}
function dedupeSorted(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  const out = [];
  for (const v of sorted) {
    if (out.length === 0 || out[out.length - 1] !== v) out.push(v);
  }
  return out;
}

// src/apps/business-os/rxdb/src/query-meta-backend-indexeddb.mjs
var SIDECAR_DB_VERSION = 2;
var STORE_QUERY_WINDOWS = "queryWindows";
var STORE_QUERY_WINDOW_REFS = "queryWindowRefs";
var STORE_DOCUMENT_ACCESS = "documentAccess";
var STORE_CACHE_STATS = "cacheStats";
var OPEN_TIMEOUT_MS = 4e3;
function createIndexedDbMetaBackend({ databaseName }) {
  if (!databaseName) throw new TypeError("createIndexedDbMetaBackend requires databaseName");
  let dbPromise = null;
  let fallbackBackend = null;
  const fallback = () => {
    if (!fallbackBackend) fallbackBackend = createMemoryMetaBackend();
    return fallbackBackend;
  };
  const open = async () => {
    if (!dbPromise) {
      dbPromise = Promise.resolve().then(() => openSidecarDatabase(databaseName)).catch((error) => {
        dbPromise = null;
        throw markSidecarOpenError(error);
      });
    }
    return dbPromise;
  };
  const withDb = async (method, args, operation) => {
    if (fallbackBackend) return fallbackBackend[method](...args);
    try {
      return await operation(await open());
    } catch (error) {
      if (!isSidecarOpenError(error)) throw error;
      return fallback()[method](...args);
    }
  };
  return {
    get name() {
      return fallbackBackend ? "memory-fallback" : "indexeddb";
    },
    async putQueryWindow(record) {
      await withDb(
        "putQueryWindow",
        [record],
        (db) => runRequest(
          db.transaction(STORE_QUERY_WINDOWS, "readwrite").objectStore(STORE_QUERY_WINDOWS).put(record)
        )
      );
    },
    async getQueryWindow(key) {
      return withDb(
        "getQueryWindow",
        [key],
        (db) => runRequest(
          db.transaction(STORE_QUERY_WINDOWS, "readonly").objectStore(STORE_QUERY_WINDOWS).get(parseQueryWindowKey(key))
        )
      );
    },
    async deleteQueryWindow(key) {
      await withDb(
        "deleteQueryWindow",
        [key],
        async (db) => {
          await deleteQueryWindowRefs(db, stringKey3(parseQueryWindowKey(key)));
          await runRequest(
            db.transaction(STORE_QUERY_WINDOWS, "readwrite").objectStore(STORE_QUERY_WINDOWS).delete(parseQueryWindowKey(key))
          );
        }
      );
    },
    async scanQueryWindows() {
      return withDb(
        "scanQueryWindows",
        [],
        (db) => runRequest(
          db.transaction(STORE_QUERY_WINDOWS, "readonly").objectStore(STORE_QUERY_WINDOWS).getAll()
        )
      );
    },
    async replaceQueryWindowDocumentRefs(record) {
      await withDb(
        "replaceQueryWindowDocumentRefs",
        [record],
        async (db) => {
          const windowKey = queryWindowKey2(record);
          await deleteQueryWindowRefs(db, windowKey);
          await putQueryWindowRefs(db, record);
        }
      );
    },
    async getQueryWindowKeysByDocumentIds(collection, ids) {
      const normalizedIds = normalizeDocumentIds3(ids);
      if (!normalizedIds.length) return [];
      return withDb(
        "getQueryWindowKeysByDocumentIds",
        [collection, ids],
        async (db) => {
          const tx = db.transaction(STORE_QUERY_WINDOW_REFS, "readonly");
          const index = tx.objectStore(STORE_QUERY_WINDOW_REFS).index("collection_documentId");
          const requests = normalizedIds.map((id) => runRequest(index.getAll([collection, id])));
          const rowsByDocument = await Promise.all(requests);
          const keys = /* @__PURE__ */ new Set();
          for (const rows of rowsByDocument) {
            for (const row of rows || []) {
              if (row?.windowKey) keys.add(row.windowKey);
            }
          }
          return Array.from(keys);
        }
      );
    },
    async putDocumentAccess(record) {
      await withDb(
        "putDocumentAccess",
        [record],
        (db) => runRequest(
          db.transaction(STORE_DOCUMENT_ACCESS, "readwrite").objectStore(STORE_DOCUMENT_ACCESS).put(record)
        )
      );
    },
    async getDocumentAccess(collection, id) {
      return withDb(
        "getDocumentAccess",
        [collection, id],
        (db) => runRequest(
          db.transaction(STORE_DOCUMENT_ACCESS, "readonly").objectStore(STORE_DOCUMENT_ACCESS).get([collection, id])
        )
      );
    },
    async deleteDocumentAccess(collection, id) {
      await withDb(
        "deleteDocumentAccess",
        [collection, id],
        (db) => runRequest(
          db.transaction(STORE_DOCUMENT_ACCESS, "readwrite").objectStore(STORE_DOCUMENT_ACCESS).delete([collection, id])
        )
      );
    },
    async scanDocumentAccess() {
      return withDb(
        "scanDocumentAccess",
        [],
        (db) => runRequest(
          db.transaction(STORE_DOCUMENT_ACCESS, "readonly").objectStore(STORE_DOCUMENT_ACCESS).getAll()
        )
      );
    },
    async putCacheStats(record) {
      await withDb(
        "putCacheStats",
        [record],
        (db) => runRequest(
          db.transaction(STORE_CACHE_STATS, "readwrite").objectStore(STORE_CACHE_STATS).put(record)
        )
      );
    },
    async getCacheStats(databaseName2) {
      return withDb(
        "getCacheStats",
        [databaseName2],
        (db) => runRequest(
          db.transaction(STORE_CACHE_STATS, "readonly").objectStore(STORE_CACHE_STATS).get(databaseName2)
        )
      );
    },
    async clear() {
      await withDb(
        "clear",
        [],
        async (db) => {
          for (const name of [STORE_QUERY_WINDOWS, STORE_QUERY_WINDOW_REFS, STORE_DOCUMENT_ACCESS, STORE_CACHE_STATS]) {
            await runRequest(db.transaction(name, "readwrite").objectStore(name).clear());
          }
        }
      );
    },
    async close() {
      const currentDbPromise = dbPromise;
      dbPromise = null;
      if (currentDbPromise) {
        try {
          const db = await currentDbPromise;
          db.close();
        } catch {
        }
      }
      await fallbackBackend?.close?.();
      fallbackBackend = null;
    }
  };
}
function openSidecarDatabase(databaseName) {
  if (!globalThis.indexedDB) {
    throw new Error("indexedDB is required for sidecar metadata storage");
  }
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
      finish(reject, new Error(`IndexedDB open timed out for sidecar ${databaseName}`));
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
      if (!db.objectStoreNames.contains(STORE_QUERY_WINDOW_REFS)) {
        const store = db.createObjectStore(STORE_QUERY_WINDOW_REFS, {
          keyPath: ["collection", "documentId", "windowKey"]
        });
        store.createIndex("collection_documentId", ["collection", "documentId"], {
          unique: false
        });
        store.createIndex("windowKey", "windowKey", { unique: false });
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
      const db = request.result;
      if (!finish(resolve, db)) {
        try {
          db.close();
        } catch {
        }
      }
    };
    request.onerror = () => {
      finish(reject, request.error || new Error(`failed to open sidecar ${databaseName}`));
    };
    request.onblocked = () => {
      finish(reject, new Error(`IndexedDB open blocked for sidecar ${databaseName}`));
    };
  });
}
function markSidecarOpenError(error) {
  if (error && typeof error === "object") {
    try {
      error.ctoxSidecarOpenError = true;
    } catch {
    }
    return error;
  }
  const wrapped = new Error(String(error || "sidecar IndexedDB open failed"));
  wrapped.ctoxSidecarOpenError = true;
  return wrapped;
}
function isSidecarOpenError(error) {
  return Boolean(error?.ctoxSidecarOpenError);
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
function queryWindowKey2(record) {
  return [record.collection, record.queryFingerprint, record.offset, record.limit].join("|");
}
function stringKey3(key) {
  if (Array.isArray(key)) return key.join("|");
  if (typeof key === "string") return key;
  throw new TypeError("query window key must be array or string");
}
function normalizeDocumentIds3(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || "")).filter(Boolean)));
}
async function putQueryWindowRefs(db, record) {
  const documentIds = normalizeDocumentIds3([...record.documentIds || [], ...record.selectorRefIds || []]);
  if (!documentIds.length) return;
  const windowKey = queryWindowKey2(record);
  await runTransaction(
    db.transaction(STORE_QUERY_WINDOW_REFS, "readwrite"),
    (tx) => {
      const store = tx.objectStore(STORE_QUERY_WINDOW_REFS);
      for (const documentId3 of documentIds) {
        store.put({
          collection: record.collection,
          documentId: documentId3,
          windowKey
        });
      }
    }
  );
}
async function deleteQueryWindowRefs(db, windowKey) {
  await runTransaction(
    db.transaction(STORE_QUERY_WINDOW_REFS, "readwrite"),
    (tx) => {
      const index = tx.objectStore(STORE_QUERY_WINDOW_REFS).index("windowKey");
      const range = globalThis.IDBKeyRange.only(windowKey);
      const request = index.openCursor(range);
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return;
        cursor.delete();
        cursor.continue();
      };
    }
  );
}
function runTransaction(tx, schedule) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error || new Error("IndexedDB transaction aborted"));
    try {
      schedule(tx);
    } catch (error) {
      try {
        tx.abort();
      } catch {
      }
      reject(error);
    }
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
    this.expiryTimer = null;
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
    this.scheduleExpiryNotify();
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
    this.notifyTimer.unref?.();
  }
  scheduleExpiryNotify() {
    if (this.expiryTimer != null) {
      clearTimeout(this.expiryTimer);
      this.expiryTimer = null;
    }
    if (!this.lastExecAt.size) return;
    const now = this.clock();
    let nextExpiryAt = Infinity;
    for (const at of this.lastExecAt.values()) {
      nextExpiryAt = Math.min(nextExpiryAt, at + this.recentExecMs);
    }
    if (!Number.isFinite(nextExpiryAt)) return;
    const delayMs = Math.max(0, nextExpiryAt - now + 1);
    this.expiryTimer = setTimeout(() => {
      this.expiryTimer = null;
      this.scheduleNotify();
      this.scheduleExpiryNotify();
    }, delayMs);
    this.expiryTimer.unref?.();
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

// src/apps/business-os/rxdb/src/presence.mjs
var PRESENCE_NOTIFY_DEBOUNCE_MS = 100;
var PresenceRegistry = class {
  constructor({
    clock = () => Date.now(),
    refreshMs = Number(CTOX_PRESENCE_RPC.refreshMs) || 2e4
  } = {}) {
    this.clock = clock;
    this.refreshMs = refreshMs;
    this.localByOwner = /* @__PURE__ */ new Map();
    this.remoteEntries = [];
    this.localListeners = /* @__PURE__ */ new Set();
    this.remoteListeners = /* @__PURE__ */ new Set();
    this.notifyTimer = null;
    this.refreshTimer = null;
    this.lastNotifiedKey = null;
  }
  // Replace ONE owner's local entries (empty array or null clears them).
  // Owners are module/app surfaces; the wire set is the union of all owners.
  setLocal(ownerKey, entries) {
    const key = String(ownerKey || "default");
    const list = (Array.isArray(entries) ? entries : []).filter((entry) => entry && typeof entry === "object" && !Array.isArray(entry));
    if (list.length === 0) this.localByOwner.delete(key);
    else this.localByOwner.set(key, list);
    this.scheduleNotify();
    this.armRefreshTimer();
  }
  clearLocal(ownerKey) {
    this.setLocal(ownerKey, []);
  }
  // The union of every owner's local entries, deterministic order.
  localEntries() {
    const out = [];
    for (const list of this.localByOwner.values()) out.push(...list);
    out.sort((a, b) => JSON.stringify(a) < JSON.stringify(b) ? -1 : 1);
    return out;
  }
  // Transport hook: fires with the local entry union whenever it changes, and
  // once per refresh window while non-empty (`{ refresh: true }`) so the
  // native TTL clock keeps getting re-stamped. Fires immediately on
  // subscribe. Returns an unsubscribe function.
  onLocalChange(listener) {
    if (typeof listener !== "function") return () => {
    };
    this.localListeners.add(listener);
    try {
      listener(this.localEntries(), { refresh: false });
    } catch {
    }
    return () => {
      this.localListeners.delete(listener);
    };
  }
  // App hook: fires with the remote aggregate (other peers' entries) whenever
  // the native hub pushes a new one. Fires immediately on subscribe.
  onRemoteChange(listener) {
    if (typeof listener !== "function") return () => {
    };
    this.remoteListeners.add(listener);
    try {
      listener(this.remoteEntries.slice());
    } catch {
    }
    return () => {
      this.remoteListeners.delete(listener);
    };
  }
  // Transport hook: a `presence$` push replaces the remote aggregate
  // wholesale (the hub always sends the full set, never deltas).
  applyRemote(entries) {
    this.remoteEntries = (Array.isArray(entries) ? entries : []).filter((entry) => entry && typeof entry === "object" && !Array.isArray(entry));
    for (const listener of this.remoteListeners) {
      try {
        listener(this.remoteEntries.slice());
      } catch {
      }
    }
  }
  scheduleNotify() {
    if (this.notifyTimer != null) return;
    this.notifyTimer = setTimeout(() => {
      this.notifyTimer = null;
      const entries = this.localEntries();
      const key = JSON.stringify(entries);
      if (key === this.lastNotifiedKey) return;
      this.lastNotifiedKey = key;
      for (const listener of this.localListeners) {
        try {
          listener(entries, { refresh: false });
        } catch {
        }
      }
    }, PRESENCE_NOTIFY_DEBOUNCE_MS);
    this.notifyTimer.unref?.();
  }
  // Idle discipline: the refresh timer exists ONLY while local entries exist.
  // An idle tab with no presence publishes nothing and keeps no timer.
  armRefreshTimer() {
    const hasLocal = this.localByOwner.size > 0;
    if (!hasLocal) {
      if (this.refreshTimer != null) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
      }
      return;
    }
    if (this.refreshTimer != null) return;
    this.refreshTimer = setInterval(() => {
      if (this.localByOwner.size === 0) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
        return;
      }
      for (const listener of this.localListeners) {
        try {
          listener(this.localEntries(), { refresh: true });
        } catch {
        }
      }
    }, this.refreshMs);
    this.refreshTimer.unref?.();
  }
};
var SINGLETON2 = null;
function getPresenceRegistry() {
  if (!SINGLETON2) SINGLETON2 = new PresenceRegistry();
  return SINGLETON2;
}
function createPresenceRegistry(options = {}) {
  return new PresenceRegistry(options);
}

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
  "pendingQueryFetchCollectors",
  "queuedQueryFetchRequests",
  "maxPendingQueryFetchCollectors",
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
  "pendingFileFetchCollectors",
  "maxPendingFileFetchCollectors",
  "fileBytesReceived",
  "fileStreamErrors",
  "fileStreamDedupHits",
  "lastFileFetchMs",
  "localPushChangedSinceCalls",
  "localPushChangedSinceScannedRows",
  "localPushChangedSinceScanLimitHits",
  "localPushChangedSinceMaxScannedRows",
  "clockSkewDetected",
  "nativeClockOffsetMs",
  "nativeClockObservedAtMs",
  "code"
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
    pendingQueryFetchCollectors: 0,
    queuedQueryFetchRequests: 0,
    maxPendingQueryFetchCollectors: 0,
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
    pendingFileFetchCollectors: 0,
    maxPendingFileFetchCollectors: 0,
    fileBytesReceived: 0,
    fileStreamErrors: 0,
    fileStreamDedupHits: 0,
    lastFileFetchMs: null,
    localPushChangedSinceCalls: 0,
    localPushChangedSinceScannedRows: 0,
    localPushChangedSinceScanLimitHits: 0,
    localPushChangedSinceMaxScannedRows: 0,
    clockSkewDetected: false,
    nativeClockOffsetMs: 0,
    nativeClockObservedAtMs: null,
    code: null
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
var CLAIM_ELECTION_MS = 25;
var CLAIM_RENEW_MS = 1e4;
function createBroadcastChannelBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  if (!databaseName) throw new TypeError("broker requires databaseName");
  if (typeof globalThis.BroadcastChannel !== "function") {
    return createMemoryBroker({ databaseName, tabId, clock });
  }
  const channel = new globalThis.BroadcastChannel(`${CHANNEL_PREFIX}${databaseName}`);
  const localClaims = /* @__PURE__ */ new Map();
  const remoteClaims = /* @__PURE__ */ new Map();
  const completions = /* @__PURE__ */ new Map();
  let closed = false;
  function post(message) {
    if (closed) return false;
    try {
      channel.postMessage(message);
      return true;
    } catch {
      return false;
    }
  }
  channel.onmessage = (event) => {
    const msg = event?.data;
    if (!msg || typeof msg !== "object") return;
    const now = clock();
    if (msg.type === "claim") {
      remoteClaims.set(msg.windowKey, { tabId: msg.tabId, expiresAt: now + CLAIM_TTL_MS });
      const local = localClaims.get(msg.windowKey);
      if (local && String(msg.tabId) < String(tabId)) {
        clearInterval(local.renewTimer);
        localClaims.delete(msg.windowKey);
      }
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
    get closed() {
      return closed;
    },
    async claim(windowKey) {
      if (closed) return false;
      const now = clock();
      const remote = remoteClaims.get(windowKey);
      if (remote && expired(remote, now)) {
        remoteClaims.delete(windowKey);
      } else if (remote) {
        return false;
      }
      const local = localClaims.get(windowKey);
      if (local && !expired(local, now)) return false;
      const renewTimer = setInterval(() => {
        const claim = localClaims.get(windowKey);
        if (!claim) return;
        claim.expiresAt = clock() + CLAIM_TTL_MS;
        if (!post({ type: "claim", windowKey, tabId, at: clock(), renewal: true })) {
          clearInterval(claim.renewTimer);
          localClaims.delete(windowKey);
        }
      }, CLAIM_RENEW_MS);
      localClaims.set(windowKey, { expiresAt: now + CLAIM_TTL_MS, renewTimer });
      if (!post({ type: "claim", windowKey, tabId, at: now })) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, CLAIM_ELECTION_MS));
      if (closed) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      const contender = remoteClaims.get(windowKey);
      if (contender && !expired(contender, clock()) && String(contender.tabId) < String(tabId)) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      return true;
    },
    async release(windowKey, result = null) {
      const local = localClaims.get(windowKey);
      if (local) clearInterval(local.renewTimer);
      localClaims.delete(windowKey);
      post({ type: "complete", windowKey, tabId, result, at: clock() });
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
      if (closed) return;
      for (const [windowKey, claim] of localClaims.entries()) {
        clearInterval(claim.renewTimer);
        post({ type: "release", windowKey, tabId, at: clock(), reason: "broker-close" });
      }
      localClaims.clear();
      for (const waiter of completions.values()) waiter.resolve(null);
      completions.clear();
      closed = true;
      channel.onmessage = null;
      try {
        channel.close();
      } catch {
      }
    }
  };
}
function createMemoryBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  const claims = /* @__PURE__ */ new Set();
  let closed = false;
  return {
    kind: "memory",
    tabId,
    get closed() {
      return closed;
    },
    async claim(windowKey) {
      if (closed) return false;
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
      closed = true;
      claims.clear();
    }
  };
}
function randomTabId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `tab-${Math.random().toString(36).slice(2, 12)}`;
}

// src/apps/business-os/rxdb/src/replication-webrtc.mjs
var ACTIVE_COLLECTIONS_METHOD = "rxdb.activeCollections";
var GLOBAL_QUERY_META_BUDGET_BYTES = 512 * 1024 * 1024;
var DEFAULT_QUERY_META_BUDGET_BYTES = 6 * 1024 * 1024;
var LOCAL_WRITE_PUSH_DEBOUNCE_MS = 50;
var BROWSER_CAPABILITIES = [
  "ctox-rxdb-browser-v1",
  "ctox-file-chunks-v1",
  "ctox-schema-hash-v1",
  "ctox-peer-session-v1",
  "ctox-checkpoint-epoch-v1",
  CTOX_CHECKPOINT_GENERATION_CAPABILITY,
  CTOX_APP_RUNTIME_CAPABILITY,
  CTOX_QUERY_FETCH_CAPABILITY,
  CTOX_PRESENCE_CAPABILITY,
  CTOX_COMMAND_LIFECYCLE_CAPABILITY
];
function remoteSupportsPresence(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== "object") return false;
  const capabilities = Array.isArray(remoteProtocol.capabilities) ? remoteProtocol.capabilities : [];
  return capabilities.includes(CTOX_PRESENCE_CAPABILITY);
}
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
var SHARED_PEER_OPEN_WAIT_MS = 6e4;
var SHARED_PROTOCOL_COLLECTION_CONCURRENCY = 8;
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
  changeEventHasOnlyReplicationOriginWrites,
  terminalPushRejection,
  sharedRoomPeerKey,
  stableSignalingUrlKey,
  shouldAttachQueryDemandLoader,
  shouldAttachFileDemandLoader,
  shouldPersistFetchedFileChunks,
  // SYNC-12: read-permission digest change-detector for checkpoint reuse.
  decodeCapabilityTokenClaims,
  readPermissionDigestFromCapabilityToken,
  readPermissionDigestMatches,
  // Lazy accessor (class is declared below): lets the activation-catch-up
  // smoke drive the real SharedRoomPeer registry wiring without a network.
  getSharedRoomPeerClass: () => SharedRoomPeer
});
function isTransientSharedPeerError(error) {
  const message = String(error?.message || error || "");
  return message.includes(" is not open") || message.includes("WebRTC peer") || message.includes("Peer closed") || message.includes("peer closed") || message.includes("channel-close") || message.includes("Timed out waiting for WebRTC response ctoxProtocol");
}
var SharedRoomPeer = class {
  constructor({ key, signalingUrl, room, iceServers, iceServersRefreshUrl, refreshIceServers, expectedNativePeerId }) {
    this.key = key;
    this.signalingUrl = signalingUrl;
    this.room = room;
    this.iceServers = iceServers;
    this.iceServersRefreshUrl = iceServersRefreshUrl || "";
    this.refreshIceServers = typeof refreshIceServers === "function" ? refreshIceServers : null;
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
    this.lastActiveCollectionsSet = null;
    this.presenceRegistry = getPresenceRegistry();
    this.presenceUnsub = null;
    this.presenceCapable = false;
  }
  representativeCollection() {
    const first = this.collections.keys().next();
    return first.done ? null : this.collections.get(first.value);
  }
  register(collection, registration) {
    const isNewCollection = !this.collections.has(collection);
    this.collections.set(collection, registration);
    this.refCount += 1;
    if (isNewCollection) {
      this.schemaMismatchCollections.delete(collection);
      if (this.negotiated) {
        this.negotiated = null;
      }
    }
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
      if (this.presenceUnsub) {
        try {
          this.presenceUnsub();
        } catch {
        }
        this.presenceUnsub = null;
      }
      this.presenceCapable = false;
      try {
        this.presenceRegistry.applyRemote([]);
      } catch {
      }
    }
  }
  abortPeerRequests(peerId, reason = "peer-close") {
    return this.demandTransport?.abortPeerRequests?.(peerId, reason) || 0;
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
      iceServersRefreshUrl: this.iceServersRefreshUrl,
      refreshIceServers: this.refreshIceServers,
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
      try {
        this.demandTransport.abortPeerRequests(event.detail?.peerId, event.detail?.reason || "peer-close");
      } catch {
      }
      if (this.negotiated && this.negotiated.peerId === event.detail?.peerId) {
        this.negotiated = null;
      }
      if (this.activeRemotePeerId === event.detail?.peerId) {
        this.activeRemotePeerId = null;
        this.presenceCapable = false;
        try {
          this.presenceRegistry.applyRemote([]);
        } catch {
        }
      }
      this.fanout("peer-close", event.detail);
    });
    this.peer.on("peer-state", (event) => this.fanout("peer-state", event.detail));
    this.peer.on("master-change", (event) => {
      const collection = event.detail?.collection || event.collection || null;
      this.fanoutMasterChange(collection);
    });
    this.peer.on("presence", (event) => {
      const entries = event.detail?.entries ?? event.entries ?? [];
      try {
        this.presenceRegistry.applyRemote(entries);
      } catch {
      }
    });
    if (!this.presenceUnsub) {
      this.presenceUnsub = this.presenceRegistry.onLocalChange((entries) => {
        this.sendPresenceUpdate(entries);
      });
    }
    if (!this.activeRegistryUnsub) {
      this.activeRegistryUnsub = this.activeRegistry.onChange((names) => {
        const previous = this.lastActiveCollectionsSet || /* @__PURE__ */ new Set();
        const current = new Set(Array.isArray(names) ? names : []);
        this.lastActiveCollectionsSet = current;
        this.sendActiveCollections(names);
        for (const name of current) {
          if (previous.has(name)) continue;
          const registration = this.collections.get(name);
          try {
            registration?.state?.onMasterChange?.();
          } catch {
          }
        }
      });
    }
    return this.peer;
  }
  // Presence: send `rxdb.presence.update` (fire-and-forget) to the active
  // native peer. Gated on the handshake capability so a pre-presence native
  // peer never sees the unknown method. No-op until a peer is open; resent on
  // (re)handshake because the native hub drops per-peer presence on
  // disconnect.
  sendPresenceUpdate(entries) {
    if (!this.presenceCapable) return;
    const peerId = this.activeRemotePeerId;
    if (!peerId || !this.peer) return;
    const list = Array.isArray(entries) ? entries : this.presenceRegistry.localEntries();
    try {
      this.peer.send(peerId, {
        id: `presence-update|${Date.now()}`,
        method: CTOX_PRESENCE_RPC.update,
        params: [list]
      });
    } catch {
    }
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
    return this.collectCollectionMap(async (name, registration) => {
      const state = registration.state;
      if (!state) return null;
      let hash = state.schemaHashValue;
      if (!hash) {
        try {
          hash = await state.collection.schema.hash();
        } catch {
          hash = null;
        }
      }
      return [name, {
        schemaVersion: state.collection?.schema?.version ?? null,
        schemaHash: hash || null,
        schemaHashSource: schemaHashSource(name)
      }];
    });
  }
  async collectCollectionCheckpoints() {
    return this.collectCollectionMap(async (name, registration) => {
      const state = registration.state;
      if (!state) return null;
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
          return [name, {
            ...checkpoint,
            collection: checkpoint.collection || name
          }];
        }
      } catch {
      }
      return null;
    });
  }
  async collectCollectionMap(mapper) {
    const entries = [...this.collections.entries()];
    const results = new Array(entries.length);
    let nextIndex = 0;
    const workerCount = Math.min(SHARED_PROTOCOL_COLLECTION_CONCURRENCY, entries.length);
    await Promise.all(Array.from({ length: workerCount }, async () => {
      while (nextIndex < entries.length) {
        const index = nextIndex;
        nextIndex += 1;
        const [name, registration] = entries[index];
        results[index] = await mapper(name, registration);
      }
    }));
    const map = {};
    for (const result of results) {
      if (!Array.isArray(result) || !result[0]) continue;
      map[result[0]] = result[1];
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
        ...schema || {},
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
    this.presenceCapable = remoteSupportsPresence(normalizedRemoteProtocol);
    this.sendPresenceUpdate();
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
    return {
      ...this.peer?.getTransportStatus?.() || {},
      demandTransport: this.demandTransport?.diagnostics?.() || null
    };
  }
};
function getOrCreateSharedRoomPeer({ signalingUrl, room, iceServers, iceServersRefreshUrl, refreshIceServers, expectedNativePeerId }) {
  const key = sharedRoomPeerKey(signalingUrl, room);
  let shared = SHARED_ROOM_PEERS.get(key);
  if (!shared) {
    shared = new SharedRoomPeer({ key, signalingUrl, room, iceServers, iceServersRefreshUrl, refreshIceServers, expectedNativePeerId });
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
    this.pushAgainAfterCurrent = false;
    this.pullRetryTimer = null;
    this.pushRetryTimer = null;
    this.localPushTimer = null;
    this.checkpointStorageKey = persistentCheckpointStorageKey(topic, collection.name);
    this.retainedCheckpoints = readPersistentCheckpoints(this.checkpointStorageKey);
    this.localCheckpointValidityKey = "";
    this.readPermissionDigest = "";
    this.activeRemotePeerId = null;
    this.demandLoaderActive = false;
    this.demandStatus = createV1_5StatusState();
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
      iceServersRefreshUrl: connectionHandlerCreator?.config?.iceServersRefreshUrl || "",
      refreshIceServers: connectionHandlerCreator?.config?.refreshIceServers || null,
      expectedNativePeerId: this.ctox?.expectedNativePeerId || ""
    });
    this.shared.register(this.collection.name, {
      collection: this.collection.name,
      state: this
    });
    this.shared.start();
    this.changeSubscription = this.collection.observe((event) => {
      if (changeEventHasOnlyReplicationOriginWrites(event)) {
        return;
      }
      this.scheduleLocalWritePush();
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
      if (["closed", "failed"].includes(stateName)) {
        this.removePeer(detail?.peerId, `peer-${stateName}`);
      }
    }
  }
  onMasterChange() {
    if (this.cancelled) return;
    this.pullFromRemotePeers().catch((error) => {
      this.error$.next(error);
      this.schedulePullRetry();
    });
  }
  emitError(error) {
    this.error$.next(error);
  }
  async buildProtocolPayload() {
    const checkpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    const capabilityToken = await resolveCapabilityToken(this.ctox);
    return buildProtocolPayload({
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema.version,
      schemaHash: this.schemaHashValue,
      schemaHashSource: schemaHashSource(this.collection.name),
      peerSessionId: `browser:${this.topic}`,
      peerGeneration: 1,
      checkpoint,
      role: "browser",
      capabilities: BROWSER_CAPABILITIES,
      capabilityToken: typeof capabilityToken === "string" ? capabilityToken : null
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
    if (Number.isFinite(normalizedRemoteProtocol?.nativeTimeMs)) {
      Object.assign(
        this.demandStatus,
        setHybridLogicalClockTimeAnchor(normalizedRemoteProtocol.nativeTimeMs, Date.now())
      );
    }
    this.activeRemotePeerId = peerId;
    this.demandStatus.peerConnected = true;
    this.demandStatus.peerCapabilityQueryFetchV1 = queryFetchCapable === true;
    const validityKey = checkpointValidityKeyFromProtocol(normalizedRemoteProtocol);
    const localCheckpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    const localValidityKey = localCheckpointValidityKey(localCheckpoint);
    this.localCheckpointValidityKey = localValidityKey;
    const readPermissionDigest = await this.resolveReadPermissionDigest();
    const retained = this.retainedCheckpoints;
    if (retained && validityKey) {
      if (retained.validityKey === validityKey && retained.localValidityKey && retained.localValidityKey === localValidityKey && readPermissionDigestMatches(retained.permissionDigest, readPermissionDigest)) {
        if (retained.pull && !this.pullCheckpointsByPeer.has(peerId)) {
          this.pullCheckpointsByPeer.set(peerId, retained.pull);
        }
        if (retained.push && !this.pushCheckpointsByPeer.has(peerId)) {
          this.pushCheckpointsByPeer.set(peerId, retained.push);
        }
      } else {
        this.retainedCheckpoints = null;
        clearPersistentCheckpoints(this.checkpointStorageKey);
      }
    }
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
        if (results.some((result) => result.status === "rejected")) {
          this.schedulePullRetry();
        }
      } while (this.pullAgainAfterCurrent && !this.cancelled);
    })().finally(() => {
      this.pullInProgress = false;
      this.pullInProgressPromise = null;
      this.pullAgainAfterCurrent = false;
    });
    return this.pullInProgressPromise;
  }
  // Pulls are otherwise purely event-driven (`masterChangeStream$`): a pull
  // that failed past its in-band attempts was simply LOST until the next
  // remote write produced a new master-change event — a quiet collection
  // stayed stale until reload. Re-arm a single retry timer with the
  // configured `retryTime` (which was previously stored and never read).
  schedulePullRetry() {
    if (this.cancelled || this.pullRetryTimer) return;
    this.pullRetryTimer = setTimeout(() => {
      this.pullRetryTimer = null;
      if (this.cancelled) return;
      this.pullFromRemotePeers().catch((error) => {
        this.error$.next(error);
        this.schedulePullRetry();
      });
    }, Math.max(1e3, Number(this.retryTime) || 5e3));
  }
  schedulePushRetry() {
    if (this.cancelled || this.pushRetryTimer) return;
    this.pushRetryTimer = setTimeout(() => {
      this.pushRetryTimer = null;
      if (this.cancelled) return;
      this.pushToRemotePeers().catch((error) => {
        this.error$.next(error);
        this.schedulePushRetry();
      });
    }, Math.max(1e3, Number(this.retryTime) || 5e3));
  }
  scheduleLocalWritePush() {
    if (this.cancelled || !this.push || this.localPushTimer) return;
    this.localPushTimer = setTimeout(() => {
      this.localPushTimer = null;
      if (this.cancelled) return;
      this.pushToRemotePeers().catch((error) => this.error$.next(error));
    }, LOCAL_WRITE_PUSH_DEBOUNCE_MS);
    this.localPushTimer.unref?.();
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
        await this.collection.storageCollection.bulkWrite(documents, {
          replicationOrigin: this.replicationOriginForPeer(activePeerId)
        });
        await this.invalidateDemandCacheForRemoteWrite(documents);
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pullCheckpointsByPeer.set(activePeerId, checkpoint);
      await this.persistCheckpointsForPeer(activePeerId);
      if (!documents.length) break;
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
    if (this.pushInProgressPromise) {
      this.pushAgainAfterCurrent = true;
      return this.pushInProgressPromise;
    }
    this.pushInProgress = true;
    this.pushAgainAfterCurrent = false;
    this.pushInProgressPromise = (async () => {
      try {
        do {
          this.pushAgainAfterCurrent = false;
          const peerIds = this.openPeerIds();
          const results = await Promise.allSettled(peerIds.map((peerId) => this.pushToPeer(peerId)));
          this.reportPeerResults(results, peerIds);
          if (results.some((result) => result.status === "rejected")) {
            this.schedulePushRetry();
          }
        } while (this.pushAgainAfterCurrent && !this.cancelled);
      } finally {
        this.pushInProgress = false;
        this.pushInProgressPromise = null;
        this.pushAgainAfterCurrent = false;
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
      this.recordLocalPushChangedSinceRead(result, documents);
      for (const document2 of documents) {
        const id = primaryValue(document2, this.collection.schema.primaryPath);
        if (id) await this.demandSidecar?.markDirty?.(this.collection.name, id, true);
      }
      if (!documents.length) {
        const nextCheckpoint = result?.checkpoint || checkpoint;
        if (result?.scanLimitReached && nextCheckpoint && checkpointKey(nextCheckpoint) !== checkpointKey(checkpoint)) {
          checkpoint = nextCheckpoint;
          this.pushCheckpointsByPeer.set(peerId, checkpoint);
          await this.persistCheckpointsForPeer(peerId);
          continue;
        }
        checkpoint = nextCheckpoint;
        this.pushCheckpointsByPeer.set(peerId, checkpoint);
        await this.persistCheckpointsForPeer(peerId);
        break;
      }
      let rows = documents.map((doc) => ({
        newDocumentState: doc,
        assumedMasterState: null
      }));
      let terminalRejection = null;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const masterWriteResult = await this.peer.request(
          peerId,
          "masterWrite",
          [rows],
          this.requestTimeoutMsFor("masterWrite"),
          this.collection.name
        );
        terminalRejection = terminalPushRejection(masterWriteResult);
        if (terminalRejection) {
          rows = [];
          break;
        }
        const conflicts = masterWriteResult;
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
        if (this.collection.storageCollection?.conflictStrategy !== "field-merge") {
          rows = await this.resolveWholeDocumentLwwConflicts(rows, peerId);
          if (!rows.length) break;
        }
        rows = await this.absorbMasterStateIntoConflictRows(rows);
      }
      if (terminalRejection) {
        await this.reconcileTerminalPushRejection(documents, peerId, terminalRejection);
        checkpoint = result?.checkpoint || checkpoint;
        this.pushCheckpointsByPeer.set(peerId, checkpoint);
        await this.persistCheckpointsForPeer(peerId);
        if (documents.length < batchSize) break;
        continue;
      }
      if (rows.length) {
        rows = await this.absorbAuthoritativeCommandConflicts(rows, peerId);
      }
      if (rows.length) {
        throw new Error(`masterWrite conflicts remained for ${this.collection.name}`);
      }
      for (const document2 of documents) {
        const id = primaryValue(document2, this.collection.schema.primaryPath);
        if (id) await this.demandSidecar?.markDirty?.(this.collection.name, id, false);
      }
      checkpoint = result?.checkpoint || checkpoint;
      this.pushCheckpointsByPeer.set(peerId, checkpoint);
      await this.persistCheckpointsForPeer(peerId);
      if (documents.length < batchSize) break;
    }
  }
  async resolveWholeDocumentLwwConflicts(rows, peerId) {
    const retryRows = [];
    const acceptedMaster = [];
    const finalDelete = this.collection.storageCollection?.deleteStrategy === "final";
    for (const row of rows) {
      const local = row?.newDocumentState;
      const master = row?.assumedMasterState;
      const nativeAuthoritative = ["business_commands", "ctox_queue_tasks"].includes(this.collection.name);
      if (finalDelete && !nativeAuthoritative) {
        const localDeleted = Boolean(local?._deleted);
        const masterDeleted = Boolean(master?._deleted);
        if (localDeleted && !masterDeleted) {
          retryRows.push(row);
          continue;
        }
        if (masterDeleted && !localDeleted) {
          if (master) acceptedMaster.push(master);
          continue;
        }
      }
      if (isFutureHybridLogicalClock(local?._meta?.ctoxHlc)) {
        await this.collection.storageCollection?.recoveryJournal?.recordConflict?.({
          code: "clock_skew_detected",
          collection: this.collection.name,
          base: row?.base || null,
          local,
          master,
          message: "A local HLC is more than five minutes ahead of the native time reference.",
          clock: hybridLogicalClockStatus()
        });
        if (master) acceptedMaster.push(master);
        continue;
      }
      const order = compareHybridLogicalClocks(
        local?._meta?.ctoxHlc,
        master?._meta?.ctoxHlc
      );
      if (nativeAuthoritative || order < 0) {
        if (master) acceptedMaster.push(master);
      } else {
        retryRows.push(row);
      }
    }
    if (acceptedMaster.length) {
      await this.collection.storageCollection.bulkWrite(acceptedMaster, {
        replicationOrigin: this.replicationOriginForPeer(peerId)
      });
      await this.invalidateDemandCacheForRemoteWrite(acceptedMaster);
    }
    return retryRows;
  }
  recordLocalPushChangedSinceRead(result, documents = []) {
    const scanned = Number.isFinite(Number(result?.scanned)) ? Math.max(0, Number(result.scanned)) : Array.isArray(documents) ? documents.length : 0;
    this.demandStatus.localPushChangedSinceCalls = Number(this.demandStatus.localPushChangedSinceCalls || 0) + 1;
    this.demandStatus.localPushChangedSinceScannedRows = Number(this.demandStatus.localPushChangedSinceScannedRows || 0) + scanned;
    this.demandStatus.localPushChangedSinceMaxScannedRows = Math.max(
      Number(this.demandStatus.localPushChangedSinceMaxScannedRows || 0),
      scanned
    );
    if (result?.scanLimitReached) {
      this.demandStatus.localPushChangedSinceScanLimitHits = Number(this.demandStatus.localPushChangedSinceScanLimitHits || 0) + 1;
    }
  }
  // Field-merge push repair: a masterWrite conflict means the master row
  // moved while our local write was unsynced. For `field-merge` collections
  // we three-way merge (stored base, local doc, master's conflict row),
  // persist the merged doc locally as a LOCAL write (it still carries
  // unsynced state), and retry the push with the merged doc + the master row
  // as assumedMasterState. LWW collections pass through untouched and keep
  // the existing local-wins force retry.
  async absorbMasterStateIntoConflictRows(rows) {
    const storage = this.collection?.storageCollection;
    if (!rows.length || storage?.conflictStrategy !== "field-merge") return rows;
    const primaryPath = this.collection.schema.primaryPath;
    const mergedRows = [];
    for (const row of rows) {
      const id = primaryValue(row.newDocumentState, primaryPath);
      let record = null;
      try {
        record = await storage.getStoredRecord?.(id);
      } catch {
      }
      const { merged, requiresManualResolution, conflictFields } = threeWayMergeDocuments(
        record?.base,
        row.newDocumentState,
        row.assumedMasterState,
        { primaryPath }
      );
      if (requiresManualResolution) {
        const error = new Error(`Structured conflict requires native/manual resolution for ${this.collection.name}: ${conflictFields.join(", ")}`);
        error.code = "structured_conflict_requires_resolution";
        error.collection = this.collection.name;
        error.fields = conflictFields;
        throw error;
      }
      if (storage.mergeStats) storage.mergeStats.pushConflictMerges += 1;
      try {
        await storage.bulkWrite([merged], { baseById: { [id]: row.assumedMasterState } });
      } catch {
      }
      mergedRows.push({ newDocumentState: merged, assumedMasterState: row.assumedMasterState });
    }
    return mergedRows;
  }
  async absorbAuthoritativeCommandConflicts(rows, peerId) {
    if (!rows.length || this.collection.name !== "business_commands") return rows;
    const masterDocs = [];
    const unresolvedRows = [];
    for (const row of rows) {
      if (isStalePendingBusinessCommandConflict(row)) {
        masterDocs.push(row.assumedMasterState);
      } else {
        unresolvedRows.push(row);
      }
    }
    if (!masterDocs.length) return rows;
    await this.collection.storageCollection.bulkWrite(masterDocs, {
      replicationOrigin: this.replicationOriginForPeer(peerId)
    });
    await this.invalidateDemandCacheForRemoteWrite(masterDocs);
    return unresolvedRows;
  }
  // SYNC-40: reconcile a batch the native peer terminally rejected on push.
  // The rejection is non-retryable (authz/schema), so we roll the local mirror
  // back to master + journal the rejected version, surface a non-fatal sync
  // signal, and pull-and-replace any newer authoritative state. This does NOT
  // throw — throwing would re-arm the infinite push retry the finding is about.
  async reconcileTerminalPushRejection(documents, peerId, rejection) {
    const origin = this.replicationOriginForPeer(peerId) || { role: "ctox_instance", peerId, sessionId: "", collection: this.collection.name };
    let reconciledIds = [];
    try {
      reconciledIds = await this.collection.storageCollection.reconcileRejectedLocalWrites(documents, {
        origin,
        code: rejection.code,
        message: rejection.message
      });
    } catch (error) {
      this.error$.next(error);
    }
    this.error$.next({
      code: "ctox_replication_push_rejected",
      phase: "replication-io",
      direction: "push",
      terminal: true,
      collection: this.collection.name,
      rejectionKind: rejection.kind,
      rejectionCode: rejection.code,
      reconciledCount: reconciledIds.length,
      message: rejection.message
    });
    this.pullFromRemotePeers().catch((error) => this.error$.next(error));
    return reconciledIds;
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
      await this.collection.storageCollection.bulkWrite(docs, {
        replicationOrigin: this.replicationOriginForPeer(peerId)
      });
      await this.invalidateDemandCacheForRemoteWrite(docs);
    }
    return [];
  }
  awaitInitialReplication() {
    return this.initialReplication;
  }
  awaitInSync() {
    return Promise.resolve().then(() => this.awaitInitialReplication()).then(() => this.pullFromRemotePeers()).then(() => this.pushToRemotePeers());
  }
  getTransportStatus(options = {}) {
    return this.decorateTransportStatus(this.shared?.getTransportStatus?.(options) || this.transportStatus$.getValue?.() || {});
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
    if (this.pullRetryTimer) {
      clearTimeout(this.pullRetryTimer);
      this.pullRetryTimer = null;
    }
    if (this.pushRetryTimer) {
      clearTimeout(this.pushRetryTimer);
      this.pushRetryTimer = null;
    }
    if (this.localPushTimer) {
      clearTimeout(this.localPushTimer);
      this.localPushTimer = null;
    }
    const shared = this.shared;
    this.shared = null;
    try {
      shared?.unregister?.(this.collection.name);
    } catch {
    }
    if (this.collection?.demandLoader === this.demandLoader) {
      try {
        this.collection.setDemandLoader?.(null);
      } catch {
      }
    }
    try {
      this.demandLoader?.abortAllInFlight?.("replication-cancel");
    } catch {
    }
    try {
      this.demandFileLoader?.abortAllInFlight?.("replication-cancel");
    } catch {
    }
    try {
      this.demandSidecar?.stopEvictionScheduler?.();
    } catch {
    }
    try {
      this.multiTabBroker?.close?.();
    } catch {
    }
    try {
      await this.demandSidecar?.close?.();
    } catch {
    }
    this.demandLoader = null;
    this.demandFileLoader = null;
    this.multiTabBroker = null;
    this.demandLoaderActive = false;
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
    const queryDemandEnabled = shouldAttachQueryDemandLoader(this.collection.name);
    const fileDemandEnabled = shouldAttachFileDemandLoader(this.collection.name);
    if (!queryDemandEnabled && !fileDemandEnabled) {
      this.demandStatus.queryDemandLoadingEnabled = false;
      this.demandStatus.queryDemandLoadingActive = false;
      if (typeof this.collection.setDemandLoader === "function") {
        this.collection.setDemandLoader(null);
      }
      this.demandLoader = null;
      this.demandFileLoader = null;
      this.demandLoaderActive = true;
      return null;
    }
    const dbName = databaseName || `ctox_business_os_v1_5_meta_${this.collection.name}`;
    this.multiTabBroker = createBroadcastChannelBroker({
      databaseName: this.collection.storageCollection?.databaseName || this.topic
    });
    const backend = indexedDbAvailable ? createIndexedDbMetaBackend({ databaseName: dbName }) : createMemoryMetaBackend();
    this.demandStatus.queryDemandLoadingEnabled = queryDemandEnabled || fileDemandEnabled;
    const primaryDelete = async (collection, id) => {
      if (collection !== this.collection.name) return;
      const stored = await this.collection.storageCollection.getStoredRecord?.(id);
      if (!stored || Number(stored.pushable || 0) !== 0) {
        throw new Error(`Refusing to evict locally-unsynced ${collection}/${id}`);
      }
      if (typeof this.collection.storageCollection.hardDeleteByIds === "function") {
        await this.collection.storageCollection.hardDeleteByIds([id]);
      }
    };
    this.demandSidecar = new QueryMetaStorage(backend, {
      databaseName: dbName,
      schedulerKey: this.collection.storageCollection?.databaseName || this.topic,
      primaryDelete
    });
    try {
      await this.demandSidecar.setBudgetBytes(DEFAULT_QUERY_META_BUDGET_BYTES);
    } catch {
    }
    try {
      this.demandSidecar.startEvictionScheduler({
        intervalMs: 3e4,
        globalBudgetBytes: GLOBAL_QUERY_META_BUDGET_BYTES,
        shareBudgetBytes: DEFAULT_QUERY_META_BUDGET_BYTES
      });
    } catch {
    }
    const demandReplicationOrigin = () => this.replicationOriginForPeer(this.activeRemotePeerId) || { role: "ctox_instance", peerId: this.activeRemotePeerId || "", sessionId: "", collection: this.collection.name };
    this.demandLoader = queryDemandEnabled ? createQueryDemandLoader({
      storageCollection: this.collection.storageCollection,
      sidecar: this.demandSidecar,
      collectionName: this.collection.name,
      schemaVersion: this.collection.schema?.version || 0,
      requestQueryFetch: (envelope) => demandTransport.requestQueryFetch(envelope),
      requestCancel: ({ requestId, reason }) => demandTransport.requestQueryCancel({ requestId, reason }),
      status: this.demandStatus,
      multiTabBroker: this.multiTabBroker,
      replicationOrigin: demandReplicationOrigin
    }) : null;
    if (typeof this.collection.setDemandLoader === "function") {
      this.collection.setDemandLoader(this.demandLoader);
    }
    this.demandFileLoader = fileDemandEnabled ? createFileDemandLoader({
      collectionName: this.collection.name,
      storageCollection: this.collection.storageCollection,
      sidecarBackend: backend,
      persistChunks: shouldPersistFetchedFileChunks(this.collection.name),
      replicationOrigin: demandReplicationOrigin,
      requestFileFetch: ({ requestId, fileId, range, knownSequences, onChunk }) => demandTransport.requestFileFetch({
        requestId,
        fileId,
        range,
        knownSequences,
        onChunk,
        collectionName: this.collection.name
      }),
      requestFileCancel: ({ requestId, reason }) => demandTransport.requestFileCancel({ requestId, reason }),
      status: this.demandStatus
    }) : null;
    this.demandLoaderActive = true;
    this.demandStatus.queryDemandLoadingActive = queryDemandEnabled || fileDemandEnabled;
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
    const validityKey = this.checkpointValidityKeyForPeer(peerId);
    const retainedPull = this.pullCheckpointsByPeer.get(peerId) || null;
    const retainedPush = this.pushCheckpointsByPeer.get(peerId) || null;
    if (validityKey && this.localCheckpointValidityKey && (retainedPull || retainedPush)) {
      this.retainedCheckpoints = {
        validityKey,
        localValidityKey: this.localCheckpointValidityKey,
        // SYNC-12: stamp the read-permission digest last resolved at handshake
        // (removePeer is synchronous — the cached value is the current identity).
        permissionDigest: this.readPermissionDigest,
        pull: retainedPull,
        push: retainedPush
      };
      writePersistentCheckpoints(this.checkpointStorageKey, this.retainedCheckpoints);
    }
    peerStates.delete(peerId);
    this.pullCheckpointsByPeer.delete(peerId);
    this.pushCheckpointsByPeer.delete(peerId);
    this.peerStates$.next(peerStates);
    try {
      this.demandLoader?.abortAllInFlight?.(`peer-${reason}`);
    } catch {
    }
    try {
      this.demandFileLoader?.abortAllInFlight?.(`peer-${reason}`);
    } catch {
    }
    try {
      this.shared?.abortPeerRequests?.(peerId, reason);
    } catch {
    }
    if (!peerStates.size) {
      this.demandStatus.peerConnected = false;
      this.active$.next(false);
    }
    this.ctox?.onPeerClose?.({ peerId, reason });
  }
  // Checkpoints are only reusable against the SAME native storage generation.
  // Against `ctox-checkpoint-generation-v2` peers the validity key is the
  // PERSISTENT native storage generation + collection + schema hash, so
  // retained checkpoints survive daemon restarts and resume incrementally;
  // only a storage reset or schema change forces a full re-pull. Mixed-version
  // (v1) peers keep the conservative epoch + peer-session key, where a daemon
  // restart mints a new sessionId and the full resync is intentional (docs §8).
  checkpointValidityKeyForPeer(peerId) {
    const remoteProtocol = this.remoteProtocolForPeer(peerId);
    return checkpointValidityKeyFromProtocol(remoteProtocol);
  }
  // SYNC-12: resolve and cache a stable, non-secret digest of this browser's
  // effective read-permission identity. The capability token is a native-signed
  // `base64url(payload).base64url(sig)` value whose payload carries `uid`, `role`
  // and `epoch` (capability_epoch); a role change or grant change bumps the epoch
  // and issues a new token, so the digest changes. We hash ONLY the permission
  // claims (never the raw bearer token / signature), so an ordinary token refresh
  // that reissues the same role+epoch with fresh iat/exp keeps the digest stable
  // and incremental resume is preserved. An unresolvable/absent token yields ''
  // (unknown identity — no forced re-pull), which the match helper treats
  // permissively so a transient token-endpoint blip never storms a full resync.
  async resolveReadPermissionDigest() {
    let digest = "";
    try {
      const token = await resolveCapabilityToken(this.ctox);
      digest = await readPermissionDigestFromCapabilityToken(token);
    } catch {
      digest = "";
    }
    this.readPermissionDigest = digest;
    return digest;
  }
  async persistCheckpointsForPeer(peerId) {
    const validityKey = this.checkpointValidityKeyForPeer(peerId);
    if (!validityKey) return;
    const localCheckpoint = await this.collection.storageCollection.replicationCheckpointStatus(this.schemaHashValue);
    const localValidityKey = localCheckpointValidityKey(localCheckpoint);
    if (!localValidityKey) return;
    this.localCheckpointValidityKey = localValidityKey;
    const retained = {
      validityKey,
      localValidityKey,
      // SYNC-12: keep the browser read-permission digest with the checkpoint so
      // a later role/epoch change invalidates reuse and forces a full re-pull.
      permissionDigest: this.readPermissionDigest,
      pull: this.pullCheckpointsByPeer.get(peerId) || null,
      push: this.pushCheckpointsByPeer.get(peerId) || null,
      collection: this.collection.name,
      schemaHash: this.schemaHashValue || "",
      updatedAtMs: Date.now()
    };
    this.retainedCheckpoints = retained;
    writePersistentCheckpoints(this.checkpointStorageKey, retained);
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
  async invalidateDemandCacheForRemoteWrite(changedDocuments = []) {
    try {
      const ids = changedDocuments.map((doc) => primaryValue(doc, this.collection.schema.primaryPath)).filter(Boolean);
      if (typeof this.demandLoader?.invalidateDocuments === "function") {
        await this.demandLoader.invalidateDocuments(changedDocuments);
      } else if (typeof this.demandLoader?.invalidateDocumentChange === "function") {
        await this.demandLoader.invalidateDocumentChange(ids);
      } else {
        await this.demandLoader?.invalidateCollectionChange?.();
      }
    } catch {
    }
  }
  requestTimeoutMsFor(method) {
    if (this.collection.name === "desktop_file_chunks") {
      return method === "masterChangesSince" ? 45e3 : 3e4;
    }
    if (method === "masterWrite") {
      if ([
        "business_commands",
        "ctox_queue_tasks",
        "business_chats",
        "research_runs",
        "research_notes",
        "knowledge_items"
      ].includes(this.collection.name)) {
        return 6e4;
      }
      return 45e3;
    }
    return 15e3;
  }
  periodicPullIntervalMs() {
    if (!this.pull) return 0;
    return ["business_commands", "ctox_queue_tasks"].includes(this.collection.name) ? 1e3 : 0;
  }
  periodicPushIntervalMs() {
    if (!this.push) return 0;
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
    const demandTransport = status.demandTransport || this.shared?.demandTransport?.diagnostics?.() || null;
    if (demandTransport) {
      this.demandStatus.pendingQueryFetchCollectors = Number(demandTransport.pendingQueryCollectors || 0);
      this.demandStatus.pendingFileFetchCollectors = Number(demandTransport.pendingFileCollectors || 0);
      this.demandStatus.queuedQueryFetchRequests = Number(demandTransport.queuedQueryRequests || 0);
      this.demandStatus.maxPendingQueryFetchCollectors = Math.max(
        Number(this.demandStatus.maxPendingQueryFetchCollectors || 0),
        Number(demandTransport.maxPendingQueryCollectors || 0)
      );
      this.demandStatus.maxPendingFileFetchCollectors = Math.max(
        Number(this.demandStatus.maxPendingFileFetchCollectors || 0),
        Number(demandTransport.maxPendingFileCollectors || 0)
      );
    }
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
      demandLoading: snapshotV1_5Status(this.demandStatus),
      demandTransport,
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
function checkpointValidityKeyFromProtocol(remoteProtocol) {
  if (!remoteProtocol || typeof remoteProtocol !== "object") return "";
  const capabilities = Array.isArray(remoteProtocol.capabilities) ? remoteProtocol.capabilities : [];
  const storageGeneration = typeof remoteProtocol.storageGeneration === "string" ? remoteProtocol.storageGeneration.trim() : "";
  const epoch = typeof remoteProtocol.checkpoint?.epoch === "string" ? remoteProtocol.checkpoint.epoch.trim() : "";
  const sessionId = typeof remoteProtocol.peerSession?.sessionId === "string" ? remoteProtocol.peerSession.sessionId.trim() : "";
  const schemaHashValue = String(
    remoteProtocol.collection?.schemaHash || remoteProtocol.schemaHash || remoteProtocol.schema?.hash || ""
  ).trim();
  if (capabilities.includes(CTOX_CHECKPOINT_GENERATION_CAPABILITY) && storageGeneration && schemaHashValue) {
    const collectionName = String(remoteProtocol.collection?.name || "").trim();
    return `${storageGeneration}|${collectionName}|${schemaHashValue}`;
  }
  if (!epoch || !sessionId || !schemaHashValue) return "";
  return `${epoch}|${sessionId}|${schemaHashValue}`;
}
function localCheckpointValidityKey(checkpoint) {
  if (!checkpoint || typeof checkpoint !== "object") return "";
  const epoch = typeof checkpoint.epoch === "string" ? checkpoint.epoch.trim() : "";
  const schemaHashValue = typeof checkpoint.schemaHash === "string" ? checkpoint.schemaHash.trim() : "";
  if (!epoch) return "";
  return `${epoch}|${schemaHashValue}`;
}
function persistentCheckpointStorageKey(topic, collection) {
  return `ctox.rxdb.checkpoints.v1.${encodeURIComponent(String(topic || ""))}.${encodeURIComponent(String(collection || ""))}`;
}
function readPersistentCheckpoints(key) {
  try {
    const parsed = JSON.parse(globalThis.localStorage?.getItem?.(key) || "null");
    if (!parsed || typeof parsed !== "object" || !parsed.validityKey) return null;
    return parsed;
  } catch {
    return null;
  }
}
function writePersistentCheckpoints(key, value) {
  try {
    globalThis.localStorage?.setItem?.(key, JSON.stringify(value));
  } catch {
  }
}
function clearPersistentCheckpoints(key) {
  try {
    globalThis.localStorage?.removeItem?.(key);
  } catch {
  }
}
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
function terminalPushRejection(result) {
  if (!result || typeof result !== "object" || Array.isArray(result)) return null;
  if (result.type !== "ctoxError" || result.scope !== "replication") return null;
  const message = String(result.message || "");
  const code = String(result.code || "");
  const status = String(result.status ?? "");
  const isAuthz = /not authorized/i.test(message) || /authz/i.test(code);
  const isSchema = /schema/i.test(message) || /schema/i.test(code) || status === "422" || /\b422\b/.test(message) || /422/.test(code);
  if (!isAuthz && !isSchema) return null;
  return {
    kind: isAuthz ? "authz" : "schema",
    code: code || "RC_WEBRTC_PEER",
    direction: String(result.direction || "push"),
    collection: String(result.collection || ""),
    message: message || code || "terminal replication rejection"
  };
}
function changeEventHasOnlyReplicationOriginWrites(event) {
  const docs = Object.values(event?.success || {});
  return docs.length > 0 && docs.every((doc) => Boolean(doc?._meta?.ctoxReplicationOrigin?.role));
}
function isStalePendingBusinessCommandConflict(row = {}) {
  const local = row?.newDocumentState;
  const master = row?.assumedMasterState;
  if (!local || !master || typeof local !== "object" || typeof master !== "object") return false;
  const localStatus = String(local.status || "").trim();
  const masterStatus = String(master.status || "").trim();
  return localStatus === "pending_sync" && masterStatus && masterStatus !== "pending_sync";
}
async function resolveCapabilityToken(ctox = {}) {
  if (typeof ctox?.capabilityTokenProvider === "function") {
    try {
      const token = await ctox.capabilityTokenProvider();
      return typeof token === "string" && token.trim() ? token.trim() : null;
    } catch {
      return null;
    }
  }
  const source = ctox?.capabilityToken;
  if (typeof source === "function") {
    try {
      const token = await source();
      return typeof token === "string" && token.trim() ? token.trim() : null;
    } catch {
      return null;
    }
  }
  return typeof source === "string" && source.trim() ? source.trim() : null;
}
function decodeCapabilityTokenClaims(token) {
  if (typeof token !== "string" || !token) return null;
  const dot = token.indexOf(".");
  const payloadB64 = dot === -1 ? token : token.slice(0, dot);
  if (!payloadB64) return null;
  try {
    const json = base64UrlDecodeToString(payloadB64);
    const payload = JSON.parse(json);
    if (!payload || typeof payload !== "object") return null;
    return {
      uid: typeof payload.uid === "string" ? payload.uid : "",
      role: typeof payload.role === "string" ? payload.role : "",
      epoch: Number.isFinite(payload.epoch) ? Number(payload.epoch) : 0
    };
  } catch {
    return null;
  }
}
async function readPermissionDigestFromCapabilityToken(token) {
  const claims = decodeCapabilityTokenClaims(token);
  if (!claims) return "";
  const material = `${claims.uid}|${claims.role}|${claims.epoch}`;
  try {
    return await sha256Hex(material);
  } catch {
    return "";
  }
}
function base64UrlDecodeToString(value) {
  let base64 = String(value).replace(/-/g, "+").replace(/_/g, "/");
  const pad = base64.length % 4;
  if (pad === 2) base64 += "==";
  else if (pad === 3) base64 += "=";
  else if (pad === 1) base64 = base64.slice(0, -1);
  if (typeof globalThis.atob === "function") {
    const binary = globalThis.atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return new TextDecoder().decode(bytes);
  }
  return globalThis.Buffer ? globalThis.Buffer.from(base64, "base64").toString("utf8") : "";
}
function readPermissionDigestMatches(storedDigest, currentDigest) {
  if (!currentDigest) return true;
  return String(storedDigest || "") === currentDigest;
}
function checkpointKey(checkpoint) {
  if (!checkpoint) return "";
  return `${Number(checkpoint.lwt || 0)}\0${String(checkpoint.id || "")}`;
}
function primaryValue(doc = {}, primaryPath = "id") {
  if (!doc || typeof doc !== "object") return "";
  if (doc.id != null) return String(doc.id);
  if (doc._id != null) return String(doc._id);
  return String(replicationValueAtPath(doc, primaryPath) ?? "");
}
function shouldPersistFetchedFileChunks(collectionName = "") {
  return String(collectionName || "") === "desktop_file_chunks";
}
function shouldAttachQueryDemandLoader(collectionName = "") {
  return !String(collectionName || "").endsWith("_chunks");
}
function shouldAttachFileDemandLoader(collectionName = "") {
  return String(collectionName || "") !== "desktop_file_chunks";
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

// src/apps/business-os/rxdb/src/multi-tab-sync-coordinator.mjs
var COORDINATORS = /* @__PURE__ */ Symbol.for("ctox.rxdb.multi-tab-sync-coordinators.v1");
var CHANNEL_PREFIX2 = "ctox-rxdb-sync-leader-";
var HEARTBEAT_MS = 5e3;
var LEASE_TTL_MS = 15e3;
var DIRTY_ACK_TIMEOUT_MS = 1e4;
function getMultiTabSyncCoordinator({ databaseName, room } = {}) {
  const key = `${databaseName || "ctox"}|${room || "default"}`;
  const root = globalThis;
  if (!root[COORDINATORS]) root[COORDINATORS] = /* @__PURE__ */ new Map();
  if (!root[COORDINATORS].has(key) || root[COORDINATORS].get(key)?.isClosed?.()) {
    root[COORDINATORS].set(key, createMultiTabSyncCoordinator({ databaseName, room }));
  }
  return root[COORDINATORS].get(key);
}
function createMultiTabSyncCoordinator({
  databaseName,
  room,
  tabId = globalThis.crypto?.randomUUID?.() || `tab-${Math.random().toString(36).slice(2)}`,
  clock = Date.now
} = {}) {
  if (!databaseName || !room) throw new TypeError("multi-tab sync coordinator requires databaseName and room");
  const listeners = /* @__PURE__ */ new Set();
  const dirtyListeners = /* @__PURE__ */ new Set();
  const externalChangeListeners = /* @__PURE__ */ new Set();
  const pendingDirtyAcks = /* @__PURE__ */ new Map();
  const channel = typeof globalThis.BroadcastChannel === "function" ? new BroadcastChannel(`${CHANNEL_PREFIX2}${databaseName}-${stableHash(room)}`) : null;
  const lockName = `ctox-rxdb-sync:${databaseName}:${stableHash(room)}`;
  let role = "follower";
  let leaderTabId = "";
  let leaderSeenAtMs = 0;
  let started = false;
  let closed = false;
  let heartbeatTimer = null;
  let electionTimer = null;
  let releaseLock = null;
  let lockRequestRunning = false;
  const emitRole = () => {
    const status = snapshot();
    for (const listener of listeners) {
      try {
        listener(status);
      } catch {
      }
    }
    globalThis.dispatchEvent?.(new CustomEvent("ctox-rxdb-multi-tab-status", { detail: status }));
  };
  const post = (message) => {
    try {
      channel?.postMessage({ ...message, tabId, atMs: clock() });
    } catch {
    }
  };
  const handleDirty = async (message) => {
    let error = "";
    try {
      await Promise.all([...dirtyListeners].map((listener) => listener(message)));
    } catch (cause) {
      error = String(cause?.message || cause || "leader push failed").slice(0, 240);
    }
    if (message.requestId) {
      post({
        type: "dirty-ack",
        requestId: message.requestId,
        targetTabId: message.tabId,
        ok: !error,
        error
      });
    }
    if (error && !message.requestId) throw new Error(error);
  };
  const becomeLeader = (reason) => {
    if (closed) return;
    role = "leader";
    leaderTabId = tabId;
    leaderSeenAtMs = clock();
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    heartbeatTimer = setInterval(() => {
      leaderSeenAtMs = clock();
      post({ type: "leader-heartbeat" });
    }, HEARTBEAT_MS);
    heartbeatTimer.unref?.();
    post({ type: "leader-heartbeat", reason });
    emitRole();
  };
  const becomeFollower = (leader = "", reason = "") => {
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    heartbeatTimer = null;
    const changed = role !== "follower" || leader && leader !== leaderTabId;
    role = "follower";
    if (leader) leaderTabId = leader;
    if (changed) emitRole();
    if (reason) post({ type: "follower", reason });
  };
  const tryWebLock = async () => {
    if (closed || lockRequestRunning || !globalThis.navigator?.locks?.request) return false;
    lockRequestRunning = true;
    let resolveAttempt;
    const attempted = new Promise((resolve) => {
      resolveAttempt = resolve;
    });
    navigator.locks.request(lockName, { mode: "exclusive", ifAvailable: true }, async (lock) => {
      if (!lock || closed) {
        lockRequestRunning = false;
        resolveAttempt(false);
        return;
      }
      becomeLeader("web-lock");
      resolveAttempt(true);
      await new Promise((resolve) => {
        releaseLock = resolve;
      });
      releaseLock = null;
      lockRequestRunning = false;
      becomeFollower("", "web-lock-released");
    }).catch(() => {
      lockRequestRunning = false;
      resolveAttempt(false);
    });
    return attempted;
  };
  const attemptElection = async () => {
    if (closed || role === "leader") return;
    if (clock() - leaderSeenAtMs < LEASE_TTL_MS) return;
    if (globalThis.navigator?.locks?.request) {
      await tryWebLock();
      return;
    }
    post({ type: "leader-claim" });
    await delay3(30);
    if (clock() - leaderSeenAtMs >= LEASE_TTL_MS || !leaderTabId || tabId < leaderTabId) {
      becomeLeader("broadcast-election");
    }
  };
  if (channel) {
    channel.onmessage = (event) => {
      const message = event?.data;
      if (!message || message.tabId === tabId) return;
      if (message.type === "leader-heartbeat") {
        leaderSeenAtMs = clock();
        leaderTabId = String(message.tabId || "");
        if (role === "leader" && leaderTabId < tabId) {
          releaseLock?.();
          becomeFollower(leaderTabId, "leader-tiebreak");
        } else if (role !== "leader") {
          becomeFollower(leaderTabId);
        }
      } else if (message.type === "leader-claim") {
        if (role === "leader") post({ type: "leader-heartbeat", reason: "claim-rejected" });
        else if (!leaderTabId || String(message.tabId) < leaderTabId) leaderTabId = String(message.tabId);
      } else if (message.type === "leader-release" && String(message.tabId || "") === leaderTabId) {
        leaderSeenAtMs = 0;
        leaderTabId = "";
        attemptElection().catch(() => {
        });
      } else if (message.type === "dirty" && role === "leader") {
        handleDirty(message).catch(() => {
        });
      } else if (message.type === "dirty-ack" && String(message.targetTabId || "") === tabId) {
        const pending = pendingDirtyAcks.get(String(message.requestId || ""));
        if (pending) {
          pendingDirtyAcks.delete(String(message.requestId || ""));
          clearTimeout(pending.timer);
          if (message.ok === false) pending.reject(new Error(message.error || "Leader could not push the collection."));
          else pending.resolve(message);
        }
      } else if (message.type === "replicated-change" && role === "follower") {
        for (const listener of externalChangeListeners) {
          try {
            listener(message);
          } catch {
          }
        }
        globalThis.dispatchEvent?.(new CustomEvent("ctox-rxdb-external-change", { detail: message }));
      }
    };
  }
  const lifecycleRelease = () => {
    if (role === "leader") {
      post({ type: "leader-release" });
      releaseLock?.();
    }
    becomeFollower("", "page-lifecycle");
  };
  const lifecycleResume = () => attemptElection().catch(() => {
  });
  function start() {
    if (started) return Promise.resolve(snapshot());
    started = true;
    globalThis.document?.addEventListener?.("freeze", lifecycleRelease);
    globalThis.addEventListener?.("pagehide", lifecycleRelease);
    globalThis.document?.addEventListener?.("resume", lifecycleResume);
    globalThis.addEventListener?.("pageshow", lifecycleResume);
    electionTimer = setInterval(() => attemptElection().catch(() => {
    }), HEARTBEAT_MS);
    electionTimer.unref?.();
    return attemptElection().then(snapshot);
  }
  function snapshot() {
    return {
      schema: "ctox.rxdb.multi-tab-sync.v1",
      databaseName,
      role,
      isLeader: role === "leader",
      tabId,
      leaderTabId,
      leaderLeaseAgeMs: leaderSeenAtMs ? Math.max(0, clock() - leaderSeenAtMs) : null,
      updatedAtMs: clock()
    };
  }
  return {
    start,
    snapshot,
    isLeader: () => role === "leader",
    isClosed: () => closed,
    onRoleChange(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    onDirty(listener) {
      dirtyListeners.add(listener);
      return () => dirtyListeners.delete(listener);
    },
    onExternalChange(listener) {
      externalChangeListeners.add(listener);
      return () => externalChangeListeners.delete(listener);
    },
    notifyDirty(collection, ids = []) {
      post({ type: "dirty", collection, ids });
    },
    notifyDirtyAndWait(collection, ids = [], { timeoutMs = DIRTY_ACK_TIMEOUT_MS } = {}) {
      if (role === "leader") {
        return handleDirty({ type: "dirty", collection, ids, tabId, atMs: clock() });
      }
      if (!channel || !leaderTabId) return Promise.reject(new Error("No multi-tab sync leader is available."));
      const requestId = globalThis.crypto?.randomUUID?.() || `dirty-${tabId}-${clock()}-${Math.random().toString(36).slice(2)}`;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pendingDirtyAcks.delete(requestId);
          reject(new Error(`Multi-tab leader did not acknowledge ${collection} within ${timeoutMs}ms.`));
        }, Math.max(100, Number(timeoutMs) || DIRTY_ACK_TIMEOUT_MS));
        pendingDirtyAcks.set(requestId, { resolve, reject, timer });
        post({ type: "dirty", requestId, collection, ids });
      });
    },
    notifyReplicatedChange(collection, ids = []) {
      post({ type: "replicated-change", collection, ids });
    },
    async close() {
      if (role === "leader") post({ type: "leader-release" });
      closed = true;
      releaseLock?.();
      if (heartbeatTimer) clearInterval(heartbeatTimer);
      if (electionTimer) clearInterval(electionTimer);
      globalThis.document?.removeEventListener?.("freeze", lifecycleRelease);
      globalThis.removeEventListener?.("pagehide", lifecycleRelease);
      globalThis.document?.removeEventListener?.("resume", lifecycleResume);
      globalThis.removeEventListener?.("pageshow", lifecycleResume);
      try {
        channel?.close();
      } catch {
      }
      for (const pending of pendingDirtyAcks.values()) {
        clearTimeout(pending.timer);
        pending.reject(new Error("Multi-tab sync coordinator closed before leader acknowledgement."));
      }
      pendingDirtyAcks.clear();
      listeners.clear();
      dirtyListeners.clear();
      externalChangeListeners.clear();
    }
  };
}
function stableHash(value) {
  let hash = 2166136261;
  for (const character of String(value || "")) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}
function delay3(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
var multiTabSyncCoordinatorTestInternals = Object.freeze({
  HEARTBEAT_MS,
  LEASE_TTL_MS,
  DIRTY_ACK_TIMEOUT_MS,
  stableHash
});

// src/apps/business-os/rxdb/src/sync-profile-registry.mjs
var REGISTRY_KEY = "__ctoxCollectionSyncProfiles";
var VALID_PROFILES = /* @__PURE__ */ new Set(["demand-only", "demand-chunks"]);
function registryMap() {
  const existing = globalThis[REGISTRY_KEY];
  if (existing instanceof Map) return existing;
  const created = /* @__PURE__ */ new Map();
  try {
    globalThis[REGISTRY_KEY] = created;
  } catch {
  }
  return created;
}
function registerCollectionSyncProfile(name, profile) {
  const key = String(name || "").trim();
  if (!key) return;
  if (!VALID_PROFILES.has(profile)) {
    registryMap().delete(key);
    return;
  }
  registryMap().set(key, profile);
}
function getCollectionSyncProfile(name) {
  const key = String(name || "").trim();
  if (!key) return null;
  return registryMap().get(key) || null;
}
function clearCollectionSyncProfiles() {
  registryMap().clear();
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
    request.onblocked = () => reject(new Error(`IndexedDB delete blocked for ${name}`));
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
    getMultiTabSyncCoordinator,
    getPresenceRegistry,
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
    const journal = storage?.recoveryJournal || null;
    this.recovery = {
      getStatus: () => journal?.getStatus?.() || Promise.resolve(emptyRecoveryStatus(name)),
      export: (passphrase) => journal?.export?.(passphrase),
      previewImport: (file, passphrase) => journal?.previewImport?.(file, passphrase),
      applyImport: (previewId) => journal?.applyImport?.(previewId),
      retryPrimaryOpen: async () => {
        if (!globalThis.indexedDB?.open) return false;
        const request = indexedDB.open(name);
        const opened = await new Promise((resolve, reject) => {
          request.onsuccess = () => resolve(request.result);
          request.onerror = () => reject(request.error || new Error(`Failed to reopen IndexedDB ${name}`));
          request.onblocked = () => reject(new Error(`IndexedDB open blocked for ${name}`));
        });
        opened.close();
        return true;
      }
    };
    this.conflicts = {
      list: () => journal?.listConflicts?.() || Promise.resolve([]),
      resolve: (id, resolution) => journal?.resolveConflict?.(id, resolution)
    };
  }
  async addCollections(collections) {
    for (const [name, definition] of Object.entries(collections || {})) {
      if (this.collections[name]) continue;
      const schema = definition?.schema || definition;
      const conflictStrategy = definition?.conflictStrategy;
      const deleteStrategy = definition?.deleteStrategy;
      registerCollectionSyncProfile(name, definition?.syncProfile);
      const collection = new CtoxRxCollection({
        name,
        schema,
        storageCollection: this.storage.collection(name, { schema, conflictStrategy, deleteStrategy })
      });
      this.collections[name] = collection;
      this[name] = collection;
      collection.recoveryInitialization = Promise.resolve(
        collection.storageCollection.initializeRecovery?.()
      ).catch((error) => {
        collection.recoveryInitializationError = error;
        console.error(`[ctox-rxdb] recovery initialization failed for ${name}:`, error);
        return null;
      });
    }
    return this.collections;
  }
  collection(name) {
    return this.collections[name] || this[name] || null;
  }
  async getUnsyncedWriteSummary() {
    return this.storage.unsyncedWriteSummary?.() || { total: 0, byCollection: {} };
  }
  async close() {
    for (const collection of Object.values(this.collections)) {
      collection.storageCollection?.close?.();
    }
    this.storage.close();
  }
};
function emptyRecoveryStatus(databaseName) {
  return {
    schema: "ctox.browser-recovery.status.v2",
    databaseName,
    pendingBatches: 0,
    pendingWrites: 0,
    pendingBytes: 0,
    oldestPendingAtMs: 0,
    unresolvedConflicts: 0,
    lastExportAtMs: 0,
    updatedAtMs: Date.now()
  };
}
var CtoxRxCollection = class {
  constructor({ name, schema, storageCollection }) {
    this.name = name;
    this.schema = {
      jsonSchema: schema,
      version: schema?.version || 0,
      primaryPath: primaryPathFromSchema2(schema),
      hash: () => schemaHash(schema, name)
    };
    this.storageCollection = storageCollection;
    this.demandLoader = null;
    this.liveQueryPerformanceStats = {
      complexLiveQueryReexecs: 0,
      deltaLiveQueryApplies: 0,
      lastComplexLiveQuery: null,
      lastDeltaLiveQuery: null
    };
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
    const normalized = docs.map((doc) => normalizeDoc(doc, this.schema.primaryPath));
    const result = typeof this.storageCollection.bulkUpsert === "function" ? await this.storageCollection.bulkUpsert(normalized) : await this.storageCollection.bulkWrite(normalized);
    const success = result?.success || {};
    return normalized.map((doc) => new CtoxRxDocument(this, success[doc.id] || doc));
  }
  find(query = {}) {
    return new CtoxRxQuery(this, query, false);
  }
  findOne(idOrQuery) {
    return new CtoxRxQuery(this, idOrQuery, true);
  }
  count(query = {}) {
    return {
      exec: async () => {
        const normalized = normalizeQuery(query, this.schema.primaryPath);
        if (typeof this.storageCollection.countDocuments === "function") {
          return this.storageCollection.countDocuments(normalized, {
            matchesSelector,
            sortDocuments
          });
        }
        return (await this.find(query).exec()).length;
      }
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
  setQueryPerformancePolicy(policy = {}) {
    this.storageCollection.setQueryPerformancePolicy?.(policy);
  }
  resetQueryPerformanceStats() {
    this.storageCollection.resetQueryPerformanceStats?.();
    this.liveQueryPerformanceStats = {
      complexLiveQueryReexecs: 0,
      deltaLiveQueryApplies: 0,
      lastComplexLiveQuery: null,
      lastDeltaLiveQuery: null
    };
  }
  getQueryPerformanceStats() {
    return {
      storage: this.storageCollection.getQueryPerformanceStats?.() || null,
      liveQueries: cloneJson2(this.liveQueryPerformanceStats)
    };
  }
  recordComplexLiveQueryReexec(query = {}) {
    this.liveQueryPerformanceStats.complexLiveQueryReexecs += 1;
    this.liveQueryPerformanceStats.lastComplexLiveQuery = {
      at: Date.now(),
      selectorFields: Object.keys(query?.selector || {}).filter((field) => !field.startsWith("$")),
      sortFields: normalizeSort2(query?.sort || []).map((entry) => Object.keys(entry || {})[0]).filter(Boolean),
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0
    };
  }
  recordDeltaLiveQueryApply(query = {}, changedCount = 0) {
    this.liveQueryPerformanceStats.deltaLiveQueryApplies += 1;
    this.liveQueryPerformanceStats.lastDeltaLiveQuery = {
      at: Date.now(),
      changedCount,
      selectorFields: Object.keys(query?.selector || {}).filter((field) => !field.startsWith("$")),
      sortFields: normalizeSort2(query?.sort || []).map((entry) => Object.keys(entry || {})[0]).filter(Boolean),
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0
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
        let initialRetryTimer = null;
        let initialRetryAttempt = 0;
        let initialized = false;
        let pendingSuccess = {};
        const documentsById = /* @__PURE__ */ new Map();
        const debounceMs = OBSERVABLE_DEBOUNCE_MS;
        const emitSnapshot = () => {
          listener({
            collectionName: this.name,
            documents: Array.from(documentsById.values())
          });
        };
        const applySuccess = (success = {}) => {
          for (const rawDoc of Object.values(success || {})) {
            const id = documentIdFromDoc(rawDoc);
            if (!id) continue;
            if (rawDoc?._deleted) {
              documentsById.delete(id);
            } else {
              documentsById.set(id, new CtoxRxDocument(this, rawDoc));
            }
          }
        };
        const flushInitial = async () => {
          if (!active) return;
          let documents;
          try {
            documents = await this.find().exec();
          } catch (error) {
            if (isIndexedDbConnectionClosingError(error)) return;
            if (active && isRetryableObservableInitError(error)) {
              const delayMs = observableInitRetryDelayMs(initialRetryAttempt);
              initialRetryAttempt += 1;
              initialRetryTimer = setTimeout(() => {
                initialRetryTimer = null;
                void flushInitial();
              }, delayMs);
              return;
            }
            throw error;
          }
          if (!active) return;
          initialRetryAttempt = 0;
          documentsById.clear();
          for (const doc of documents) {
            const id = documentIdFromDoc(doc);
            if (id) documentsById.set(id, doc);
          }
          applySuccess(pendingSuccess);
          pendingSuccess = {};
          initialized = true;
          emitSnapshot();
        };
        const flushDelta = () => {
          pendingTimer = null;
          if (!active || !initialized) return;
          applySuccess(pendingSuccess);
          pendingSuccess = {};
          emitSnapshot();
        };
        const emit = (event) => {
          pendingSuccess = {
            ...pendingSuccess,
            ...successPayloadFromChangeEvent(event)
          };
          if (!initialized) return;
          if (pendingTimer != null) return;
          pendingTimer = setTimeout(flushDelta, debounceMs);
        };
        void flushInitial();
        const unsubscribe = this.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
            }
            if (initialRetryTimer != null) {
              clearTimeout(initialRetryTimer);
              initialRetryTimer = null;
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
var OBSERVABLE_INIT_RETRY_BASE_MS = 100;
var OBSERVABLE_INIT_RETRY_MAX_MS = 2e3;
function isRetryableObservableInitError(error) {
  if (error?.retryable === true) return true;
  const code = String(error?.code || "").trim().toUpperCase();
  if (code === "QUERY_QUEUE_LIMIT") return true;
  return String(error?.message || error || "").includes("QUERY_QUEUE_LIMIT:");
}
function observableInitRetryDelayMs(attempt = 0) {
  const exponent = Math.max(0, Math.min(8, Number(attempt) || 0));
  return Math.min(
    OBSERVABLE_INIT_RETRY_MAX_MS,
    OBSERVABLE_INIT_RETRY_BASE_MS * 2 ** exponent
  );
}
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
        let initialized = false;
        let pendingPrimaryDoc = void 0;
        const primaryId = this.single ? singlePrimaryKeyCandidateId(this.query, this.collection.schema.primaryPath) : "";
        const canApplyPrimaryDelta = Boolean(primaryId);
        const canApplyQueryDelta = !this.single && canApplyUnboundedQueryDelta(this.query);
        let pendingSuccess = {};
        const queryDocumentsById = /* @__PURE__ */ new Map();
        const emitQueryDocuments = () => {
          listener(sortDocuments(Array.from(queryDocumentsById.values()), this.query.sort));
        };
        const applyQuerySuccess = (success = {}) => {
          for (const rawDoc of Object.values(success || {})) {
            const id = documentIdFromDoc(rawDoc);
            if (!id) continue;
            if (rawDoc?._deleted || !matchesSelector(rawDoc, this.query.selector)) {
              queryDocumentsById.delete(id);
            } else {
              queryDocumentsById.set(id, new CtoxRxDocument(this.collection, rawDoc));
            }
          }
        };
        const flushEmit = () => {
          pendingTimer = null;
          if (!active) return;
          if (initialized && !canApplyPrimaryDelta && !canApplyQueryDelta) {
            this.collection.recordComplexLiveQueryReexec(this.query);
          }
          this.exec().then((value) => {
            if (!active) return;
            initialized = true;
            if (pendingPrimaryDoc !== void 0 && canApplyPrimaryDelta) {
              listener(wrapPrimaryDeltaDocument(this.collection, pendingPrimaryDoc));
              pendingPrimaryDoc = void 0;
              return;
            }
            if (canApplyQueryDelta && Array.isArray(value)) {
              queryDocumentsById.clear();
              for (const doc of value) {
                const id = documentIdFromDoc(doc);
                if (id) queryDocumentsById.set(id, doc);
              }
              if (Object.keys(pendingSuccess).length > 0) {
                applyQuerySuccess(pendingSuccess);
                pendingSuccess = {};
                emitQueryDocuments();
                return;
              }
            }
            listener(value);
          }).catch(() => {
          });
        };
        const flushPrimaryDelta = () => {
          pendingTimer = null;
          if (!active || !initialized || !canApplyPrimaryDelta || pendingPrimaryDoc === void 0) return;
          const next = pendingPrimaryDoc;
          pendingPrimaryDoc = void 0;
          listener(wrapPrimaryDeltaDocument(this.collection, next));
        };
        const flushQueryDelta = () => {
          pendingTimer = null;
          if (!active || !initialized || !canApplyQueryDelta) return;
          const success = pendingSuccess;
          pendingSuccess = {};
          applyQuerySuccess(success);
          this.collection.recordDeltaLiveQueryApply(this.query, Object.keys(success).length);
          emitQueryDocuments();
        };
        const emit = (event) => {
          if (canApplyPrimaryDelta) {
            const success = successPayloadFromChangeEvent(event);
            if (!Object.prototype.hasOwnProperty.call(success, primaryId)) return;
            pendingPrimaryDoc = success[primaryId] || null;
            if (!initialized) return;
            if (pendingTimer != null) return;
            pendingTimer = setTimeout(flushPrimaryDelta, 50);
            return;
          }
          if (canApplyQueryDelta) {
            pendingSuccess = {
              ...pendingSuccess,
              ...successPayloadFromChangeEvent(event)
            };
            if (!initialized) return;
            if (pendingTimer != null) return;
            pendingTimer = setTimeout(flushQueryDelta, 50);
            return;
          }
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
      skip: patch.skip ?? this.query.skip,
      requireRevision: patch.requireRevision ?? this.query.requireRevision
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
    skip: Number.isFinite(Number(query?.skip)) ? Math.max(0, Number(query.skip)) : void 0,
    requireRevision: typeof query?.requireRevision === "string" && query.requireRevision ? query.requireRevision : void 0
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
    const actual = valueAtPath4(doc, key);
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
      const a = valueAtPath4(left, key);
      const b = valueAtPath4(right, key);
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
function successPayloadFromChangeEvent(event) {
  return event?.success && typeof event.success === "object" ? event.success : event?.detail?.success && typeof event.detail.success === "object" ? event.detail.success : {};
}
function documentIdFromDoc(doc) {
  return String(doc?.id || doc?._id || doc?.document_id || doc?.documentId || "").trim();
}
function cloneJson2(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}
function singlePrimaryKeyCandidateId(query = {}, primaryPath = "id") {
  const selector = query?.selector || {};
  for (const field of ["id", "_id", primaryPath].filter(Boolean)) {
    if (!Object.prototype.hasOwnProperty.call(selector, field)) continue;
    const value = selector[field];
    if (typeof value === "string" || typeof value === "number") return String(value);
    if (value && typeof value === "object" && !Array.isArray(value) && "$eq" in value && value.$eq != null) {
      return String(value.$eq);
    }
    return "";
  }
  return "";
}
function canApplyUnboundedQueryDelta(query = {}) {
  return !Number.isFinite(Number(query?.limit)) && !(Number.isFinite(Number(query?.skip)) && Number(query.skip) > 0);
}
function wrapPrimaryDeltaDocument(collection, doc) {
  if (!doc || doc._deleted) return null;
  return new CtoxRxDocument(collection, doc);
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
  const compiled = compileLinearRegexPattern(pattern);
  if (!compiled) return false;
  return testLinearRegexPattern(String(actual), compiled);
}
var MAX_LINEAR_REGEX_PATTERN_LENGTH = 128;
var MAX_LINEAR_REGEX_INPUT_LENGTH = 8192;
function compileLinearRegexPattern(pattern) {
  const source = pattern instanceof RegExp ? pattern.source : String(pattern ?? "");
  const ignoreCase = pattern instanceof RegExp ? pattern.ignoreCase : false;
  if (!source || source.length > MAX_LINEAR_REGEX_PATTERN_LENGTH) return null;
  let cursor = 0;
  let end = source.length;
  const anchoredStart = source[cursor] === "^";
  if (anchoredStart) cursor += 1;
  const anchoredEnd = end > cursor && source[end - 1] === "$" && !isEscaped(source, end - 1);
  if (anchoredEnd) end -= 1;
  const tokens = [];
  while (cursor < end) {
    const parsed = parseLinearRegexAtom(source, cursor, end);
    if (!parsed) return null;
    cursor = parsed.next;
    let min = 1;
    let max = 1;
    if (cursor < end && ["*", "+", "?"].includes(source[cursor])) {
      const quantifier = source[cursor];
      min = quantifier === "+" ? 1 : 0;
      max = quantifier === "?" ? 1 : Infinity;
      cursor += 1;
    }
    tokens.push({ ...parsed.atom, min, max });
  }
  return { tokens, anchoredStart, anchoredEnd, ignoreCase };
}
function parseLinearRegexAtom(source, cursor, end) {
  const char = source[cursor];
  if (!char) return null;
  if (char === ".") {
    return { atom: { kind: "any" }, next: cursor + 1 };
  }
  if (char === "\\") {
    const escaped = source[cursor + 1];
    if (!escaped || cursor + 1 >= end) return null;
    if (escaped === "s") return { atom: { kind: "space" }, next: cursor + 2 };
    if (escaped === "d") return { atom: { kind: "digit" }, next: cursor + 2 };
    if (escaped === "w") return { atom: { kind: "word" }, next: cursor + 2 };
    return { atom: { kind: "literal", value: escaped }, next: cursor + 2 };
  }
  if ("()[]{}|".includes(char)) return null;
  if ("*+?".includes(char)) return null;
  return { atom: { kind: "literal", value: char }, next: cursor + 1 };
}
function testLinearRegexPattern(value, compiled) {
  const input = String(value || "").slice(0, MAX_LINEAR_REGEX_INPUT_LENGTH);
  const text = compiled.ignoreCase ? input.toLocaleLowerCase() : input;
  const tokens = compiled.ignoreCase ? compiled.tokens.map((token) => token.kind === "literal" ? { ...token, value: token.value.toLocaleLowerCase() } : token) : compiled.tokens;
  if (!tokens.length) return true;
  const starts = compiled.anchoredStart ? [0] : Array.from({ length: text.length + 1 }, (_, index) => index);
  return starts.some((start) => {
    const endings = consumeLinearRegexTokens(text, tokens, start, 0);
    return endings.some((end) => compiled.anchoredEnd ? end === text.length : true);
  });
}
function consumeLinearRegexTokens(text, tokens, position, tokenIndex) {
  if (tokenIndex >= tokens.length) return [position];
  const token = tokens[tokenIndex];
  const endings = [];
  let next = position;
  let count = 0;
  while (count < token.min) {
    if (!linearRegexAtomMatches(text[next], token)) return endings;
    next += 1;
    count += 1;
  }
  const positions = [next];
  while (count < token.max && next < text.length && linearRegexAtomMatches(text[next], token)) {
    next += 1;
    count += 1;
    positions.push(next);
  }
  for (let index = positions.length - 1; index >= 0; index -= 1) {
    endings.push(...consumeLinearRegexTokens(text, tokens, positions[index], tokenIndex + 1));
  }
  return endings;
}
function linearRegexAtomMatches(char, token) {
  if (char === void 0) return false;
  if (token.kind === "any") return true;
  if (token.kind === "space") return /\s/.test(char);
  if (token.kind === "digit") return char >= "0" && char <= "9";
  if (token.kind === "word") return /[A-Za-z0-9_]/.test(char);
  return char === token.value;
}
function isEscaped(source, index) {
  let slashCount = 0;
  for (let cursor = index - 1; cursor >= 0 && source[cursor] === "\\"; cursor -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}
function arrayContains(actual, expected) {
  return Array.isArray(actual) && actual.includes(expected);
}
function elemMatch(actual, selector) {
  return Array.isArray(actual) && actual.some((item) => item && typeof item === "object" ? matchesSelector(item, selector) : item === selector);
}
function valueAtPath4(doc, path) {
  const parts = pathSegments(path);
  if (parts.some(isUnsafePathSegment)) return void 0;
  return parts.reduce((value, key) => value?.[key], doc);
}
function setValueAtPath(doc, path, value) {
  const parts = assertSafePathSegments(path, "document path");
  if (!parts.length) return;
  let target = doc;
  for (const part of parts.slice(0, -1)) {
    let next = ownValue(target, part);
    if (!next || typeof next !== "object") {
      next = {};
      defineOwnValue(target, part, next);
    }
    target = next;
  }
  defineOwnValue(target, parts[parts.length - 1], value);
}
function pathSegments(path) {
  return String(path || "").split(".").filter(Boolean);
}
function isUnsafePathSegment(segment) {
  return segment === "__proto__" || segment === "prototype" || segment === "constructor";
}
function assertSafePathSegments(path, label) {
  const parts = pathSegments(path);
  if (parts.some(isUnsafePathSegment)) {
    throw new Error(`${label} contains unsafe prototype segment`);
  }
  return parts;
}
function ownValue(object, key) {
  if (!object || typeof object !== "object" || !Object.hasOwn(object, key)) return void 0;
  return Object.getOwnPropertyDescriptor(object, key)?.value;
}
function defineOwnValue(object, key, value) {
  Object.defineProperty(object, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true
  });
}
function primaryPathFromSchema2(schema) {
  const primary = schema?.primaryKey;
  if (typeof primary === "string") return primary;
  if (primary?.key) return primary.key;
  return "id";
}
function normalizeDoc(doc, primaryPath) {
  if (!doc || typeof doc !== "object") {
    throw new TypeError("document must be an object");
  }
  assertSafePathSegments(primaryPath, "primary key path");
  const normalized = { ...doc };
  const id = normalized.id || normalized._id || valueAtPath4(normalized, primaryPath);
  if (!id) {
    throw new Error(`document is missing primary key ${primaryPath}`);
  }
  normalized.id = String(id);
  if (valueAtPath4(normalized, primaryPath) === void 0) {
    setValueAtPath(normalized, primaryPath, normalized.id);
  }
  normalized._deleted = Boolean(normalized._deleted);
  normalized._meta = {
    ...normalized._meta || {},
    lwt: documentLwt2(normalized)
  };
  return normalized;
}
function documentLwt2(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0)
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
}
var ctoxRxdbTestInternals = {
  matchesSelector,
  normalizeDoc,
  normalizeQuery,
  normalizeSort: normalizeSort2,
  sortDocuments
};

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
      publicName: "CTOX Sync Engine",
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
      localPush: {
        changedSinceCalls: snapshot.localPushChangedSinceCalls,
        scannedRows: snapshot.localPushChangedSinceScannedRows,
        scanLimitHits: snapshot.localPushChangedSinceScanLimitHits,
        maxScannedRows: snapshot.localPushChangedSinceMaxScannedRows
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
  CtoxRecoveryJournal,
  CtoxSubject,
  CtoxWebRtcNativePeer,
  DEFAULT_QUERY_META_BUDGET_BYTES,
  DEFAULT_WINDOW_LIMIT,
  FILE_CHUNK_PRESENCE_KEY,
  OBSERVABLE_DEBOUNCE_MS,
  PRESENCE_NOTIFY_DEBOUNCE_MS,
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
  clearCollectionSyncProfiles,
  compareHybridLogicalClocks,
  correctedHybridLogicalClockNowMs,
  createActiveCollectionRegistry,
  createBroadcastChannelBroker,
  createCtoxWebRtcNativePeer,
  createDemandLoadingTransport,
  createFileDemandLoader,
  createIndexedDbMetaBackend,
  createMemoryBroker,
  createMemoryMetaBackend,
  createMultiTabSyncCoordinator,
  createPresenceRegistry,
  createQueryDemandLoader,
  createRxDatabase,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
  ctoxIndexedDbStorageTestInternals,
  ctoxRxdbTestInternals,
  decodeChunk,
  decryptRecoveryArtifact,
  deepEqualJson,
  encryptRecoveryArtifact,
  formatHybridLogicalClock,
  getActiveCollectionRegistry,
  getCollectionSyncProfile,
  getConnectionHandlerSimplePeer,
  getCtoxIndexedDbStorage,
  getMultiTabSyncCoordinator,
  getPresenceRegistry,
  hybridLogicalClockNodeId,
  hybridLogicalClockStatus,
  isFutureHybridLogicalClock,
  multiTabSyncCoordinatorTestInternals,
  nextHybridLogicalClock,
  normalizeConflictStrategy,
  normalizeSignalingControlPlaneError,
  openCtoxIndexedDbStorage,
  openRecoveryJournal,
  parseHybridLogicalClock,
  projectStatusFromSidecar,
  queryFingerprint,
  recoverQueryMetaQuota,
  recoveryCryptoTestInternals,
  recoveryJournalTestInternals,
  registerCollectionSyncProfile,
  remoteSupportsQueryFetch,
  removeRxDatabase,
  replicateWebRTC,
  replicationWebRtcTestInternals,
  rxdbCore,
  schemaHash,
  schemaHashSource,
  setHybridLogicalClockTimeAnchor,
  setV15LogSink,
  sha256Hex,
  sha256Json,
  snapshotV1_5Status,
  threeWayMergeDocuments,
  waitForEvent
};
