# Feldbefund 10.09.2026 — native RPC `ctox.outbound.sellify_lookup.v1` antwortet dem Browser nicht in 6 s

Mandant: THESEN (`thesen.ctox.dev`), Release `branch-main-20260910T143924Z`,
Outbound-App 1.0.138. Gemessen über Playwright gegen den Loopback-Dienst
(`ctox business-os serve`, Sitzung „Local CTOX").

## Beobachtung

Die Outbound-App prüft vor jeder Sellify-Übergabe den Sperrvermerk jedes
ausgewählten Kontakts. Dafür ruft sie zuerst den nativen Direktpfad:

```js
state.ctx.sync.requestNative('ctox.outbound.sellify_lookup.v1', payload,
  { timeoutMs: 6_000, collection: 'outbound_lead_generation_leads' })
```

In jeder frischen Browsersitzung läuft dieser Aufruf in den 6-s-Deckel der App
(„Der direkte Sellify-Lookup hat zu lange gedauert"), 4× in 45 s. Danach
sperrt die App den Direktpfad für 5 min und fällt auf den Business-Command
`outbound.sellify.lookup` zurück (`commandBus.dispatch`, `until: 'terminal'`).

## Gegenmessung auf dem Server

`business_command_aggregates`, letzte Stunde:

| command_type | terminal_status | Anzahl | Ø Dauer | max |
|---|---|---:|---:|---:|
| `outbound.sellify.lookup` | completed | 4 | 2,0 s | 3 s |

Der Server beantwortet dieselbe Abfrage also in 2 s. Die Zeit geht zwischen
Browser und Daemon verloren — auf dem nativen RPC-Weg ganz, auf dem
Command-Weg über die Replikation des Ergebnisses.

## Folge für den Nutzer

Die Sperrvermerk-Prüfung eines Leads braucht mehrere Abfragen; sie läuft in
der Sitzung in ihr Zeitbudget („Sperrvermerk-Prüfung steht aus, 59 Kontakte"),
jeder Empfänger bleibt gelb und gesperrt. Ein zeitüberschrittenes Urteil wird
absichtlich nicht gespeichert. Die App speichert erfolgreiche Urteile seit
1.0.129 am Lead (`recipient_eligibility`, 12 h für alle Nutzer) — das lindert,
behebt aber nicht.

## Bitte an die Sync-Seite

1. Warum erreicht `requestNative('ctox.outbound.sellify_lookup.v1')` den
   Daemon nicht bzw. kommt die Antwort nicht zurück? (App-Kommentar vom 31.08.:
   „Der Direktpfad ist auf diesem Tenant tot".) Die Shell erzwingt 20 s je
   Native-Request; auch die kommen nicht an.
2. Wie lange braucht das Ergebnis eines terminalen `outbound.sellify.lookup`
   zurück in den Browser (Command-Record-Replikation)?

Reproduktion: Outbound-App öffnen, Kampagne „Chemie Test 2026", Konsole —
`[outbound-lead-generation] Direkter Sellify-Lookup nicht verfügbar;
Command-Fallback`.
