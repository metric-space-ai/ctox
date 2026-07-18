# Coding Agents

Business OS workbench for delegating coding tasks to CTOX's built-in **pi coding
agent**. Pick a Business OS app, describe a change, and the agent edits the app's
source inside a sandboxed projection — the result is recorded as a versioned
commit in the app's source history.

The module dispatches through the shared `business_commands` shell collection:

- `ctox.coding.turn` — run one bounded coding turn on a module. Payload
  `{ module_id, prompt, model? }`. `model` is optional: by default the agent uses
  the **same model/provider as CTOX**; any pi provider can be sent to override
  it. The native owner (`src/core/coding_agents`) projects the app source, drives
  one bounded turn through the embedded pi sidecar, and applies the resulting
  snapshot back as source versions/commits.

Recent turns are read from the command log and shown in the workbench. The
module also declares these collections in `collections.schema.json` for durable
session/event state:

- `coding_agent_workspace_grants`
- `coding_agent_sessions`
- `coding_agent_events`

The pi coding sidecar is embedded in the `ctox` binary
(`src/core/coding_agents/pi-sidecar`, a self-contained bundle of the pinned Pi
coding-agent core) and runs as a **bounded, sandboxed leaf process** — a fresh
daemon per turn, killed on completion, with no host filesystem and none of the
CTOX daemon's environment/secrets. See the coding-harness delegation notes for
the app-source versioning and delegation design.
