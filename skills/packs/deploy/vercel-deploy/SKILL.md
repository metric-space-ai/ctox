---
name: vercel-deploy
description: Deploy applications and websites to Vercel. Use when the user requests deployment actions like "deploy my app", "deploy and give me the link", "push this live", or "create a preview deployment".
cluster: deploy
---

# Vercel Deploy

Deploy any project to Vercel instantly. **Always deploy as preview** (not production) unless the user explicitly asks for production.

For CTOX mission work, a Vercel deployment is not complete when HTML loads. Public deploys that depend on APIs, server-side handlers, or interactive buyer flows must be verified as a working public surface, not only as a successful upload.

## Prerequisites

- Check whether the Vercel CLI is installed **without** escalated permissions (for example, `command -v vercel`).
- Only escalate the actual deploy command if sandboxing blocks the deployment network calls (`sandbox_permissions=require_escalated`).
- The deployment might take a few minutes. Use appropriate timeout values.

## Quick Start

1. Check whether the Vercel CLI is installed (no escalation for this check):

```bash
command -v vercel
```

2. If `vercel` is installed, run this (with a 10 minute timeout):
```bash
vercel deploy [path] -y
```

**Important:** Use a 10 minute (600000ms) timeout for the deploy command since builds can take a while.

3. If `vercel` is not installed, or if the CLI fails with "No existing credentials found", use the fallback method below.

## Fallback (No Auth)

If CLI fails with auth error, use the deploy script:

```bash
skill_dir="<path-to-skill>"

# Deploy current directory
bash "$skill_dir/scripts/deploy.sh"

# Deploy specific project
bash "$skill_dir/scripts/deploy.sh" /path/to/project

# Deploy existing tarball
bash "$skill_dir/scripts/deploy.sh" /path/to/project.tgz
```

The script handles framework detection, packaging, and deployment. It waits for the build to complete and returns JSON with `previewUrl` and `claimUrl`.

**Tell the user:** "Your deployment is ready at [previewUrl]. Claim it at [claimUrl] to manage your deployment."

## Production Deploys

Only if user explicitly asks:
```bash
vercel deploy [path] --prod -y
```

For an existing linked production project, prefer using the linked project/team rather than creating a fresh ad hoc deployment path.

## CTOX Deployment Rules

When CTOX is deploying a real product surface, follow this order:

1. Confirm the linked Vercel project and team are the intended canonical target.
2. Inspect whether the project is shipping a static upload, a framework build, or a server/runtime-backed app.
3. If the app depends on live API routes, server handlers, or checkout/session state, verify at least:
   - the primary public page
   - one critical API route
   - one critical user-path action if feasible
4. Treat a static shell plus broken API routes as a failed deploy, even if Vercel says `Ready`.

### Custom Node Server Pitfall

If the workspace contains a local Node server such as:

- `http.createServer(...)`
- `server.listen(...)`
- ad hoc static serving plus `/api/*` handlers

do **not** assume Vercel will run it correctly by default.

This usually means one of the following is required:

- convert the app into Vercel-compatible serverless/API handlers
- add explicit `vercel.json` routing/build configuration
- split static assets and API functions into Vercel-native surfaces

If you see this failure pattern:

- public HTML loads
- `/api/*` returns `404 NOT_FOUND`
- Vercel deployment shows `Ready`
- build output is effectively static / no real server runtime

then the deployment is broken. Fix the deployment architecture before reporting success.

## Verification

After deploy, do not stop at the returned URL.

Verify the live surface directly:

1. fetch the main public URL
2. fetch at least one critical API endpoint if the page depends on one
3. when the product is browser-driven, open the live page in a browser and check for visible runtime failures

For public launch or buyer-facing pages, browser verification is the default, not an optional extra.

## Approval / Claim Flow

If Vercel requires a browser approval, device login confirmation, or claim/access-grant link:

1. identify the exact approval URL or claim URL
2. persist the blocker durably
3. use the owner-communication skill to send the approval request to the owner/CEO with:
   - the exact link
   - what approval is needed
   - what will happen immediately after approval
4. do not keep spinning on blind retries while waiting for approval

If the owner approves and the login/access is confirmed, return immediately to deploy/verify work.

## Output

Show the user the deployment URL. For fallback deployments, also show the claim URL.

Do not report a public deploy as complete until live verification passed.

## Troubleshooting

### Escalated Network Access

If deployment fails due to network issues (timeouts, DNS errors, connection resets), rerun the actual deploy command with escalated permissions (use `sandbox_permissions=require_escalated`). Do not escalate the `command -v vercel` installation check. The deploy requires escalated network access when sandbox networking blocks outbound requests.

Example guidance to the user:

```
The deploy needs escalated network access to deploy to Vercel. I can rerun the command with escalated permissions—want me to proceed?
```
