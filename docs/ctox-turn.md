# CTOX TURN — Relay-Architektur und Betrieb

Stand: 2026-07-06
Status: Architekturentscheidung (Backlog OS-B1) + Betriebsanleitung

## Entscheidung

**Der TURN-Relay wird NICHT in den CTOX-Daemon eingebettet. Er läuft als
externes coturn neben der Signaling-Ebene; CTOX mintet nur ephemere
Credentials dafür.**

Begründung:

1. **Erreichbarkeit ist das Kernargument.** Ein Relay muss von beiden Peers
   öffentlich erreichbar sein. TURN wird aber genau dann gebraucht, wenn die
   CTOX-Box **nicht** öffentlich erreichbar ist (symmetrisches NAT, CGNAT,
   strikte Firmen-Firewall). Ein im Daemon eingebetteter Relay auf einer
   NATed Box wäre für exakt den Fall nutzlos, für den TURN existiert. Der
   Signaling-Host ist dagegen per Definition öffentlich — beide Peers
   erreichen ihn bereits per WebSocket.
2. **Idle-Disziplin & Angriffsfläche.** Ein Relay im Daemon würde
   Fremdverkehr durch den Prozess schleusen, der die Geschäftsdaten hält —
   Bandbreite/CPU im Daemon und ein offener UDP-Dienst auf der Datenbox
   widersprechen den Engineering-Prinzipien (`docs/ctox-os-framework-strategy.md`).
3. **Keine neue Dependency.** coturn ist der battle-tested Standard; die
   Rust-Seite braucht nichts Neues (Credential-Minting existiert, siehe
   unten).
4. **Fehlerisolation.** Relay-Ausfall oder -Überlast berührt den Daemon
   nicht; das Relay skaliert unabhängig.

## Was CTOX bereits implementiert

Alles Kryptographische liegt in `src/core/business_os/store.rs`:

- `mint_ephemeral_turn_credentials`: coturn-REST-Schema
  (draft-uberti-behave-turn-rest): `username = "<expiry-unix>:<session>"`,
  `password = base64(HMAC-SHA1(secret, username))`, TTL 3600 s. Die
  Credentials sind pro Peer-Session gebunden und verfallen — ein Leak ist
  nicht dauerhaft replaybar.
- `ephemeral_turn_server(root, session_id)`: baut den `iceServers`-Eintrag;
  `None` ohne Konfiguration → Peers fallen auf STUN zurück.
- `sync_config` hängt den Eintrag automatisch an `ice_servers` an
  (Refresh über `/api/business-os/sync/config`); Browser-Diagnose meldet
  `iceServersHaveCredentialedTurn`.

Konfiguration (Runtime-Store + Secret-Store, kein Env-Toggle):

| Was | Wo |
|---|---|
| TURN-URL | Runtime-Store-Key `CTOX_BUSINESS_OS_TURN_URL` |
| Shared Secret | Secret-Store `business-os/webrtc_turn_secret` |

## Betrieb (Self-Hosting)

1. coturn auf einem öffentlich erreichbaren Host installieren — im
   Normalfall derselbe Host, der den Signaling-Server trägt:

   ```
   # /etc/turnserver.conf (Minimalfall)
   listening-port=3478
   tls-listening-port=5349
   fingerprint
   use-auth-secret
   static-auth-secret=<SHARED_SECRET>
   realm=turn.example.com
   cert=/etc/letsencrypt/live/turn.example.com/fullchain.pem
   pkey=/etc/letsencrypt/live/turn.example.com/privkey.pem
   no-multicast-peers
   denied-peer-ip=10.0.0.0-10.255.255.255
   denied-peer-ip=172.16.0.0-172.31.255.255
   denied-peer-ip=192.168.0.0-192.168.255.255
   denied-peer-ip=127.0.0.0-127.255.255.255
   ```

   Die `denied-peer-ip`-Zeilen verhindern, dass der Relay als Brücke ins
   private Netz des Hosts missbraucht wird.

2. CTOX konfigurieren:

   ```
   ctox business-os turn set --url turns:turn.example.com:5349 --secret <SHARED_SECRET>
   ctox business-os turn status
   ```

3. Verifizieren: Browser-Sync-Diagnose (ctox-Modul) zeigt
   `iceServersHaveCredentialedTurn: true`; ein Peer hinter symmetrischem NAT
   verbindet sich jetzt über den Relay.

## Betrieb (ctox.dev-Fleet) — offener Teil von OS-B1

Für verwaltete Instanzen provisioniert die Fleet coturn neben dem
Signaling-Dienst und setzt pro Instanz URL + Secret bei der Bereitstellung
(Secret in der Neon-Control-DB, Übergabe über den bestehenden
Provisioning-Pfad). Dieser Teil lebt im ctox.dev-Repo, nicht hier.

## Nicht-Ziele

- Kein TURN-Server-Code im CTOX-Daemon (siehe Entscheidung).
- Kein statisches Langzeit-Credential — nur das ephemere REST-Schema.
- Kein neuer Env-Toggle; Konfiguration läuft über Runtime-/Secret-Store
  (die `CTOX_BUSINESS_OS_TURN_*`-Env-Reads sind Migrations-Importe, die in
  den Store persistieren).
