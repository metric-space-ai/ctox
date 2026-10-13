# Native Jour fixe preparation brief

The T−2h native schedule saves one owner/project/thread-scoped JSON snapshot
before admitting the registered supervisor turn. Replaying that occurrence
uses the same snapshot. The queue prompt carries the complete brief in a
separate enrichment block, outside the truncated payload preview. The embedded
jour-fix skill is unchanged.

The snapshot contains the previous completed meeting date, the confirmed goal
and its persisted plan progress, retained worker outcomes, native KPI state,
project context, and repository facts. Each source has a status and data.
Unavailable facts stay unavailable; source errors and diagnostics are
preparation inputs and must never become slide content. Sections are bounded
with explicit truncation; the complete serialized brief is at most 12 KiB.

## Repository binding

The owner command `ctox.workjet.project.upsert` accepts `repository_url`
alongside the existing `repo_url`. They are independent fields. An omitted
field preserves its stored value; null clears it. Project ownership and native
command policy remain unchanged. The RxDB projection carries the new field;
schema version 5 migrates old documents without changing their contents.

Example owner payload (retain the real project ID/name):

```json
{"project_id":"<existing project ID>","name":"miltonticket.app","repository_url":"https://github.com/mkh-welsch/miltonticket-app"}
```

Only after the supervisor deploys this PR, apply these bindings through the
normal owner command and retain the projected readback:

| Project | repository_url |
|---|---|
| ctox.dev | https://github.com/mkh-welsch/ctox-dev |
| greppy.xyz | https://github.com/metric-space-ai/greppy |
| Molecularity | https://github.com/metric-space-ai/molecularity |
| Learordie.app | https://github.com/metric-space-ai/learnordie |
| kunstmen.com | https://github.com/metric-space-ai/kunstmen |
| miltonticket.app | https://github.com/mkh-welsch/miltonticket-app |
| fzul.app | https://github.com/mkh-welsch/fzul-app |
| flylabs.dev | https://github.com/mkh-welsch/flylabs |

Leave metric-space.ai, mypokedex.app, I-hate-AI.community and dommify.dev unbound.

## GitHub source

The proposed existing secret-store entry is
`credentials/GITHUB_READ_TOKEN`. This implementation creates no credential.
At runtime an absent entry uses the public GitHub REST API. A credential-store
error is an unavailable source and is not silently treated as an absent token.
No token or provider error text reaches the brief, command or prompt.

Repository URLs are parsed before requests. Only HTTPS github.com owner/repo
paths without credentials, ports, queries or fragments are supported. Requests
go to the fixed api.github.com origin, disable redirects, have a four-second
deadline each and accept at most 256 KiB per response. There are four requests,
no retries: merged/closed PRs (50), open PRs (50), releases (20) and tags (20).
Closed PRs count only if merged_at lies in the window. Releases use published_at.
Tags have no date in this API and are labeled latest tags, never as releases
since the previous meeting. Reaching a list cap and brief truncation remain
explicit. The first meeting uses a labeled seven-day source window.

This is an external source adapter, not a Business OS HTTP data plane.
Project, meeting and command records continue through the existing RxDB path.
The native brief and command-binding tables are not browser-writable collections.
