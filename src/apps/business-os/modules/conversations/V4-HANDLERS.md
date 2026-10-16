# conversations – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 0 eigene Befehlstypen; 4 gesendete Typen gehören outbound (dort portiert). Kein `ctx.cap.legacy`. Tests: –.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `outbound.message.approve` | FACHLOGIK (App outbound) | `store_outbound_commands.rs` | `modules/outbound/handlers/outbound.message.approve.js` (Strang H3) | nicht hier – gehört outbound |
| 2 | `outbound.message.reject` | FACHLOGIK (App outbound) | `store_outbound_commands.rs` | `modules/outbound/handlers/outbound.message.reject.js` (Strang H3) | nicht hier – gehört outbound |
| 3 | `outbound.reply.classify` | FACHLOGIK (App outbound) | `store_outbound_commands.rs` | `modules/outbound/handlers/outbound.reply.classify.js` (Strang H3) | nicht hier – gehört outbound |
| 4 | `outbound.reply.match` | FACHLOGIK (App outbound) | `store_outbound_commands.rs` | `modules/outbound/handlers/outbound.reply.match.js` (Strang H3) | nicht hier – gehört outbound |

conversations hat **keine eigenen** nativ gerouteten Befehlstypen. Ihre vier Sender sind `outbound.*`-Befehle,
deren Eigentümer outbound ist; die Handler existieren bereits unter `modules/outbound/handlers/` (Strang H3). Die
Kommunikationsplattform (`threads.rs`, Kanal-Transport) ist Fähigkeit (`cap.mail.*`, `cap.mail.channel`).
Für v4 nötig: der Handler-Aufruf läuft unter `app_id = outbound`; conversations sendet nur den Befehl
(Befehlsweg über App-Grenzen – zu klären mit „ctox sync v4“, ob `command.app_id` der Sender oder der Eigentümer ist).
