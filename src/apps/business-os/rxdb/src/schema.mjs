// =============================================================================
// AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
// =============================================================================
// This file is part of CTOX Sync Engine, the WebRTC-ONLY data plane between Business OS
// and the CTOX daemon. Hard rules (each one has caused real regressions):
//   1. NO HTTP fallback/bridge for collection data — ever. WebRTC only.
//   2. NO npm/bare/node: imports — this runtime is package-manager-free.
//   3. After ANY src edit: rebuild dist with the pinned esbuild command and
//      bump the ?v= cache-buster (see docs/ctox-rxdb.md "Build & release").
//      Never patch dist/ctox-rxdb-js.mjs directly.
//   4. Wire-contract constants are GENERATED from fixtures — never hand-edit
//      *-contract.generated.mjs or the Rust twins.
//   5. Run `node src/apps/business-os/rxdb/tests/run-all.mjs` and keep it
//      green. Never delete or weaken a failing test to make it pass.
// =============================================================================

// CTOX_BUSINESS_OS_SCHEMA_HASHES below MUST stay identical to the Rust
// fixture src/core/business_os/business_os_schema_hashes.json — enforced by
// tests/schema-hash-registry-smoke.mjs.
export {
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_PROTOCOL_PHASE,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  CTOX_SCHEMA_HASH_SOURCES,
} from './protocol-contract.generated.mjs';
import {
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_PROTOCOL_PHASE,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  CTOX_SCHEMA_HASH_SOURCES,
} from './protocol-contract.generated.mjs';

