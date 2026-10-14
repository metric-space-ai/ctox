#!/usr/bin/env bash
# Live-Smoke-Test der Toolhub auf einer Instanz. Ohne Mocks, ohne SSH, ohne Gateway-Änderung.
#   ./smoke.sh https://welsch.ctox.dev                      Teil A (anonym)
#   SESSION_COOKIE='…' ./smoke.sh https://welsch.ctox.dev   + Teil B (angemeldeter Admin im Browser)
set -u
pass=0; fail=0
ok()  { echo "  ok   $*"; pass=$((pass+1)); }
bad() { echo "  FAIL $*"; fail=$((fail+1)); }
base="${1:?Basis-URL, z. B. https://welsch.ctox.dev}"
cb="$base/toolhub/oauth/callback?state=smoke-unknown&code=smoke"

echo "A) Anonym"
code=$(curl -s -o /tmp/cb-anon.html -w '%{http_code}' "$cb")
if grep -q 'data-toolhub-result' /tmp/cb-anon.html; then
  bad "anonymer Callback hat die Toolhub erreicht ($code); sie darf ohne Anmeldung nichts verarbeiten"
else
  ok "anonymer Callback verarbeitet nichts (Login-Seite/Shell, $code)"
fi
for s in mcp.vercel.com mcp.cloudflare.com; do
  curl -sf "https://$s/.well-known/oauth-authorization-server" | grep -q registration_endpoint \
    && ok "$s bietet dynamische Client-Registrierung" || bad "$s Discovery"
done

if [ -n "${SESSION_COOKIE:-}" ]; then
  echo "B) Angemeldet (Admin)"
  code=$(curl -s -H "cookie: $SESSION_COOKIE" -o /tmp/cb-auth.html -w '%{http_code}' "$cb")
  if grep -q 'data-toolhub-result="failed"' /tmp/cb-auth.html; then
    ok "Callback erreicht die Instanz über die normale Seitenauslieferung ($code); unbekannter state abgelehnt"
  elif grep -qi 'Business OS' /tmp/cb-auth.html; then
    bad "Callback liefert nur die Shell ($code): Instanz hat den Toolhub-Stand noch nicht (Core anwenden) oder Sitzung ist kein Admin"
  else
    bad "Callback liefert $code ohne Toolhub-Seite"
  fi
fi
echo "pass=$pass fail=$fail"; [ $fail -eq 0 ]
