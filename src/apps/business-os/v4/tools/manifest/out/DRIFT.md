# Drift-Bericht Shell v4 Manifeste (generiert, nicht von Hand ändern)

Erzeugt von `gen-manifest.mjs`, Basis main@4794af03d. a = schema.js (soweit noch vorhanden), b = collections.schema.json.

## Zahlen

| Kennzahl | Wert |
|---|---|
| apps | 41 |
| manifests_total | 42 |
| collections_total | 237 |
| collections_in_contract | 233 |
| owned_by_apps | 221 |
| owned_by_platform | 16 |
| owner_proposals | 6 |
| handlers_total | 116 |
| apps_with_handlers | 15 |
| legacy_total | 0 |
| capabilities_distinct | 41 |
| triggers_total | 10 |
| command_rights_entries | 42 |
| foreign_declarations | 297 |
| foreign_write_declarations | 32 |
| foreign_code_only | 94 |
| foreign_without_export | 0 |
| platform_clamps | 0 |
| exported_collections | 84 |
| handler_conflicts | 0 |
| handlers_platform_type | 282 |
| schema_missing | 0 |
| schema_js_present | 3 |
| schema_js_load_errors | 0 |
| schema_from_v4_schemas | 41 |
| no_owner_app | 6 |
| contract_only | 12 |
| multiple_owners | 0 |
| drift_schema_js_vs_json_collections | 29 |
| drift_migration_steps | 3 |
| drift_copy_divergence | 0 |
| prefix_mismatch | 2 |
| class_proposals | 22 |
| shell_files | 126 |
| apps_with_styles | 41 |
| write_guards | 1 |
| todo_total | 181 |
| declarations | 40 |
| commands_out_total | 20 |
| commands_in_total | 7 |
| commands_unmatched | 0 |
| commands_unrouted | 0 |
| commands_out_ambiguous | 0 |
| commands_routes_pending | 13 |
| commands_in_derived | 1 |
| commands_out_unresolved | 1 |
| commands_in_without_handler | 0 |
| exports_beyond_declaration | 0 |
| net_ingest_total | 2 |
| multi_instance_false | 40 |
| icon_raster | 40 |
| icon_svg | 1 |
| icon_generated |  |

## 1. schema.js vs. collections.schema.json je App (29 Fälle)