export const CTOX_SCHEMA_HASH_CAPABILITY = 'ctox-schema-hash-v1';
export const CTOX_PEER_SESSION_CAPABILITY = 'ctox-peer-session-v1';
export const CTOX_CHECKPOINT_EPOCH_CAPABILITY = 'ctox-checkpoint-epoch-v1';
export const CTOX_BUSINESS_OS_SCHEMA_HASHES = Object.freeze({
  accounting_accounts: '49289609d65cd6dd1ce2ed700bcdf90e3ea4d1c544a49d6c53bc198f0b36090b',
  accounting_bank_statement_lines: '293037b34bd0611ad1517b57ed3cb6e16df4c64f6f2d015a29244cc44b8d1c79',
  accounting_bank_statements: '9afe72d42212dae5f94b0280530c466fee83121dbfc125c831e0cf4e5082e29c',
  accounting_credit_notes: 'aaa9857647733e4bc62a0a6582cc2726c540f7c6ca646cf89d8e04562aef1c73',
  accounting_dunning_letters: 'e65a8857631082094370117507ff0639d4c12a9819f19a510c1f9d152590c31d',
  accounting_dunning_runs: 'f8b7492ce9da3b0edda0bcf6543c984fc62cf75bf54bfd6c573652f2b58f6ec8',
  accounting_invoice_approvals: 'a1c36be266686fa71031af4208883f99e11764ae3326b5426b9d47f76d9d953d',
  accounting_invoice_attachments: 'ef0d908b819f26e471f55e084d0033968f7b418b15053c816613cdcc3a3fd9c6',
  accounting_invoice_lines: '723f7c87a10efb70775ac6cf897b34d7ed62c508e719cb38ac6891692d1d50cb',
  accounting_invoices: 'c17ee2b8f1f3095d4b083373fff82b6219c19eb60503c3be44764baf215ad704',
  accounting_journal_entries: 'd19825f6d3426fd1d2c15f3a2aeae033d2e96aad1b2bf06e49d896b4c3d26b73',
  accounting_journal_entry_lines: '6ba7ca11b7f81f044ddd5886758a3df79323e0b1778bf2d3048528fead22d91d',
  accounting_ledger_entries: '58d17a1089b6591e0a78e3f9f1a093815fa4742f73347bb8c404a099dc9c4461',
  accounting_number_series: '5fe7fda57fb6df74e86432afebd2672ec233140d2de7181a62889d82f8e08bce',
  accounting_payment_allocations: 'fadad3a78419a2e7e43100f45539ce53343f13eda6a79ccff515b782a7b24ebc',
  accounting_payment_terms: 'f3dd933e24d4309281b12cf3ee6bceb79110b02074583846d965a9869d8505a5',
  accounting_payments: 'ad499684f4082c37411936777b743f94873f11fdb42e256ce78f065d3a87a518',
  accounting_receipts: 'f0995e7532ead4def819aa6bc554d53c4abbaccc8bee0db89272c0a11a5e43a0',
  accounting_recurring_invoices: '924982a4dc4773c2fdc01bf5a05a29ee26fd3e5708f69a6ad061bef0dadcc67a',
  applications: 'ef7b52e117777575029346888b1c17412e2f4bda0ef9ca6c5a5e89c54a310e00',
  appsec_approvals: 'ab75d7da2d137f7a328e99735d3fc0350be18d7cbbffccc05cd84dccca5feaf3',
  appsec_artifacts: '96e48a95be0e013773561dd52b5be8170d13aa0409488d5569e7060654eb22e1',
  appsec_assessments: '31327725da3d9bd2769bc6f27b1299a792cb388aaf413dd95c3e8247d8a50fa2',
  appsec_coverage: '6fb9caf08dcf52ab1e7a669950bdb62159aec0b37112fb89aed0f93de475dde1',
  appsec_findings: '1c16a6500dc1dfbb3401ca83208338f2d8371c5abe88e8f5dfd21d4b3a73fbec',
  appsec_pipeline_stages: '62a18e76f2a1eaf38243d7b65f1614eeb7f38c67e5e32a8211e9bc099a7b613b',
  appsec_runs: '815d0931042bedab1356e27a22b3172b8e62e6c3eff983e7e2f74be89f30b326',
  appsec_scanner_inventory: '529588bf3120a75dbf86d7f8663eb5e088cc19e13477042ae5338fa0bb079bb3',
  browser_frames: '3718321ed3853a1dff93aaedd5650e2487bb65795f761611a017e66f998635ed',
  browser_input_events: '5733148e341550c1166db59b9b93c7975dc7aa4753451da9dd876b4cbe3d0875',
  browser_sessions: '8f9d925480b6fa11755bb0800e47da9d4b8dca59f510fb5c6bfb3d84cec212d3',
  browser_tabs: '3387a8373cad98f4651b15173cf920568970ad2afa7f14758bbfffe9d77d5004',
  business_chats: '0e52de33b4ea565122debb0e46296b44cdbe13f60190b9d9d06259f3719918d7',
  business_commands: '83f3dc7b9078ae89640b9ffcc33bf29e1c74ec40df7cf63af9889b5bc0e4d238',
  business_consents: '4e0031090f60e466e8d9b2818a73faac41d89adabba5c2f2fd75a4b48cef9d68',
  business_credentials: '5583908188482df5c694d6214ef4f3a250fdcd09d7111a5a859a5976f4a40b7d',
  business_module_acl: '7f2c6c44ffadefb0c9be30dba9f3067fc48e0847424e3f2709638c5ebcd8bedf',
  business_module_catalog: '332763869d93c2bb55fa6b217c36521d1c1f17be4701d8538d686cda89f5cea0',
  business_module_releases: '8d9ff79eec5eccc04353a885002a8982deb169dbbf3a348998b88fafb7e219f7',
  business_module_reports: '440b04e33e1040e556c62741d7c4289422b6d0d01203c74e5aee391d5f050ed1',
  business_module_source_files: 'fa9cdeda3530f04bd84b926cb8ffae650c8f5886efac079daee0d01315737551',
  business_users: 'da6d1a192bc21ad59baf2680d8b80faa471a4883457a8d0ad5a533a1afefba42',
  business_workspace_branding: 'a53d4f3e84454928bfeb239c22820b305cec6c657bb6d7340f68594f20baae22',
  calendar_availability_rules: 'be220a21b86c15d22627f8685e0a90849a485baaa2b07b7133364107ffde661a',
  calendar_booking_holds: '1add78cb8f30596fd320eafcd798f54e06cd09451097398d87f56b6dcea6bde4',
  calendar_booking_pages: 'cab127f6ea856a86f4b9f9e6b241f14490107ebea9673a99e1049cac2fcb9290',
  calendar_bookings: '65d02d6737bb81b4065137ac98d9514b1bc2fff83737f8daf969fab61d94e1f5',
  calendar_calendars: '5aecddfa2b457b45137356e178da572fb38488435ae050d8ecc11413c007bb82',
  calendar_event_instances: '664cae43311b56a13ad6f0fe1aa405bc6829dd179f33502a05ef15ea9d62e86f',
  calendar_events: '6baa83e7381279ac9f9963c907605d301e4686126204ebda54e80b0420bc0d45',
  calendar_sources: 'db772cfda4ba2e0bc52b86899d155ea06e15a5811f370714206e5f3aff555d38',
  channel_pairing_state: 'd93ceef99b772bc57939143bc6ef0044bf816801700d2dbc8f88def356aa246a',
  coding_agent_events: '784ad1849525cceb945125e8383a2994c33e802ee3bef4591e2ea331c63e3781',
  coding_agent_sessions: '55eebef3dd6ec5bc0650ae37280552ad81b4185d4f0acca5766fc3d0da4da090',
  coding_agent_workspace_grants: 'e377e22121858098b74d6c29de67c19345c6c9e27eae46900434bb292cdb09e0',
  communication_accounts: 'd40ca549e2f112071b6eb39bf0999a743643073279af4471a477cef259275653',
  communication_messages: '10d120234ec23bbe98124d255599f44d2ef68ecb5ff29787b9b647aaf6537b6f',
  communication_threads: '2111d907ee8cc8c7c2c4e9f10a43bc56f217071dbee0610a96b0457ef6473a8d',
  ctox_bug_reports: 'f7329368ad5144b8ea740600265f06c6ac19ad049de751cec92818d9d9de94b5',
  ctox_queue_tasks: 'ef301e5c6b94a75aff9cc3e7f3d901ce0096b6cd003880450d54450d31f4ed0e',
  ctox_runs: '73df37bddc2e511b0567496f6199089aef436dd598a3e0bf85f462d38b4f3fff',
  ctox_runtime_settings: '3958bb6580e9705f3688fcf453a80ec33c486b43ac6988f015ffc16cb5ac918d',
  ctox_task_approval_requests: '5bbda4583cadd08e30c5948d2ed197cbf4a1f8f342580c1e531fd2a054da84fe',
  ctox_ticket_approvals: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_cases: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_clarification_requests: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_control_bundles: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_event_routing_state: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_events: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_items: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_label_assignments: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_self_work_items: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_self_work_notes: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_verifications: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  ctox_ticket_writebacks: 'b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9',
  customer_accounts: '9a98ca2106e699119cd958f5fe35baf31a1eaa90aaf2dced51a903e353aa5e47',
  customer_activities: '011f283b5b83c14faecff2f639af4db0d4f8ea97f8373bf513dbb50db251e5d8',
  customer_contacts: '5f7abb4b088c1ce30f12ae1438d75012de41bcd9d46c9b07d8f34478f506a093',
  customer_dedupe_candidates: '5b9503d8708014f6d7210ec37450611b77ee8127557d6dd5d901f1ad4e9c5097',
  customer_files: '2436166ea70232f2beeebc3d2a6841a61a06985805db2968db39d4506ab9277e',
  customer_import_batches: '59c02b9e9d7ea0449d407a9454550476457e2a2ec5af89090b7599eeee493f3b',
  customer_notes: 'b0ac4af2ad41f552f589e8cd9c55511fc35efd55c8b0f87012aa52089d1ac0f9',
  customer_opportunities: '222c4446b849ad99db0210a36ebb1911b84a789952ff56489884675d3541cec5',
  customer_tasks: 'b9de4bca1c54d10100a19c5453fc1803990d15f803ecb52e6075b61daf1109c4',
  customer_view_filters: 'abd2880ebc48b29b72ef205a4b09085ee7acf1bdde3ffebcea08059ed7e99123',
  customer_view_sorts: 'be8feb8ba887879e7c223d4883b9b7771a0b137e9523c732d4de3432b3f4dc51',
  customer_views: 'c20ecce31428596148a2a4348061465eb8055aee14ef84ed9755d1a84503936e',
  desktop_file_chunks: 'f3d9e6f8278f3140993109de6e34617376a4cd2b850e59dc066c0af066d6cc63',
  desktop_files: '5c8ea6eddecd37233ef1b99ad10280afe9ae5654bc77819d85d56236257be627',
  desktop_icons: 'b3fc7cde6c2df59469255353b9ce91e5213ad091b86e8b3f2372e63db8c5ecd9',
  desktop_layout: 'd741aa98029c7e0c38fb2ef53e32319ee4c7891b808c875802c540d60bdf5c3c',
  desktop_notifications: '5c312d2c291bf2b36fdbda8aacc1b2de7873c6ee7058c9960897bbb5b0797d0d',
  desktop_windows: 'bcd10d8462083460b5025160f88f0abe6c7118d583aa4d1fd97433942617627f',
  document_blob_chunks: '9b4e27b2f795c697b67747b55e388b8d42afb3d5b8f66e6f9ec36f9564028b16',
  document_runbooks: '50b126b168c2fbf148da6b8693bbf455f6124c1b798a19e48aaaf5174acc9b7b',
  document_versions: 'fca6df9bfa1d0d27f93d41cb7685fd08dacbf9f4843b7c1d95142b4cbe157738',
  documents: '600e0a73160dfaa480dd0ff8b833c85cec8aa60d41a9982a1ecd971e8a291ec1',
  interview_meetings: 'e3e829b1b8b8ab4e5c2f9e3e7af82061125ffadc33cf484858dace36b5e2c62a',
  interview_scorecards: '126212aa5d37811134a96ca150fd07304eca1242d8de13d534458b9a48bf827a',
  iot_agent_status: 'c719592fcc4274060d12567b09013cff8dc11b605b790b349e8efac88cfb6ccd',
  iot_agents: '0bf0fed6ea33be5d475e88b7b913fb1675bb1bf5d4361cc3c5eb6befec6480f8',
  iot_alarms: '978c527550ceb781393bba6e9e886714f7c66f60bc2f7b98be55896bb2ccb149',
  iot_asset_types: '5aebcc5fb39fe783d5364ce21c6f50dc929935ad1cef4964ad1ae996221064d3',
  iot_assets: 'b56ee809bbf974a07d1a6423753bedc195e49f7ea4a9f0f4077afa54486ff93e',
  iot_attributes: '35a1c2494238fffedd2b6006ff5269bc7183a5ac60e2cd4a4c12ed17a9acabcb',
  iot_dashboards: '29a0875c3214b50bce0198608dd4a44e969b51900ea0b76128a53a4fffd25d49',
  iot_datapoints: '6313f3c8671e3406d789877aca842f8bf5b6a7fa2b63a8458dece314a2f55a80',
  iot_realms: '42ff4cfc74268c51602dd3873df95127f9070068aa5d7c1994e80f5275f78ada',
  iot_rulesets: '0232a7ef9501f87ff583848bf29489aff7105d79ea7a1740dbfc357476f799f1',
  iot_widgets: 'acd9a9b1bdaabe7118403bd998190ac785cbf3133c2352386f14f1a4579eb66e',
  knowledge_items: '33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300',
  knowledge_runbooks: '33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300',
  knowledge_tables: '33db05bd0efe97e32343da493cd3cb552099383a4bfde182012e334034467300',
  matching_objects: '31ff9b1fce039239cf0684e1cf246b9e5d3a222abd8ca4b0c9f3c837dfeb55e1',
  matching_requirements: '7a57a57784d58c9898d135a519a8789380742cb5a0de055f19e8f6a279035b50',
  matching_results: 'a5260077a1b4e9d5881ff3b265daf8651b8c6be3158cb5eff0d4f78bed21137c',
  notes: '9c02d9c9f4362f7cb9739b5b401eb59528254534fdfd807050a941041304854d',
  offers: 'ee230a74c678a29a209f08b48a98e2bb7d2ddca64c606f89f4cb2af3de7382d3',
  outbound_account_limits: '35d7a40e3e485447e234f72ec898ce57b7f2b7ebc4f01bb748a7e9ea5a3fc68e',
  outbound_approvals: 'f7be2c8526ffc3df85e92a56c8e808adebbcd8944be95bd05658bc6f9d7b143a',
  outbound_campaigns: '194e3748c589a9cfc50ed63dccab525028e9bdbd006f20b73c10e29aa865e58d',
  outbound_companies: '1d79eb4b67d84826ed2016b0385224600d51c334d5b91d4adb77e62e916d0bbf',
  outbound_engagements: 'f310db7ac3c7abdc78b40b227866ce673f5871601d594b00853000f7c4e088c2',
  outbound_letter_templates: '9839d58ede05148b48b2a7e494fc29d4aa94611034a11bc4c73b32de866a7466',
  outbound_meeting_requests: 'f04c3249c3a3d8cf7ca6c2a4b51fbb15729035bca707668fbef3988242e69aa2',
  outbound_messages: '93b8e2cea0670112b6499a86a774dafef3cbd289d11725bf57d4e0941ad13006',
  outbound_pipeline_items: 'd128a88597977a96b0b2572c0eaeb7c2e5da7d21ae691ff0b0a18e4824fd378c',
  outbound_research_adapters: '97ca18afb680d7103173ab5ac08178644998b9c8681ed1a9cc3738736b4c59e1',
  outbound_research_runs: '46573b72d1bd75daf105265b179af2b0b5d9fae5a61e15cf1198e0dc2604a372',
  outbound_sender_assignments: 'd57aeee6946976bd082044147591d648583a6493c6c1c320359b0949c3405c78',
  outbound_sequences: '9368f8c42dc026c94549485d230d01ea511358313b64de0100b5f7706bae251b',
  outbound_skillbooks: 'a896fd1593614940aa223831a949fbda53e8714c9b5086a4f1949db1ace83c35',
  outbound_sources: '241a2673630fb51c06a4e3155465855f299cb56ceeb8ce09ab1ba0d4c460c29a',
  outbound_suppression_entries: '2a894fbfc598d41b81ad7c76466e531d6771c7a9f6e5aa34389dba0e5f2cb329',
  placements: '638132ba63acb16782721b0d0b0469cf44de077f50e367d1287f5ef27e8a3df0',
  planning_absences: '20263440e5b0fa1d7a3a8c0d95f0753f6f5a30da517dcc208fafe5467ef1870b',
  planning_employees: '36852db8c0acb2b48b653592aeefa1af483843e22a2f400cf411178d7e8377c7',
  planning_projects: 'fc558898d1dfe2d9f8cfb925b5fbd304133fcfad7b2e63069770d5f8325e9b6f',
  planning_shifts: '3e5a629a3dd83035c59f23ece1074478bc37afbdea14a7c02dc262cb47813804',
  planning_time_records: '2674badebb2a9b2133f5053b651ec7723b197869c6e32db59153cf0c227c4829',
  research_notes: 'd078cd9b657f5eeb66281eb33e8b912c772fac447a5e60b580901fd4ef82c6dd',
  research_runs: 'ba19ca3daec5cd92154b75faa056bbfab95383769dd69b77ce663656d18c282c',
  research_tasks: '502aa089a7498cf17db0bad1bba2d4bda864261b99488a07e783f6c107dc0dd0',
  signature_requests: '878b66f65173aa1e28f69866ef6f1562a1e564028604c8af68d67616618156bd',
  spreadsheet_blob_chunks: 'dc97cfb4feca43442477d88da04528ecda56ab7cb52b38a19306270eddf26168',
  spreadsheet_runbooks: '08bf33d949370df78a4598cc97208212df6944c4feefe291787dad75e8b0d985',
  spreadsheet_versions: '5c569a9152b65e943b047a0419afea200a7c43e83e6c07eb0a0c667282e45842',
  spreadsheets: '1dfe54101a8efe6ad4d127bc9ac102c74d6b211cda716b1fa5411fc473c24367',
  submissions: '30b927ed9ba7168ef4c911db5450e862771dd00cf510eb75a867497010ce2c78',
  support_agent_requests: 'a031d246fc60f1beaf6df2c94336f18ef5e341e176f4a57e06c3d0c410df6407',
  support_agent_suggestions: 'bca59edd6f3d0ffb6e5518d7644f0f84919df4b7fab2d2155663658a9d1ed357',
  support_applied_slas: 'd50e338b13e2fcb0af8a4547f6d1c30891ec6608954ed2bac60107eba7b8cefd',
  support_assignment_events: 'fae28885fb0333ddc9439c894d38cdd9c58dfd67fda6eba60f8a76399ff13dc4',
  support_assignment_policies: 'fba6729228ad5004a2d315cf66680d5bb189578903cd9ffdc69ae24b49cc4361',
  support_automation_rules: 'fc18c94d6401eb07d96cf6c1f7b74fa1bb11c03341adbe94c6c41134d136529c',
  support_conversation_events: '351e19ffb42335f5de669d8a7f006fdae0ce710fdb9d97cca796d60fd534a1d0',
  support_conversations: '64aa01a2b74975dcfd468b70127ec0f8617347b539d9c201186f40fea40af625',
  support_identity_links: 'a79004a14487ad13bf09813ab33d685cfafdd1b04afbfc15310c4467ae0eb42a',
  support_inboxes: '848bac741efde32519bba9f58007bde04f756e361eb2c79eab071c318418b073',
  support_label_assignments: '62bcae9d0547e22e9628814d15d73512b4c583dcefd93860d537a3c0762cc5ad',
  support_labels: '2fe1e3f3fe4c27240cfaf3fab66e331841e3a3aa80e03b1362b791342479113a',
  support_macros: 'e60286afb22495b48efe1991d9efddbaa5244374f55c80f67d0b4efd43005b30',
  support_notes: '746ca7eaf4ce15fbc65d18cddaa9fe2fd6fdc4eb862e1d559877bda261638a2c',
  support_reporting_events: '7152cfb7d95ebec7c650911034afb0a398c9ce394fd0de6244ec142fc1fec431',
  support_reporting_rollups: '722f0ac0dcf53f3cf1465b8ef07e3471f4cbf33b746748511b407eb3acb81ade',
  support_sla_events: 'f88c8c62da253ca76afddc746e38d8b7c3b8a0fe01f1d28058387431da1651dc',
  support_sla_policies: 'c8f69d71947f117259d132c02e7b513d20a9467d400c2c08b573618177724953',
  support_thread_links: 'c144074785a1e22697f7f2ebc30b297d404fd3ff2bfca797b78371e9f205a8be',
  support_view_filters: 'e8988877eef64c437758f90f5d6868d8310122bb5f78e854fad31d256d3cafe5',
  support_views: '10ac9212258aef30b798d1d4e6d58712b9f59ee725966a8c7bd0fa49f72c1033',
  user_notifications: '28593fbad81de44fc2218886d67284cc140ca4b657bf75267412859a32753e5b',
  user_thread_links: 'cc911076015a884b58fda2b28b5e8d840b048e78d958081429db31d573916129',
  user_thread_messages: '27f6e6e683c5ae1ccce85e4a73ce6d7df44639faeaad85f9f3fbadf0762a573a',
  user_threads: '5074a07e8b5b03b69f6f47e4f908cbbe52b920a10f0ce615459afb3af47edb63',
});

