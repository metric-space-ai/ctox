# Toolhub-Deploy auf welsch.ctox.dev (nur welsch)

Branch `feat/ctox-toolhub-remote-mcp` (PR #689), **nur** auf welsch.ctox.dev,
nie auf SKF, Thesen oder MiltonTicket, kein Merge nach main. Dauer etwa 15
Minuten, davon den größten Teil der Release-Build auf dem Host.

## 1. Auf dem welsch-Host (als CTOX-Benutzer)

```bash
# Vorher: aktuellen Stand für einen Rollback notieren
ctox update status | tee ~/toolhub-before.json        # current_release / previous_release

# Branch holen
git clone --branch feat/ctox-toolhub-remote-mcp --depth 1 \
  https://github.com/metric-space-ai/ctox.git ~/src/ctox-toolhub   # oder: git fetch && git checkout im vorhandenen Klon

# Öffentliche Basis-URL für die OAuth-Rückleitung (wird von systemd geladen)
mkdir -p ~/.config/ctox
grep -q '^CTOX_PUBLIC_BASE_URL=' ~/.config/ctox/business-os.env 2>/dev/null \
  && sed -i 's#^CTOX_PUBLIC_BASE_URL=.*#CTOX_PUBLIC_BASE_URL=https://welsch.ctox.dev#' ~/.config/ctox/business-os.env \
  || echo 'CTOX_PUBLIC_BASE_URL=https://welsch.ctox.dev' >> ~/.config/ctox/business-os.env

# Aus dem Quellbaum bauen und als neues Release aktivieren (Symlink-Switch, Dienste werden neu gestartet)
ctox update apply --source ~/src/ctox-toolhub --release toolhub-689

ctox update status                                     # current_release = toolhub-689
systemctl --user status ctox.service --no-pager | head -5
```

## 2. ctox.dev-Gateway: Rückleitung ohne Login durchlassen

Nur ein Pfad, nur GET, nur welsch. Im Gateway (Vercel-Projekt `ctox-dev`, Team
`metric-spaces-projects`) vor dem Login-Gate für Business OS:

```ts
// Ausnahme: OAuth-Rückleitung der Toolhub-Verbindungen. Der Anbieter leitet
// den Browser ohne ctox.dev-Sitzung hierher; der Code wird durch state + PKCE
// auf der Instanz geschützt, nicht durch das Login.
if (
  host === "welsch.ctox.dev" &&
  request.method === "GET" &&
  url.pathname === "/toolhub/oauth/callback"
) {
  return proxyToInstance(request); // gleicher Upstream wie die übrigen Instanz-Routen
}
```

Danach das Gateway deployen. Prüfen: `curl -sI 'https://welsch.ctox.dev/toolhub/oauth/callback?state=x&code=y'`
muss **200 text/html** (Seite „Verbindung fehlgeschlagen“) liefern, nicht 302 zum Login.

## 3. Smoke-Test (live, keine Mocks)

```bash
./smoke.sh https://welsch.ctox.dev            # Teil A, ohne Login
SESSION_COOKIE='…' ./smoke.sh https://welsch.ctox.dev   # dazu Teil B mit Admin-Sitzung
# Auf dem Host selbst zusätzlich: ./smoke.sh --host
```

## Rollback

```bash
ctox update rollback            # zurück auf previous_release aus ~/toolhub-before.json
# Optional die Env-Zeile wieder entfernen:
sed -i '/^CTOX_PUBLIC_BASE_URL=/d' ~/.config/ctox/business-os.env && systemctl --user restart ctox.service
```

Gateway-Ausnahme: den `if`-Block entfernen und das vorherige Deployment in Vercel
wieder promoten. Gespeicherte Verbindungen bleiben verschlüsselt in der
Secret-Datenbank der Instanz; `ctox toolhub mcp-disconnect --server <x>` entfernt sie.