| App | Collection | Art | Feld | schema.js | collections.schema.json |
|---|---|---|---|---|---|
| conversations | communication_accounts | differs | `$.version` | 0 | 1 |
| conversations | communication_accounts | differs | `$.primaryKey` | "account_key" | "id" |
| conversations | communication_accounts | differs | `$.required` | nur: – | nur: id |
| conversations | communication_accounts | differs | `$.indexes` | nur: – | nur: account_key |
| conversations | communication_accounts | only_b | `properties.id` | – | {"type":"string","maxLength":256} |
| conversations | communication_messages | differs | `$.version` | 0 | 1 |
| conversations | communication_messages | differs | `$.primaryKey` | "message_key" | "id" |
| conversations | communication_messages | differs | `$.required` | nur: – | nur: id |
| conversations | communication_messages | differs | `$.indexes` | nur: – | nur: message_key |
| conversations | communication_messages | only_b | `properties.id` | – | {"type":"string","maxLength":256} |
| conversations | communication_threads | differs | `$.version` | 0 | 1 |
| conversations | communication_threads | differs | `$.primaryKey` | "thread_key" | "id" |
| conversations | communication_threads | differs | `$.required` | nur: – | nur: id |
| conversations | communication_threads | differs | `$.indexes` | nur: – | nur: thread_key |
| conversations | communication_threads | only_b | `properties.id` | – | {"type":"string","maxLength":256} |
| conversations | outbound_approvals | only_in_schema.js | | | |
| conversations | outbound_campaigns | only_in_schema.js | | | |
| conversations | outbound_engagements | only_in_schema.js | | | |
| conversations | outbound_messages | only_in_schema.js | | | |
| conversations | outbound_pipeline_items | only_in_schema.js | | | |
| ctox | business_commands | only_in_schema.js | | | |
| ctox | business_module_catalog | only_in_schema.js | | | |
| ctox | communication_accounts | only_in_schema.js | | | |
| ctox | ctox_crew_assignments | only_in_collections.schema.json | | | |
| ctox | ctox_crew_continuity | only_in_collections.schema.json | | | |
| ctox | ctox_crew_continuity_commits | only_in_collections.schema.json | | | |
| ctox | ctox_crew_learnings | differs | `$.version` | 0 | 1 |
| ctox | ctox_crew_learnings | differs | `$.required` | nur: – | nur: normalized_text |
| ctox | ctox_crew_learnings | differs | `$.indexes` | nur: – | nur: member_id+normalized_text |
| ctox | ctox_crew_learnings | only_b | `properties.normalized_text` | – | {"type":"string"} |
| ctox | ctox_crew_members | only_b | `properties.created_at` | – | {"type":"string","default":""} |
| ctox | ctox_crew_members | only_b | `properties.created_at_ms` | – | {"type":"number","default":0} |
| ctox | ctox_crew_members | only_b | `properties.updated_at` | – | {"type":"string","default":""} |
| ctox | ctox_queue_tasks | only_in_schema.js | | | |
| ctox | ctox_runtime_settings | only_in_schema.js | | | |
| ctox | legacy_records | only_in_collections.schema.json | | | |
| ctox | planned_steps | only_in_collections.schema.json | | | |
| ctox | workjet_jour_fixe_meetings | only_in_collections.schema.json | | | |
| ctox | workjet_provider_registry | differs | `$.version` | 0 | 1 |
| ctox | workjet_provider_registry | differs | `properties.accounts` | {"type":"array","maxItems":256,"items":{"type":"object","additionalProperties… | {"items":{"additionalProperties":false,"properties":{"credentialReady":{"type… |
| ctox | workjet_session_transfers | differs | `$.version` | 0 | 1 |
| ctox | workjet_session_transfers | only_b | `properties.applied_at_ms` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.apply_mismatch_count` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.apply_mismatch_sha256` | – | {"type":"string","maxLength":64} |
| ctox | workjet_session_transfers | only_b | `properties.last_terminal_turn_id` | – | {"type":["string","null"],"maxLength":160} |
| ctox | workjet_session_transfers | only_b | `properties.observed_head` | – | {"type":"string","maxLength":64} |
| ctox | workjet_session_transfers | only_b | `properties.observed_manifest_sha256` | – | {"type":"string","maxLength":64} |
| ctox | workjet_session_transfers | only_b | `properties.observed_tree_sha256` | – | {"type":"string","maxLength":64} |
| ctox | workjet_session_transfers | only_b | `properties.packed_at_ms` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.pause_acked_at_ms` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.recovery_aborting_at_ms` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.source_git_repository` | – | {"type":"boolean"} |
| ctox | workjet_session_transfers | only_b | `properties.switched_at_ms` | – | {"type":"integer","minimum":0} |
| ctox | workjet_session_transfers | only_b | `properties.target_partial_cleanup_required` | – | {"type":"boolean"} |
| ctox | workjet_session_transfers | only_b | `properties.worker_instance_id` | – | {"type":"string","maxLength":256} |
| ctox | workjet_sessions | differs | `$.version` | 0 | 1 |
| ctox | workjet_sessions | differs | `properties.last_terminal_turn_id` | {"type":"string","maxLength":160} | {"maxLength":160,"type":["string","null"]} |
| outbound | outbound_sender_assignments | differs | `$.version` | 0 | 1 |
| outbound | outbound_sender_assignments | differs | `$.required` | nur: campaign_id | nur: – |
| outbound | outbound_suppression_entries | differs | `$.version` | 0 | 1 |
| outbound | outbound_suppression_entries | differs | `$.required` | nur: active, scope, value | nur: – |
| outbound | outbound_suppression_entries | only_b | `properties.domain` | – | {"type":"string"} |
| outbound | outbound_suppression_entries | only_b | `properties.email` | – | {"type":"string"} |
| outbound | outbound_suppression_entries | only_b | `properties.engagement_id` | – | {"type":"string"} |
| outbound | outbound_suppression_entries | only_b | `properties.recipient_email` | – | {"type":"string"} |
| outbound | outbound_suppression_entries | only_b | `properties.status` | – | {"type":"string"} |
| outbound | outbound_suppression_entries | only_b | `properties.suppression_reason` | – | {"type":"string"} |
| outbound | outbound_tracking_endpoints | only_in_collections.schema.json | | | |
| outbound | outbound_tracking_events | only_in_collections.schema.json | | | |
| outbound | outbound_tracking_tokens | only_in_collections.schema.json | | | |

## 2. Migrationsstufen schema.js vs. collections.schema.json (3)

| App | Collection | schema.js | collections.schema.json |
|---|---|---|---|
| conversations | outbound_messages | [1] | [] |
| ctox | business_commands | [1,2] | [] |
| ctox | ctox_queue_tasks | [1,2,3] | [] |

## 3. schema.js nicht ladbar (0)

| App | Fehler |
|---|---|

## 4. Nur im Schema-Vertrag, in keiner collections.schema.json (12)

| Collection | Eigentümer |
|---|---|
| app_activations | ctox-platform |
| app_drafts | ctox-platform |
| app_user_preferences | ctox-platform |
| business_app_versions | ctox-platform |
| business_apps | ctox-platform |
| business_commands | ctox-platform |
| business_module_catalog | ctox-platform |
| ctox_queue_tasks | ctox-platform |
| ctox_runtime_settings | ctox-platform |
| files | ctox-platform |
| mail_server_state | ctox-platform |
| shell_preferences | ctox-platform |

## 5. Ohne definierende App (6)

| Collection | Eigentümer | Grundlage |
|---|---|---|
| app_catalog | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |
| app_user_preferences | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |
| coding_turns | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |
| devices | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |
| machine_enrollments | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |
| mail_server_state | ctox-platform | keine App definiert die Collection; keine Präfixregel → Plattform |

## 6. Mehrere Definierer in collections.schema.json (0)

| Collection | Definierer | Eigentümer |
|---|---|---|

## 7. Abweichende Kopien derselben Collection über Apps (0)

| Collection | Eigentümer | Variante (Quellen) | Feld | Variante | Referenz |
|---|---|---|---|---|---|

## 8. Eigentümer weicht von Namenspräfix ab (2)

| Collection | Eigentümer | Präfix legt nahe | Grundlage |
|---|---|---|---|
| ctox_task_approval_requests | threads | ctox | Basis-Zuordnung ownership-baseline.json (Strang M, 8a332b929) |
| outbound_research_policies | outbound-lead-generation | outbound | Entscheidung: §16.21 Eigentum: OLG (einziger Schreiber outbound.research_policy.publish; heute nur nativ in business_records) |

## 9. Eigene Collections ohne Schema (0)

| App | Collection |
|---|---|

## 10. Befehlstypen mit Handlern in mehreren Apps (0)

| Befehlstyp | Apps |
|---|---|

## 11. Plattformzugriffe auf exports beschränkt (0)

| App | Collection | gewünscht | gewährt | Grund |
|---|---|---|---|---|

## 12. foreign ohne passendes exports (0, Soll 0)

| App | Collection | Eigentümer | fehlende ops |
|---|---|---|---|