export function canonicalJson(value) {
  return JSON.stringify(sortCanonical(value));
}

export async function sha256Hex(text) {
  if (!globalThis.crypto?.subtle) {
    throw new Error('WebCrypto crypto.subtle is required for CTOX schema hashes');
  }
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export async function schemaHash(schema, collectionName = '') {
  const registryHash = CTOX_BUSINESS_OS_SCHEMA_HASHES[String(collectionName || '')];
  if (registryHash) return registryHash;
  return sha256Hex(canonicalJson(normalizeSchema(schema)));
}

export function schemaHashSource(collectionName = '') {
  return CTOX_BUSINESS_OS_SCHEMA_HASHES[String(collectionName || '')]
    ? CTOX_SCHEMA_HASH_SOURCES.businessOsRegistry
    : CTOX_SCHEMA_HASH_SOURCES.canonicalJson;
}

export function normalizeSchema(schema) {
  if (!schema || typeof schema !== 'object') {
    throw new TypeError('schema must be an object');
  }
  const normalized = structuredCloneSafe(schema);
  delete normalized.hash;
  normalized.version = Number.isFinite(normalized.version) ? normalized.version : 0;
  normalized.type = typeof normalized.type === 'string' && normalized.type ? normalized.type : 'object';
  normalized.properties = normalized.properties && typeof normalized.properties === 'object'
    ? normalized.properties
    : {};
  normalized.required = Array.isArray(normalized.required)
    ? normalized.required.map(String)
    : [];
  normalized.indexes = Array.isArray(normalized.indexes)
    ? normalized.indexes.map(normalizeSchemaIndex)
    : [];
  normalized.encrypted = Array.isArray(normalized.encrypted)
    ? normalized.encrypted.map(String)
    : [];
  normalized.keyCompression = normalized.keyCompression === true;
  normalized.additionalProperties = false;

  normalized.properties._rev = { type: 'string', minLength: 1 };
  normalized.properties._attachments = { type: 'object' };
  normalized.properties._deleted = { type: 'boolean' };
  normalized.properties._meta = rxMetaSchema();

  for (const field of ['_deleted', '_rev', '_meta', '_attachments']) {
    if (!normalized.required.includes(field)) normalized.required.push(field);
  }
  normalized.required.push(...finalSchemaFields(normalized));
  const requiredSeen = new Set();
  normalized.required = normalized.required.filter((field) => {
    if (field.includes('.') || requiredSeen.has(field)) return false;
    requiredSeen.add(field);
    return true;
  });

  const primaryPath = primaryFieldOfPrimaryKey(normalized.primaryKey);
  const indexes = normalized.indexes.map((index) => {
    const next = index.slice();
    if (!next.includes(primaryPath)) next.push(primaryPath);
    if (next[0] !== '_deleted') next.unshift('_deleted');
    return next;
  });
  if (indexes.length === 0) indexes.push(['_deleted', primaryPath]);
  indexes.push(['_meta.lwt', primaryPath]);
  if (Array.isArray(normalized.internalIndexes)) {
    for (const index of normalized.internalIndexes) indexes.push(normalizeSchemaIndex(index));
  }
  const indexSeen = new Set();
  normalized.indexes = indexes.filter((index) => {
    const key = index.join(',');
    if (indexSeen.has(key)) return false;
    indexSeen.add(key);
    return true;
  });
  return normalized;
}

function primaryFieldOfPrimaryKey(primaryKey) {
  if (typeof primaryKey === 'string' && primaryKey) return primaryKey;
  if (primaryKey && typeof primaryKey === 'object' && typeof primaryKey.key === 'string' && primaryKey.key) {
    return primaryKey.key;
  }
  return 'id';
}

function normalizeSchemaIndex(index) {
  if (Array.isArray(index)) return index.map(String);
  return [String(index)];
}

function finalSchemaFields(schema) {
  const fields = [];
  for (const [name, property] of Object.entries(schema.properties || {})) {
    if (property && typeof property === 'object' && property.final === true) fields.push(name);
  }
  fields.push(primaryFieldOfPrimaryKey(schema.primaryKey));
  if (schema.primaryKey && typeof schema.primaryKey === 'object' && Array.isArray(schema.primaryKey.fields)) {
    for (const field of schema.primaryKey.fields) fields.push(String(field));
  }
  return fields;
}

function rxMetaSchema() {
  return {
    type: 'object',
    properties: {
      lwt: {
        type: 'number',
        minimum: 1,
        maximum: 1000000000000000,
        multipleOf: 0.01,
      },
    },
    required: ['lwt'],
    additionalProperties: true,
  };
}

export function buildProtocolPayload({
  collectionName,
  schemaVersion,
  schemaHash: hash,
  schemaHashSource: source,
  peerSessionId,
  peerGeneration,
  checkpoint,
  role = 'browser',
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
} = {}) {
  const checkpointEvidence = checkpoint || null;
  const peerSession = {
    role,
    sessionId: peerSessionId || null,
    generation: Number.isFinite(peerGeneration) ? peerGeneration : null,
  };
  const cleanCapabilityToken = typeof capabilityToken === 'string' ? capabilityToken.trim() : '';
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
      checkpoint: checkpointEvidence,
    } : null,
    // `{ collectionName: { schemaVersion, schemaHash, schemaHashSource } }`.
    // Omitted (null) for single-collection rooms so the legacy single-
    // collection handshake stays byte-identical.
    collectionSchemas: normalizeCollectionSchemas(collectionSchemas),
    collectionCheckpoints: normalizeCollectionCheckpoints(collectionCheckpoints),
    peerSession,
    capabilities: Array.from(new Set([
      ...CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
      ...capabilities,
    ])).sort(),
  };
}

