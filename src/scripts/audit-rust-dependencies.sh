#!/usr/bin/env bash
set -euo pipefail

# cargo-audit evaluates every package recorded in Cargo.lock, including optional
# dependencies whose features are disabled. Keep the exceptions below valid
# only while neither package is reachable in any target's active dependency
# graph.
for package in quick-xml@0.39.4 rsa@0.9.10; do
  if [ -n "$(cargo tree --locked --target all -i "$package" 2>/dev/null)" ]; then
    echo "security exception became reachable: $package" >&2
    exit 1
  fi
done

if [ -n "$(cargo tree --locked --manifest-path src/core/harness/Cargo.toml --target all -i rsa@0.9.10 2>/dev/null)" ]; then
  echo "security exception became reachable in the harness: rsa@0.9.10" >&2
  exit 1
fi

# The vendored rtc-ice manifest retains an upstream ping_pong example on
# hyper 0.14. Production and build dependency edges do not include that
# example-only HTTP stack. Fail closed if h2 ever enters the patch crate's
# normal/build graph; until then, scope the advisory exception to this one
# standalone lockfile instead of weakening the root audit.
rtc_ice_manifest="src/core/rxdb/patches/rtc-ice/Cargo.toml"
rtc_ice_lockfile="src/core/rxdb/patches/rtc-ice/Cargo.lock"
if [ -n "$(cargo tree --locked --manifest-path "$rtc_ice_manifest" --target all --edges normal,build -i h2@0.3.27 2>/dev/null)" ]; then
  echo "security exception became reachable in rtc-ice production edges: h2@0.3.27" >&2
  exit 1
fi

audit_args=(
  --deny unsound
  --ignore RUSTSEC-2026-0002
  --ignore RUSTSEC-2026-0097
  --ignore RUSTSEC-2026-0183
  --ignore RUSTSEC-2026-0184
  --ignore RUSTSEC-2026-0186
  --ignore RUSTSEC-2026-0190
  --ignore RUSTSEC-2026-0221
  --ignore RUSTSEC-2026-0253
)

# The XML and RSA exceptions are root-lockfile-only. Never extend them to a
# standalone component without a separate reachability proof.
cargo audit \
  --file Cargo.lock \
  "${audit_args[@]}" \
  --ignore RUSTSEC-2026-0194 \
  --ignore RUSTSEC-2026-0195 \
  --ignore RUSTSEC-2023-0071

while IFS= read -r lockfile; do
  [ "$lockfile" = "Cargo.lock" ] && continue
  if [ "$lockfile" = "src/core/harness/Cargo.lock" ]; then
    cargo audit \
      --no-fetch \
      --file "$lockfile" \
      "${audit_args[@]}" \
      --ignore RUSTSEC-2023-0071
  elif [ "$lockfile" = "$rtc_ice_lockfile" ]; then
    cargo audit \
      --no-fetch \
      --file "$lockfile" \
      "${audit_args[@]}" \
      --ignore RUSTSEC-2026-0258
  elif [ "$lockfile" = "src/core/transfers/Cargo.lock" ] ||
    [ "$lockfile" = "src/core/transfers/vendor/smb/Cargo.lock" ]; then
    # Known risk, tracked in metric-space-ai/ctox#506: russh's `rsa` feature
    # pulls rsa 0.10.0-rc.18, and SSH/SFTP transfers sign with a
    # user-supplied RSA client key (ssh_exec.rs, storage_ssh.rs); the
    # vendored smb crate pulls the same rsa through picky/sspi (Kerberos).
    # RustSec lists no patched rsa release. Unlike the root and harness
    # exceptions this one IS reachable; it stays scoped to these two
    # lockfiles and must be removed as soon as a fixed rsa release exists.
    cargo audit \
      --no-fetch \
      --file "$lockfile" \
      "${audit_args[@]}" \
      --ignore RUSTSEC-2023-0071
  else
    cargo audit --no-fetch --file "$lockfile" "${audit_args[@]}"
  fi
done < <(git ls-files | awk '/(^|\/)Cargo[.]lock$/' | sort)
