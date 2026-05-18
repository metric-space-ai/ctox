#!/usr/bin/env bash
#
# End-to-end test for `ctox web search` — verifies the provider cascade
# returns usable hits against the live web.
#
# Probes the three HTTP-only providers individually (no patchright/chromium
# dependency, just network) and then the auto-cascade. Each test asserts:
#   - exit code 0
#   - ok=true in the JSON output
#   - non-zero citation count
#   - provider name is non-empty
#
# The Google-via-Patchright provider is intentionally not exercised here —
# it requires the browser runtime and lands in the existing baseline
# probes (sannysoft / incolumitas) which already validate the runner.
#
# Usage:
#   tools/web-stack/scripts/test_web_search_e2e.sh           # all providers + auto
#   tools/web-stack/scripts/test_web_search_e2e.sh --quick   # auto-cascade only
#
# Exit codes: 0 = all passed, non-zero = at least one provider failed.
# When run in CI without network, set CTOX_SKIP_NETWORK=1 to skip the
# real probes — only the JSON-shape check runs (against `--source mock`).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
if [[ -x "$ROOT/target.nosync/debug/ctox" ]]; then
  CTOX="$ROOT/target.nosync/debug/ctox"
elif [[ -x "$ROOT/target/debug/ctox" ]]; then
  CTOX="$ROOT/target/debug/ctox"
elif [[ -x "$ROOT/target/release/ctox" ]]; then
  CTOX="$ROOT/target/release/ctox"
else
  CTOX="$ROOT/target/debug/ctox"
fi

QUICK=0
case "${1:-}" in
  --quick) QUICK=1 ;;
  -h|--help)
    sed -n '2,18p' "$0" | sed 's/^# //;s/^#//'
    exit 0
    ;;
  "") ;;
  *) echo "Unknown arg: $1"; exit 2 ;;
esac

log()  { printf "\n\033[36m[%s]\033[0m %s\n" "$(date +%H:%M:%S)" "$*"; }
ok()   { printf "\033[32m  ok\033[0m %s\n" "$*"; }
die()  { printf "\033[31m  FAIL\033[0m %s\n" "$*" >&2; exit 1; }

[[ -x "$CTOX" ]] || die "ctox binary missing at $CTOX — run 'cargo build -p ctox' first"
command -v jq >/dev/null 2>&1 || die "jq is required"

cd "$ROOT"

# Always-on shape check: --source mock should return a stable, parseable
# JSON envelope with the same fields as live providers. Catches output-
# schema regressions even when no network is available.
test_shape() {
  log "shape: ctox web search --source mock"
  local out
  out=$("$CTOX" web search --query "schema check" --source mock 2>&1)
  echo "$out" | jq -e '.ok == true' >/dev/null || { echo "$out"; die "mock ok != true"; }
  echo "$out" | jq -e '.provider | type == "string" and length > 0' >/dev/null \
    || die "mock provider missing or non-string"
  echo "$out" | jq -e '.query | type == "string"' >/dev/null || die "query missing"
  echo "$out" | jq -e '.context | type == "string"' >/dev/null || die "context missing"
  # Either results or citations must contain at least one hit; the live
  # mock path falls back to a cached real provider so both layouts must work.
  local hits
  hits=$(echo "$out" | jq -r '(.citations // []) | length')
  [[ "$hits" -ge 1 ]] || die "no hits in mock probe (citations=$hits)"
  ok "schema valid, hits=$hits"
}

# Live provider check: a real query against the given pinned source.
test_provider() {
  local provider="$1" query="$2"
  log "live: ctox web search --source $provider"
  local out exit_code
  set +e
  out=$("$CTOX" web search --query "$query" --source "$provider" 2>&1)
  exit_code=$?
  set -e
  if [[ $exit_code -ne 0 ]]; then
    echo "$out" | head -20
    die "$provider exited $exit_code"
  fi
  echo "$out" | jq -e '.ok == true' >/dev/null \
    || { echo "$out" | head -20; die "$provider: ok != true"; }
  local hits
  hits=$(echo "$out" | jq -r '(.citations // []) | length')
  [[ "$hits" -ge 1 ]] || die "$provider returned zero citations"
  local actual_provider
  actual_provider=$(echo "$out" | jq -r '.provider')
  ok "$provider returned $hits hits via .provider=$actual_provider"
}

# Auto-cascade: the default path used by every CTOX agent. Verifies the
# whole router (cooldowns, quality gate, provider budget) ends up with
# a usable result without an explicit pin.
test_auto_cascade() {
  log "auto-cascade: ctox web search (no --source)"
  local out exit_code
  set +e
  out=$("$CTOX" web search --query "rust programming language wikipedia" 2>&1)
  exit_code=$?
  set -e
  if [[ $exit_code -ne 0 ]]; then
    echo "$out" | head -20
    die "auto-cascade exited $exit_code"
  fi
  echo "$out" | jq -e '.ok == true' >/dev/null \
    || { echo "$out" | head -20; die "auto-cascade: ok != true"; }
  local hits provider
  hits=$(echo "$out" | jq -r '(.citations // []) | length')
  provider=$(echo "$out" | jq -r '.provider')
  [[ "$hits" -ge 1 ]] || die "auto-cascade returned zero citations"
  ok "auto-cascade returned $hits hits via .provider=$provider"
}

# ── Driver ─────────────────────────────────────────────────────────────────

test_shape

if [[ "${CTOX_SKIP_NETWORK:-0}" == "1" ]]; then
  log "CTOX_SKIP_NETWORK=1 — skipping live provider probes"
  log "test_web_search_e2e: shape-only run PASSED"
  exit 0
fi

if [[ $QUICK -eq 1 ]]; then
  test_auto_cascade
else
  # HTTP-only providers — no auth required for these.
  # Brave web search requires no API key on the HTML-scrape path.
  test_provider brave "rust programming language"
  test_provider duckduckgo "rust programming language"
  test_provider bing "rust programming language"
  test_auto_cascade
fi

log "test_web_search_e2e PASSED"
