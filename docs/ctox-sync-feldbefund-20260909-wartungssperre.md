# Feldbefund Sync 09.09.2026 — eine langsame Collection hält die ganze Instanz zehn Minuten schreibgeschützt

Kontext: Kundenmandant `thesen.ctox.dev`, vier Upgrades an einem Nachmittag
(`branch-main-20260909T100255Z` → `130855Z` → `141500Z` → `152227Z`), Browser =
Business-OS-Shell im Claude-Browserfenster, Rolle `admin`, 17 Collections im
Profil der Outbound-App.

## Beobachtung

Nach jedem Upgrade blieb die Instanz für ALLE Nutzer schreibgeschützt, bis die
Zehn-Minuten-Kulanz griff. Die App meldete das korrekt:

    CTOX wird aktualisiert. outbound_lead_generation_leads bleibt
    vorübergehend schreibgeschützt.

Die Freigabe hängt an `tryAcknowledgeMaintenanceReadiness`
(`src/apps/business-os/app.js`): der Client bestätigt erst, wenn **alle**
benötigten Collections ihre erste Replikation abgeschlossen haben. Die Anzeige
sagte „Daten werden nach dem Update synchronisiert · 1 ausstehend“.

Hängen blieb genau eine Collection:

    {
      "code": "peer_connect_timeout",
      "collection": "outbound_lead_generation_adapters",
      "phase": "peer-reconnect",
      "severity": "recoverable",
      "timeoutMs": 1000,
      "message": "WebRTC native peer did not open for
                  outbound_lead_generation_adapters within 1000ms;
                  reconnect repair is scheduled."
    }

Der native Peer war dabei gesund: `ctox business-os rxdb status` meldete
`running: true`, `replicationUp: true`, alle Projektionsschleifen unter 250 ms.
Die Collection selbst ist winzig (14 Datensätze in
`ctox_business_os__outbound_lead_generation_adapters__v2`).

Zwei weitere Collections standen dauerhaft auf „ausstehend“, ohne die
Bestätigung zu blockieren: `user_thread_states` (kein Fehler) und
`business_chats` (`pending`).

## Bewertung

Eine Sekunde ist für den ersten Peer-Aufbau nach einem Dienstneustart knapp.
Reicht sie nicht, greift die Reparaturschleife, die Bestätigung bleibt aus, und
die Kulanz von zehn Minuten wird zum Regelfall statt zur Ausnahme. Für den
Kunden heißt das: nach jedem Upgrade zehn Minuten keine Schreibvorgänge, ohne
dass irgendetwas kaputt wäre.

Zusatzmessung zur Einordnung der Umgebung: in einem eingeklappten Browserfenster
brauchten zehn `setInterval(…, 50)`-Ticks **9,6 Sekunden** statt 0,5 (Faktor 16).
Mit sichtbarem Fenster: 0,59 s. Eine 1000-ms-Frist ist unter dieser Drosselung
strukturell nicht zu halten — und ein eingeklapptes Fenster ist beim Kunden ein
Normalzustand, kein Sonderfall.

## Vorschlag

1. Die Frist für den ERSTEN Peer-Aufbau nach einem Dienstneustart deutlich
   erhöhen oder an die gemessene Timer-Auflösung koppeln, statt sie fest auf
   1000 ms zu setzen.
2. Die Wartungsbestätigung nicht an ALLE Collections binden, sondern an die,
   die das offene Modul wirklich braucht — oder eine Collection, deren Peer
   wiederholt scheitert, nach n Versuchen als „nicht blockierend“ führen und
   das sichtbar machen.
3. Die Anzeige sollte benennen, WELCHE Collection aussteht. „1 ausstehend“
   zwingt zum Griff in `window.ctoxBusinessOsSyncDiagnostics`.

Gemessen am 09.09.2026 zwischen 13:38 und 15:55 UTC, viermal reproduziert.