function normalizeCollectionSchemas(map) {
  if (!map || typeof map !== 'object') return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== 'object') continue;
    out[name] = {
      schemaVersion: Number.isFinite(entry.schemaVersion) ? entry.schemaVersion : null,
      schemaHash: entry.schemaHash || null,
      schemaHashSource: entry.schemaHashSource || schemaHashSource(name),
    };
  }
  return Object.keys(out).length > 0 ? out : null;
}

function normalizeCollectionCheckpoints(map) {
  if (!map || typeof map !== 'object') return null;
  const out = {};
  for (const [name, entry] of Object.entries(map)) {
    if (!name || !entry || typeof entry !== 'object') continue;
    out[name] = {
      ...entry,
      collection: typeof entry.collection === 'string' && entry.collection ? entry.collection : name,
    };
  }
  return Object.keys(out).length > 0 ? out : null;
}

export function assertCompatibleProtocol(local, remote, {
  requiredCapabilities = CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  validateSchema = true,
} = {}) {
  if (!remote || typeof remote !== 'object') {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMissing,
      message: 'CTOX RxDB WebRTC protocol payload is missing.',
      expected: CTOX_RXDB_PROTOCOL,
      actual: null,
    });
  }
  if (!remote.protocol) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMissing,
      message: 'CTOX RxDB WebRTC protocol marker is missing.',
      expected: CTOX_RXDB_PROTOCOL,
      actual: null,
    });
  }
  if (remote.protocol !== CTOX_RXDB_PROTOCOL) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.protocolMismatch,
      message: 'Incompatible CTOX RxDB WebRTC protocol.',
      expected: CTOX_RXDB_PROTOCOL,
      actual: remote.protocol,
    });
  }
  const remoteCapabilities = new Set(
    Array.isArray(remote.capabilities)
      ? remote.capabilities.filter((capability) => typeof capability === 'string' && capability)
      : [],
  );
  for (const capability of requiredCapabilities || []) {
    if (!remoteCapabilities.has(capability)) {
      throw createProtocolCompatibilityError({
        code: CTOX_PROTOCOL_ERROR_CODES.capabilityMissing,
        message: `Remote CTOX RxDB peer is missing required capability ${capability}.`,
        expected: capability,
        actual: Array.from(remoteCapabilities).sort(),
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
      collection: localCollection.name,
    });
  }
  if (
    validateSchema
    && (
    Number.isFinite(localCollection.schemaVersion)
    && Number.isFinite(remoteCollection.schemaVersion)
    && localCollection.schemaVersion !== remoteCollection.schemaVersion
    )
  ) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.schemaVersionMismatch,
      message: `CTOX RxDB schema version mismatch for ${localCollection.name || remoteCollection.name || 'collection'}.`,
      expected: localCollection.schemaVersion,
      actual: remoteCollection.schemaVersion,
      collection: localCollection.name || remoteCollection.name || null,
    });
  }
  if (
    validateSchema
    && localCollection.schemaHash
    && remoteCollection.schemaHash
    && localCollection.schemaHash !== remoteCollection.schemaHash
  ) {
    throw createProtocolCompatibilityError({
      code: CTOX_PROTOCOL_ERROR_CODES.schemaHashMismatch,
      message: `CTOX RxDB schema hash mismatch for ${localCollection.name || remoteCollection.name || 'collection'}.`,
      expected: localCollection.schemaHash,
      actual: remoteCollection.schemaHash,
      collection: localCollection.name || remoteCollection.name || null,
    });
  }
  return true;
}

