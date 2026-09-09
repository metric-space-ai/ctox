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

## KORREKTUR der ersten Fassung dieses Befunds

Die erste Fassung las die Meldung "within 1000ms" als zu knappe Frist fuer den
Peer-Aufbau und schlug vor, sie zu erhoehen. **Das war falsch.** Die 1000 ms
sind `NATIVE_PEER_RESTART_STABLE_MS`, also das Stabilitaetsfenster NACH dem
Oeffnen. Die eigentliche Oeffnungsfrist ist
`NATIVE_PEER_RESTART_OPEN_TIMEOUT_MS = 60000` und wurde nie erreicht.

`waitForStableNativePeerOpenState` oeffnet den Peer (60 s Budget), wartet dann
eine Sekunde und prueft erneut. Ist er dann zu — und genau das war der Fall —
wirft es denselben `peer_connect_timeout` mit `timeoutMs: 1000`. Der Peer ist
also **geoeffnet und innerhalb einer Sekunde wieder weggebrochen**, nicht zu
langsam gestartet. Eine hoehere Frist haette die Lage verschlechtert, nicht
verbessert: sie haette dem Flattern nur mehr Gelegenheit gegeben.

## Bewertung

Zwei getrennte Probleme:

1. **Diagnostik (behoben).** Ein Peer, der oeffnet und wieder wegbricht, meldete
   sich mit derselben Kennung und demselben Text wie einer, der nie geoeffnet
   hat. Das kostete einen Nachmittag an der falschen Zahl. Es gibt jetzt
   `peer_unstable_after_open` mit dem Text "opened and closed again within
   {stableMs}ms". Beide Kennungen werden ueberall gleich behandelt, wo bisher
   nur `peer_connect_timeout` stand.
2. **Ursache (offen, gehoert in die Sync-Engine).** Warum bricht der Peer fuer
   `outbound_lead_generation_adapters` unmittelbar nach dem Oeffnen weg,
   waehrend der native Peer gesund ist und 16 andere Collections stabil laufen?
   Die Collection haelt 14 Datensaetze; an der Menge liegt es nicht.

Zur Einordnung der Messumgebung: in einem eingeklappten Browserfenster brauchten
zehn `setInterval(…, 50)`-Ticks **9,6 Sekunden** statt 0,5 (Faktor 16), mit
sichtbarem Fenster 0,59 s. Ein Stabilitaetsfenster von einer Sekunde ist ein
reiner `delay()` und dauert unter dieser Drosselung ein Vielfaches, waehrend der
Peer in der Zwischenzeit regulaer rotieren kann. Das ist der erste Verdacht.

## Vorschlag

1. Ursache des Wegbrechens klaeren, nicht die Frist erhoehen.
2. Die Wartungsbestaetigung nicht an ALLE Collections binden, sondern an die,
   die das offene Modul wirklich braucht — oder eine Collection, deren Peer
   wiederholt recoverable scheitert, nach n Versuchen als "nicht blockierend"
   fuehren und das sichtbar machen.
3. ~~Die Anzeige sollte benennen, WELCHE Collection aussteht.~~ **Erledigt**:
   der Wartungsbanner nennt jetzt die Namen statt "1 ausstehend".

Gemessen am 09.09.2026 zwischen 13:38 und 15:55 UTC, viermal reproduziert.
