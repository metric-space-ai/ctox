# Business OS App Access And Roles Guide

This guide explains the current CTOX Business OS app-access model in product
language. It describes the implemented behavior only.

## Roles

| Label | Meaning |
| --- | --- |
| Owner | Final workspace responsibility, release policy and recovery decisions. |
| Admin | Operational management of users and grants. Admin does not assign Owner. |
| App-Verantwortliche:r | Builds or manages assigned apps. Visibility is scoped to covered apps. |
| Teammitglied | Uses released or explicitly shared apps. Cannot change apps by default. |

App visibility and data access are separate. A person can be allowed to see an
app without being allowed to edit it, view source, release it or read its data.

## App Versions And Visibility

| App signal | What users should expect |
| --- | --- |
| `0.x.y` / Privat | Draft/private app. Visible to responsible app builders or explicitly shared users, not automatically to the whole team. |
| Vorschau | Shared with selected users for preview. Does not grant data or edit rights. |
| `1.0.0+` / Team | Released team app. Visible to the team unless it is restricted. |
| Eingeschraenkt | Released app with a smaller audience. Visible only to selected users and app managers. |

The app icon, tab, app bar and App Store card should show the version and
visibility signal. Clicking the signal opens app governance details.

## Changing Apps

`App aendern` is available only when the actor can manage that app. Team
members do not get this action by default. Source access is separate:
read-only source access does not imply the right to save changes.

## Publishing Apps

Publishing happens in the App Store with `Freigeben`. The release flow reviews
target version, source snapshot, release notes, rollback target, responsible
users and data access. Settings shows diagnostics and read-only release state;
it is not the active release/rollback surface.

Publishing an app does not create hidden data grants. Data access must be
explicitly granted or declared as a locked data area where the app renders a
restricted state.

## AI And Agents

AI-assisted actions show the acting user, selected app, version, lifecycle,
selected data scope and external-action state. MCP and agents first pass app
visibility, then data policy. External effects are disabled for this rollout
unless a later release adds an explicit approval model.

## Diagnostics

Use `Warum?` to understand why a user can or cannot see, open, edit, release,
rollback or access data through an app. Use `Support-Paket` to export
support-safe evidence. These surfaces must not expose prompt text, selected
text, record bodies, message bodies, tokens or secrets.
