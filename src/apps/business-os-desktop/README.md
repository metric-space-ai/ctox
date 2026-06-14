# CTOX Business OS Desktop

Electron target for the CTOX Business OS desktop experience.

The product goal is a Slack-like instance switcher that can show ctox.dev
managed instances next to unmanaged local, SSH-managed, and invite-paired
instances. The Business OS data plane remains RxDB/WebRTC-only; Electron may
bootstrap shell URLs and launch context, but it must not add an HTTP data
bridge.

Useful checks:

```sh
npm test
npm run check
npm run test:electron-smoke
npm run smoke:keychain-runtime
npm run smoke:local-runtime
npm run smoke:ctox-dev-live -- --email <email> --password-stdin --expected-tenant <name> --auth-window --manage-first --launch-first
npm run smoke:pairing-ssh-live -- --host <host> --user <user> --password-stdin --trusted-host-key-fingerprint <sha256:...> --rotate --revoke-local
npm run smoke:ssh-password-live -- --host <host> --user <user> --password-stdin --trusted-host-key-fingerprint <sha256:...>
npm run smoke:ssh-password-live -- --host <host> --user <user> --password-stdin --trusted-host-key-fingerprint <sha256:...> --attach
npm run smoke:ssh-password-live -- --host <host> --user <user> --password-stdin --trusted-host-key-fingerprint <sha256:...> --fresh-install
npm run smoke:ssh-password-live -- --host <host> --user <user> --password-stdin --trusted-host-key-fingerprint <sha256:...> --fresh-install --install-api-provider openai
npm run release:check
npm run pack:dir:smoke
```

Release-related checks:

- `npm run release:check` validates the `electron-builder` contract,
  registered desktop protocol, hardened macOS signing/notarization hooks and
  generic HTTPS auto-update feed. It also verifies that the tag release
  workflow contains the Business OS Desktop macOS, Linux and Windows matrix,
  including platform keychain runtime smokes, and that the main CI workflow
  runs the Desktop E2E smoke matrix on macOS, Linux and Windows.
- `npm run smoke:keychain-runtime` writes, reads and deletes a synthetic secret
  through the platform keychain: macOS Keychain, Linux Secret Service or
  Windows Credential Manager.
- `npm run smoke:local-runtime` runs against a real local `ctox` binary,
  installs Business OS into a temporary target, rejects that generated business
  repository as a CTOX runtime root, attaches via `peer ensure` in a fresh
  desktop profile and verifies a WebRTC-only launch without leaking peer
  secrets into the registry.
- `npm run smoke:ctox-dev-live -- --email <email> --password-stdin` is an
  opt-in production/staging smoke for the managed ctox.dev account path. It
  reads the password from stdin, signs in through ctox.dev's password endpoint
  with Electron's default session, checks `/api/desktop/session-package`
  through the same cookie jar, verifies the desktop protocol marker and prints
  only redacted tenant/launch evidence.
  Add `--auth-window` to exercise the real BrowserWindow/AuthPanel login UI
  instead of the password endpoint shortcut. Add one or more
  `--expected-tenant <name>` arguments to pin expected account membership,
  `--manage-first` to load the matching
  `/dashboard?tenant=<tenant-id>` management deep link in the authenticated
  Electron cookie jar, and `--launch-first` to consume a short-lived desktop
  launch token for the first matching managed instance and verify a WebRTC-only
  launch config.
- `npm run smoke:pairing-ssh-live -- --host <host> --user <user> --password-stdin`
  is an opt-in live smoke for invite-paired unmanaged instances. It uses SSH
  only as the remote control channel, pins the host key, reads the SSH password
  from stdin, imports a real remote Desktop invite into a fresh local desktop
  registry, verifies the WebRTC-only launch config, optionally runs remote
  `ctox business-os peer rotate`, rotates the local pairing, and optionally
  revokes it locally without leaking the room secret into registry or evidence.
  Add `--allow-peer-status-invite` only for older remote CTOX versions that do
  not yet expose `ctox business-os desktop invite`; that fallback derives the
  same invite shape from `peer status` and should be treated as weaker evidence
  until the remote CLI is upgraded.
- `npm run smoke:ssh-password-live -- --host <host> --user <user> --password-stdin`
  is an opt-in live infrastructure smoke for password-only VPS access. It reads
  the password from stdin, stores it only in the platform keychain, runs the
  same OpenSSH Askpass preflight path as the app, prints redacted evidence and
  deletes the temporary keychain secret afterwards. Add `--attach` to run the
  full existing-CTOX SSH-managed attach path: remote `peer ensure`, local
  registry registration, WebRTC-only launch config, and a no-secret-leak
  assertion for the registry and evidence. Add `--fresh-install` to run the
  stable online release-bundle path before peer ensure. In the stable path,
  `--install-api-provider <provider>` and `--install-model <model>` seed
  API-backed runtime config into SQLite after the verified bundle install; this
  avoids the source installer on small CPU-only VPS hosts. Use
  `--release-channel dev` when the source installer must be exercised; in that
  path `--install-api-provider <provider>`, `--install-model <model>` and
  `--install-backend <backend>` are passed to `install.sh` as CLI arguments,
  not runtime environment toggles. Add
  `--local-artifact-path <absolute-linux-binary>` with `--fresh-install` to
  exercise the SCP/local-artifact path instead of the online release bundle;
  local artifacts cannot be combined with installer seed flags. Use
  `--trusted-host-key-fingerprint <sha256:...>` for pinned host keys, or
  `--trust-scanned-host-key` only for first-contact test hosts. If a terminal
  harness makes the platform keychain CLI interactive, add
  `--file-askpass-fallback` for in-memory SSH password handling; that still
  proves real SSH password auth and strict host-key checking, but it is weaker
  than the full keychain-backed live path. Combined with `--attach` or
  `--fresh-install`, the fallback runs the same remote command path and verifies
  the WebRTC-only launch shape, but stores live-smoke secrets only in memory.
- `npm run pack:dir:smoke` builds a local unpacked app and verifies the bundle
  metadata plus packaged `app.asar` contents. This is an unsigned local smoke,
  not a production release artifact.
- `npm run smoke:signed-artifacts -- --platform <mac|linux|win>
  --evidence-json <path>` verifies release artifacts after a distribution
  build and writes uploadable JSON evidence with relative artifact paths. macOS
  checks the `.app`, `app.asar`, bundled CTOX helper, `codesign` and Gatekeeper
  assessment; Linux checks AppImage, `.deb`, `linux-unpacked`, `app.asar` and
  helper; Windows checks the NSIS installer, `win-unpacked`, `app.asar` and
  `ctox.exe`.
- `npm run dist` builds platform installers. Production macOS releases require
  signing plus notarization build secrets; unsigned local artifacts are not
  release-ready.
