# Toolhub-Deploy auf welsch.ctox.dev (nur welsch)

Branch `feat/toolhub-activepieces-port` (PR #708). Nur auf welsch.ctox.dev, nie auf
SKF, Thesen oder MiltonTicket, kein Merge nach main. Zwei Teile: (1) die Instanz
bekommt den neuen Core, (2) das ctox.dev-Gateway lässt die OAuth-Rückleitung ohne
Login durch. Ohne Teil 2 liefert `GET /toolhub/oauth/callback` die Login-Seite
(HTTP 200, Header `x-business-os-login-required: true`). Das ist der heutige Stand.

## 1. Core auf der Instanz

„Core anwenden“ in der Oberfläche spielt nur Release-Tags ein (`--latest` / `--version`),
keinen Branch. Für #708 deshalb über die CLI auf dem welsch-Host (als CTOX-Benutzer, per SSH):

```bash
ctox update status | tee ~/toolhub-before.json        # current_release / previous_release für den Rollback

git clone --branch feat/toolhub-activepieces-port --depth 1 \
  https://github.com/metric-space-ai/ctox.git ~/src/ctox-708
# vorhandener Klon: git -C ~/src/ctox-708 fetch origin feat/toolhub-activepieces-port && git -C ~/src/ctox-708 checkout -B feat/toolhub-activepieces-port FETCH_HEAD

# Öffentliche Basis-URL für die OAuth-Rückleitung (systemd lädt die Datei)
mkdir -p ~/.config/ctox
grep -q '^CTOX_PUBLIC_BASE_URL=' ~/.config/ctox/business-os.env 2>/dev/null \
  && sed -i 's#^CTOX_PUBLIC_BASE_URL=.*#CTOX_PUBLIC_BASE_URL=https://welsch.ctox.dev#' ~/.config/ctox/business-os.env \
  || echo 'CTOX_PUBLIC_BASE_URL=https://welsch.ctox.dev' >> ~/.config/ctox/business-os.env

# Quell-Release bauen und aktivieren. apply --source ruft install.sh auf; das baut
# auch das Pi-Sidecar-Bundle (npm ci + npm run build in src/core/coding_agents/pi-sidecar),
# also braucht der Host Node >= 20 und npm. Danach Symlink-Switch und Neustart der Dienste.
ctox update apply --source ~/src/ctox-708 --release toolhub-708

ctox update status                                     # current_release = toolhub-708
systemctl --user status ctox.service --no-pager | head -5
```

Dauer: vor allem der Release-Build (auf einem 8-Kern-Host ~30–40 min, ~16 GB RAM
inkl. Swap; vorher `free -g` prüfen, andere Builds anhalten).

**Ohne Build auf dem Host** (wenn der Host zu klein ist): ein Release-Tag auf dem
#708-Stand setzen, dann baut der bestehende `release.yml`-Workflow das Binär-Bundle,
und „Core anwenden“ bzw. `ctox update apply --version <tag>` spielt es ein. Ein Tag
veröffentlicht ein Release und braucht deshalb Michaels Freigabe. Ein lokal
gebautes Binary lässt sich nicht direkt einspielen: `apply --source` baut immer aus
den Quellen, Binär-Bundles kommen nur aus Releases.

## 2. ctox.dev-Gateway: Rückleitung ohne Login

Repo `mkh-welsch/ctox-dev` (Vercel-Projekt `ctox-dev`, Team `metric-spaces-projects`,
Produktionszweig `main`). Datei:

`app/instance/[tenant]/[[...path]]/route.ts`, Funktion `handler`, direkt **nach** dem
Block `if (request.method !== "GET" && request.method !== "HEAD") { … 405 … }` und
**vor** `if (isBusinessOsStaticAssetPath(relativePath))` einfügen:

```ts
  // Toolhub-OAuth-Rückleitung: öffentlich, weil der Anbieter den Browser ohne
  // ctox.dev-Sitzung hierher schickt. Geschützt durch das einmalige state +
  // PKCE auf der Instanz, nicht durch das Login. Nur GET, nur dieser Pfad.
  if (request.method === "GET" && relativePath === "toolhub/oauth/callback") {
    try {
      const upstream = await requestTenantBusinessOsAsset(row.id, relativePath, {
        method: "GET",
        search: request.nextUrl.search,
        timeoutMs: 30000,
      });
      return tenantShellProxyResponse(request, row, upstream);
    } catch {
      return new NextResponse("Toolhub-Rückleitung: Instanz nicht erreichbar", {
        status: 502,
        headers: { "cache-control": "no-store", "content-type": "text/plain; charset=utf-8" },
      });
    }
  }
```

Warum das reicht: `requestTenantBusinessOsAsset` (`lib/ssh.ts`) leitet den Pfad samt
Query über den bestehenden SSH-Tunnel an den nativen Instanzserver weiter.
`normalizedBusinessOsAssetPath` lässt `toolhub/oauth/callback` durch, sperrt aber
`api/*` und `login*`. `tenantShellProxyResponse` übernimmt Status (auch 400 bei
ungültigem state) und die CSP der Instanz. Keine Session, kein Cookie, keine weiteren Pfade.
Beide Funktionen gibt es in der Datei schon (Import bzw. lokal); es braucht keinen neuen Import.

Danach `main` deployen (Vercel baut automatisch). Prüfen:

```bash
curl -s -D- -o /dev/null 'https://welsch.ctox.dev/toolhub/oauth/callback?state=x&code=y' | grep -iE '^HTTP|x-business-os'
# erwartet: HTTP/2 400 (Seite „Verbindung fehlgeschlagen“, unbekannter state), KEIN x-business-os-login-required
```

## 3. Smoke-Test (live, keine Mocks)

```bash
./smoke.sh https://welsch.ctox.dev            # Teil A, ohne Login
SESSION_COOKIE='…' ./smoke.sh https://welsch.ctox.dev   # dazu Teil B mit Admin-Sitzung
./smoke.sh --host                              # Teil C, auf dem Host selbst
```

## Rollback

```bash
ctox update rollback            # zurück auf previous_release aus ~/toolhub-before.json
sed -i '/^CTOX_PUBLIC_BASE_URL=/d' ~/.config/ctox/business-os.env && systemctl --user restart ctox.service   # optional
```

Gateway: den `if`-Block entfernen oder in Vercel das vorherige Deployment wieder
promoten. Gespeicherte Verbindungen bleiben verschlüsselt in der Secret-Datenbank
der Instanz; `ctox toolhub mcp-disconnect --server <x>` entfernt sie.
