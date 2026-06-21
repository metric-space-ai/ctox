# Business OS App Creation And Modification Entry Points

Date: 2026-06-21
Scope: inventory of ways CTOX can create, install, update, or modify Business OS apps/modules, plus the video evidence required for each path.

## Evidence Rule

Every supported path below needs a short computer-use screen recording that shows:

1. the entry point the user or external agent uses,
2. the concrete app/module target,
3. the resulting command/tool type,
4. the accepted queue task, command projection, saved file, installed module, or lifecycle outcome.

Videos should be cut per path, not as one long recording. Store them under `runtime/evidence/app-entrypoints-20260621/videos/` or another ignored evidence directory, with the filenames listed below.

## Supported Product Paths

| ID | Path | Creates Or Modifies | Command / Tool | Evidence Clip |
| --- | --- | --- | --- | --- |
| UI-01 | App Creator, "Neue App beschreiben & installieren" submit | creates a runtime-installed app | `ctox.business_os.app.create` | `01-ui-app-creator-create.webm` |
| UI-02 | App Store card "Neue App per KI-Prompt erstellen" | opens App Creator scratch flow, then creates via UI-01 | App Store route to `creator?source=app-store&mode=scratch`, then `ctox.business_os.app.create` | `02-ui-app-store-scratch-to-creator.webm` |
| UI-03 | App Creator installed-app "Upgrade vorbereiten" | rebuilds/updates an existing runtime app through Creator | `creator?upgrade=<module-id>`, then `ctox.business_os.app.create` for the target module id | `03-ui-creator-upgrade-existing-app.webm` |
| UI-04 | App Store managed app "Bearbeiten" | opens Creator upgrade flow for selected app | `creator?mode=upgrade&upgrade=<module-id>`, then same as UI-03 | `04-ui-app-store-edit-to-creator-upgrade.webm` |
| UI-05 | Global shell context menu "App ändern" | modifies active app through Business Chat | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `05-ui-global-context-app-modify.webm` |
| UI-06 | App Store selected-app context menu "App ändern" | modifies selected installed/local app, not necessarily App Store itself | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` with selected `module_id/app_id` | `06-ui-app-store-selected-context-modify.webm` |
| UI-07 | Module-local context menus "App ändern/App modifizieren" | modifies the module that owns the local UI context | mostly `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | see per-module clips below |
| UI-08 | Source Editor save | directly modifies an app source file | `ctox.source.load` then `ctox.source.save` through `business_commands` | `08-ui-source-editor-save.webm` |
| UI-09 | App Store marketplace install/update | installs or updates a packaged marketplace app into installed modules | `ctox.app_store.install` | `09-ui-app-store-marketplace-install-update.webm` |
| UI-10 | App Store template create | creates an installed module from a shipped template | `ctox.module.install_template` | `10-ui-app-store-template-install.webm` |
| UI-11 | Settings Admin "Modul hinzufügen" with template | creates an installed module from template | `ctox.module.install_template` | `11-ui-settings-template-install.webm` |
| UI-12 | Settings Admin edit existing module | modifies an existing module manifest only | `ctox.module.save` | `12-ui-settings-existing-module-save.webm` |
| UI-13 | App Store release dialog | modifies app lifecycle/release metadata | `ctox.module.release` | `13-ui-app-store-release.webm` |
| UI-14 | App Store version rollback | modifies active app version/lifecycle | `ctox.module.rollback_version` | `14-ui-app-store-version-rollback.webm` |
| UI-15 | Source snapshot rollback | reverts app source to a saved snapshot | `ctox.source.rollback_snapshot` | `15-ui-source-snapshot-rollback.webm` |
| UI-16 | Shell template-store drawer | creates an installed module from template if the drawer is reachable | `ctox.module.install_template` | `16-ui-shell-template-store-install.webm` |

## Module-Local App-Modify Clips

These are one implementation family, but each module owns its context menu code, so each exposed module needs its own short clip or a static finding that the action is not reachable in that module.

