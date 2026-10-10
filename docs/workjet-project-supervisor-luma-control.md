# Workjet project Supervisor selection bridge

The Shell project control maps `project.configure.supervisorLumaId` to the existing Owner-checked native `ctox.workjet.project.upsert.supervisor_luma_id` field. The native `ctox.workjet.supervisor_luma.v1` fixture and generated browser validator are the source of type and length limits.

An omitted field preserves the existing selection. A non-empty ID of at most 160 Unicode characters selects an instance-wide Luma reference. Explicit `null` clears the selection. A configuration response must confirm the same selected value, or confirm the absence of a selection after a clear. An unrelated or unconfirmed selection fails instead of being shown as saved.

Project list reads expose `supervisorLumaId` only with the dedicated `includeSupervisorLuma:true` opt-in, and only when the stored native record has that field. Existing `includeConfiguration:true` reads alone retain their old result shape. Configuration writes return the selection only when the request supplied it. A successful explicit clear returns `supervisorLumaId:null`.

The bridge carries no route, model, account selector, credential or execution authority. Stored selection does not prove that a provider ran. Omitted selection preserves the existing Supervisor execution behavior. All writes and reads retain the current native Owner receipt and browser session fences, and use the existing RxDB/WebRTC command path.

Regression coverage exercises the generated native corpus, set/keep/clear, invalid type and bounds, forged execution fields, mismatched native acknowledgements, and configuration-only list projection. The browser fixture exercises the same actual Shell control.