// Phase 3 schema-validation hardening: validate EACH collection's schema hash
// individually under multiplex. `localSchemas` is `{ name -> { schemaVersion,
// schemaHash } }` for the collections THIS peer carries on the connection;
// `remote` is the negotiated remote protocol payload (which carries
// `collectionSchemas`). Returns a Map<collectionName, Error> for the
// collections that mismatched (empty Map when all compatible). Collections the
// remote does not advertise are NOT flagged — the remote simply does not serve
// them, which is benign (the local fork just never receives rows). This
// replaces the old `validateSchema: collections.size <= 1` wholesale skip:
// every collection's hash/version is now checked, and only the mismatched
// collection is skipped (its error surfaced) rather than disabling validation
// for the whole room.
export function assertCollectionSchemasCompatible(localSchemas, remote) {
  const mismatches = new Map();
  const remoteSchemas = (remote && typeof remote.collectionSchemas === 'object' && remote.collectionSchemas)
    ? remote.collectionSchemas
    : {};
  for (const [name, local] of Object.entries(localSchemas || {})) {
    const remoteEntry = remoteSchemas[name];
    // The remote did not advertise this collection — it does not serve it on
    // this connection. Not a mismatch; just no rows for that collection.
    if (!remoteEntry || typeof remoteEntry !== 'object') continue;
    const localVersion = Number.isFinite(local?.schemaVersion) ? local.schemaVersion : null;
    const remoteVersion = Number.isFinite(remoteEntry.schemaVersion) ? remoteEntry.schemaVersion : null;
    if (localVersion !== null && remoteVersion !== null && localVersion !== remoteVersion) {
      mismatches.set(name, createProtocolCompatibilityError({
        code: CTOX_PROTOCOL_ERROR_CODES.schemaVersionMismatch,
        message: `CTOX RxDB schema version mismatch for ${name}.`,
        expected: localVersion,
        actual: remoteVersion,
        collection: name,
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
        collection: name,
      }));
    }
  }
  return mismatches;
}

function normalizeProtocolCollection(payload) {
  const collection = payload?.collection && typeof payload.collection === 'object'
    ? payload.collection
    : {};
  return {
    name: collection.name || payload?.collectionName || payload?.collection || null,
    schemaVersion: Number.isFinite(collection.schemaVersion)
      ? collection.schemaVersion
      : (Number.isFinite(payload?.schemaVersion) ? payload.schemaVersion : null),
    schemaHash: collection.schemaHash || payload?.schemaHash || null,
  };
}

function createProtocolCompatibilityError({
  code,
  message,
  expected = null,
  actual = null,
  collection = null,
}) {
  const error = new Error(message);
  error.name = 'CtoxRxdbProtocolError';
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
  if (!value || typeof value !== 'object') {
    return value;
  }
  const sorted = {};
  for (const key of Object.keys(value).sort()) {
    const next = value[key];
    if (typeof next !== 'undefined') {
      sorted[key] = sortCanonical(next);
    }
  }
  return sorted;
}

function structuredCloneSafe(value) {
  if (typeof structuredClone === 'function') {
    return structuredClone(value);
  }
  return JSON.parse(JSON.stringify(value));
}