| Module | Command Path | Evidence Clip |
| --- | --- | --- |
| `ctox` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07a-ui-module-ctox-context-modify.webm` |
| `documents` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07b-ui-module-documents-context-modify.webm` |
| `reports` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07c-ui-module-reports-context-modify.webm` |
| `spreadsheets` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07d-ui-module-spreadsheets-context-modify.webm` |
| `notes` / `notizen` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07e-ui-module-notes-context-modify.webm` |
| `buchhaltung` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07f-ui-module-buchhaltung-context-modify.webm` |
| `shiftflow` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07g-ui-module-shiftflow-context-modify.webm` |
| `matching` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07h-ui-module-matching-context-modify.webm` |
| `conversations` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07i-ui-module-conversations-context-modify.webm` |
| `knowledge` | module dispatch path -> `ctox.business_os.app.modify` | `07j-ui-module-knowledge-context-modify.webm` |
| `research` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07k-ui-module-research-context-modify.webm` |
| `creator` | `ctox-business-os-chat-submit` -> `ctox.business_os.app.modify` | `07l-ui-module-creator-context-modify.webm` |

## CLI And Agent Paths

| ID | Path | Creates Or Modifies | Command / Tool | Evidence Clip |
| --- | --- | --- | --- | --- |
| CLI-01 | `ctox business-os app create --instruction ... --module-id ...` | creates a runtime-installed app task | `ctox.business_os.app.create` | `20-cli-app-create.webm` |
| CLI-02 | `ctox business-os app modify <module-id> --instruction ...` | modifies an app through an app task | `ctox.business_os.app.modify` | `21-cli-app-modify.webm` |
| CLI-03 | `ctox business-os commands dispatch --json ...` | low-level raw command-bus path for app create/modify | caller-provided `ctox.business_os.app.create` or `ctox.business_os.app.modify` | `22-cli-commands-dispatch-app-command.webm` |
| CLI-04 | `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k` | batch submits five app-create tasks for validation | five `ctox.business_os.app.create` commands | `23-cli-app-bench-core-five.webm` |
| MCP-01 | `ctox business-os mcp call business_os.create_app --args ...` | creates a runtime-installed app task through MCP | `business_os.create_app` -> `ctox.business_os.app.create` | `24-mcp-cli-create-app.webm` |
| MCP-02 | `ctox business-os mcp call business_os.modify_app --args ...` | modifies an app through MCP | `business_os.modify_app` -> `ctox.business_os.app.modify` | `25-mcp-cli-modify-app.webm` |
| MCP-03 | MCP HTTP JSON-RPC `/mcp` `tools/call` | external MCP client create/modify | `business_os.create_app` or `business_os.modify_app` | `26-mcp-http-tools-call-app.webm` |
| MCP-04 | Managed MCP gateway connect | remote external MCP client path | same MCP tools over gateway | `27-mcp-managed-gateway-create-modify.webm` |

## Not Supported Or Not Separate Paths

| Path | Status | Reason / Required Action |
| --- | --- | --- |
| Settings Admin "Blankes Modul" without template | red until fixed or removed | UI calls `ctox.module.save`, but server-side behavior says new apps must be created through App Creator or template install; `ctox.module.save` only updates existing module manifests. |
| Browser HTTP module mutation endpoints | not a current Browser Business OS app path | Browser mutations must use RxDB/WebRTC `business_commands`, not HTTP data bridges. Keep these out of green evidence unless explicitly testing legacy/control-plane behavior. |
| Plain inbound Teams/Jami/WhatsApp/email/TUI message | not currently an app-create/modify path by itself | Inbound skill inference does not route generic "build an app" text to `ctox.business_os.app.create`; app creation must enter through typed Business OS command, UI, CLI, or MCP. |
| Desktop app `creator` wrapper | not separate | It mounts the same App Creator module; evidence is covered by UI-01/UI-03. |
| Direct filesystem edits under `runtime/business-os/installed-modules` | not an official CTOX path | Official modification is Source Editor / `ctox.source.save` or app-create/modify tasks, so source snapshots, permissions, and projections stay intact. |
| `ctox business-os install --target ...` | not app creation | Installs Business OS into a customer-owned repository; it does not create an app module. |

## Source Anchors Checked

- App Creator create/upgrade: `src/apps/business-os/modules/creator/index.js`
- App Store scratch/edit/install/release/rollback/context: `src/apps/business-os/modules/app-store/index.js`
- Global shell context and Source Editor launch: `src/apps/business-os/app.js`
- Source Editor load/save: `src/apps/business-os/desktop-apps/code-editor/app.js`
- Settings module/template commands: `src/apps/business-os/shared/react-settings.js`
- Browser chat bridge: `src/apps/business-os/shared/business-chat.js`
- Native command handling and policy: `src/core/business_os/store.rs`
- CLI app create/modify/bench/dispatch: `src/core/service/business_os.rs`
- MCP create/modify and JSON-RPC transport: `src/core/business_os/mcp_channel.rs`
- Inbound non-routing check: `src/core/service/service.rs`
