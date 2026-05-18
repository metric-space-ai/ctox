# Deployment Rules

- Distinguish local installation from external integration first.
- Local installation means CTOX may generate and store local admin credentials.
- External integration means CTOX may need owner-supplied remote endpoints or credentials.
- Persist preflight evidence before mutation.
- Persist a blocker if verification fails or if a true owner-supplied input is missing.
- Treat verification as layered rather than binary:
  - process/runtime exists
  - listener reachable
  - HTTP/UI reachable
  - authenticated API or admin identity works
  - mutating smoke action works when safe
  - result persists across a fresh verification read
- If any higher verification layer fails, the deployment is not complete even if the process or web UI is up.
- A `401`, missing admin login, stale secret, or unknown auth path is a deployment repair problem, not a cosmetic detail.
