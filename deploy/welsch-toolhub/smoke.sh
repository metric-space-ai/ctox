#!/usr/bin/env bash
# Live-Smoke-Test für Toolhub auf einer Instanz. Ohne Mocks.
#   ./smoke.sh https://welsch.ctox.dev     Teil A (öffentlich) [+ Teil B mit SESSION_COOKIE]
#   ./smoke.sh --host                      Teil C, auf dem Instanz-Host (CLI)
set -u
pass=0; fail=0
ok()  { echo "  ok   $*"; pass=$((pass+1)); }
bad() { echo "  FAIL $*"; fail=$((fail+1)); }

if [ "${1:-}" = "--host" ]; then
  echo "C) CLI auf dem Host"
  grep -q '^CTOX_PUBLIC_BASE_URL=https://' ~/.config/ctox/business-os.env 2>/dev/null \
    && ok "CTOX_PUBLIC_BASE_URL gesetzt" || bad "CTOX_PUBLIC_BASE_URL fehlt in ~/.config/ctox/business-os.env"
  ctox toolhub catalog >/dev/null 2>&1 && ok "ctox toolhub catalog" || bad "ctox toolhub catalog"
  out=$(ctox toolhub mcp-connect --server vercel 2>&1)
  echo "$out" | grep -q 'redirect_uri=https%3A%2F%2F' && ok "Vercel-Anmelde-URL mit öffentlicher Rückleitung" \
    || bad "mcp-connect vercel: $out"
  out=$(ctox ui request confirm --toolkit gmail --message smoke --no-wait 2>&1)
  id=$(echo "$out" | sed -n 's/.*"id": *"\([^"]*\)".*/\1/p')
  [ -n "$id" ] && ok "ui request angelegt ($id)" || bad "ui request: $out"
  ctox ui status "$id" | grep -q '"status": *"open"' && ok "ui status open" || bad "ui status"
  out=$(ctox toolhub exec slack.SLACK_LIST_CHANNELS --json '{}' 2>&1); code=$?
  if [ $code -eq 0 ]; then ok "Slack echt aufgerufen (Kanäle gelesen)";
  elif [ $code -eq 3 ]; then echo "$out" | grep -q ui_request && ok "Slack nicht verbunden -> Karte angelegt" || bad "exit 3 ohne Karte";
  else bad "slack exec exit $code: $out"; fi
  for s in vercel cloudflare; do
    out=$(ctox toolhub mcp-tools --server $s 2>&1) && ok "$s: $(echo "$out" | grep -c '"name"') Werkzeuge (echt)" \
      || echo "  --   $s noch nicht verbunden (Michael meldet sich in Workjet an)"
  done
  echo "pass=$pass fail=$fail"; [ $fail -eq 0 ]; exit
fi

base="${1:?Basis-URL, z. B. https://welsch.ctox.dev}"
echo "A) Öffentlich"
code=$(curl -s -D /tmp/cb.hdr -o /tmp/cb.html -w '%{http_code}' "$base/toolhub/oauth/callback?state=smoke&code=smoke")
# Unbekannter state -> die Instanz antwortet 400 mit ihrer Toolhub-Fehlerseite.
# Eine Login-Seite (x-business-os-login-required) heißt: Gateway-Ausnahme fehlt.
if grep -qi '^x-business-os-login-required' /tmp/cb.hdr; then
  bad "Rückleitung landet auf der Login-Seite ($code): Gateway-Ausnahme fehlt (README Abschnitt 2)"
elif { [ "$code" = 400 ] || [ "$code" = 200 ]; } && grep -q 'data-toolhub-result' /tmp/cb.html; then
  ok "Rückleitung erreicht die Instanz ohne Login ($code, Toolhub-Seite)"
else
  bad "Rückleitung liefert $code ohne Toolhub-Seite"
fi
for s in mcp.vercel.com mcp.cloudflare.com; do
  curl -sf "https://$s/.well-known/oauth-authorization-server" | grep -q registration_endpoint \
    && ok "$s bietet dynamische Client-Registrierung" || bad "$s Discovery"
done

if [ -n "${SESSION_COOKIE:-}" ]; then
  echo "B) Mit Admin-Sitzung"
  api() { curl -s -H "cookie: $SESSION_COOKIE" -H 'content-type: application/json' -X POST "$base/api/business-os/toolhub/$1" -d "${2:-{\}}"; }
  api connections | grep -q '"ok":true' && ok "toolhub/connections" || bad "toolhub/connections (alter Stand oder kein Admin)"
  r=$(api connect '{"server":"vercel"}')
  echo "$r" | grep -q "redirect_uri=$(printf %s "$base/toolhub/oauth/callback" | sed 's#:#%3A#; s#/#%2F#g')" \
    && ok "Vercel-Anmelde-URL zeigt auf $base/toolhub/oauth/callback" || bad "connect vercel: $r"
  r=$(api connect '{"server":"cloudflare"}')
  echo "$r" | grep -q authorize_url && ok "Cloudflare-Anmelde-URL ausgegeben" || bad "connect cloudflare: $r"
  api ui/requests | grep -q '"ok":true' && ok "ui/requests" || bad "ui/requests"
fi
echo "pass=$pass fail=$fail"; [ $fail -eq 0 ]
