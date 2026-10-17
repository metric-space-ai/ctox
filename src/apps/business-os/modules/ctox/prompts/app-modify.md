Modifiziere die App {{source_app}} anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Daten selbst nicht als primäres Ziel verändern.

{{text}}
Business OS task resources:
- required_skills: business-os-app-module-development
- skill_context: read-only; edit the requested app artifacts, not skill files, unless the task explicitly targets skill maintenance

Business OS app task metadata:
- module_id: {{module_id}}
- install_target: {{install_target}}
- app_directory: {{app_directory}}
- skill: business-os-app-module-development
- resource.shell_v2_contract: src/skills/system/product_engineering/business-os-app-module-development/references/shell-v2-contract.md
- canonical_starter: src/apps/business-os/app-starter/v2/ (pre-materialized for import jobs; modify it in place)
- resource.module_contract: src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md
- resource.design_guide: src/skills/system/product_engineering/business-os-app-module-development/references/design-guide.md
- resource.standalone_porting: src/skills/system/product_engineering/business-os-app-module-development/references/standalone-porting.md
- resource.dos_and_donts: src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md
- resource.green_checklist: src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md
- resource.architecture_translation: src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md
- reference_catalog: ctox business-os app references --query "<workflow data keywords>" --json --limit 8
- validation: ctox business-os app validate {{module_id}} {{mode_flag}}
- go_no_go: declared collections need reviewed data.read/data.write grants; validate and smoke with the real tenant actor; only core-recorded passed smoke evidence may publish an imported app.
- tool_boundary: do not run ctox stop/start/upgrade, launchctl, systemctl, or service lifecycle commands during app creation; the running CTOX service is the required app runtime.

Business OS app request summary:
{{request_summary}}
Business OS command:
- command_id: {{command_id}}
- module: {{source_app}}
- type: {{command_type}}
- record_id: {{record_id}}

The full command payload and client context remain persisted on the Business OS command record. Do not copy raw client context into app code or prompts.
