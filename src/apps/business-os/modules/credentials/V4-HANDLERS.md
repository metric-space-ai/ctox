# credentials – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 3 von 3 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 0 PLATTFORM (nur dokumentiert). Tests: 3 grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.secret.put` | PLATTFORM/FAEHIGKEIT (+ Schlüsselprüfung) | `store.rs:17408-17451`, `:15353` | `handlers/ctox.secret.put.js` → `cap.secret.put` | portiert |
| 2 | `ctox.secret.list` | PLATTFORM/FAEHIGKEIT | `store.rs:17385-17407`, `:15190` | `handlers/ctox.secret.list.js` → `cap.secret.list` | portiert |
| 3 | `ctox.secret.delete` | PLATTFORM/FAEHIGKEIT (+ Schlüsselprüfung) | `store.rs:17452-17485`, `:15368` | `handlers/ctox.secret.delete.js` → `cap.secret.delete` | portiert |

E stuft die drei als PLATTFORM ein; der Auftrag ordnet Geheimnisse den generischen Capabilities zu. Umgesetzt als
Delegation: einzige Fachlogik ist `is_valid_credential_key` (UPPER_SNAKE_CASE, ≤ 64, `handlers/_secret.js`). Der Wert
kommt aus dem Secret-Intake des Befehls; der Handler reicht ihn nie weiter und gibt ihn nie zurück. Leerwert-Prüfung,
Verschlüsselung, Schwärzung des Befehls (`secret_command_safe_command`) und Rechte `SecretsManage` bleiben im Kern.
Abweichung: Rust meldete Prüffehler als Befehl `failed` mit `{outcome:{ok:false,error}}`; v4 wirft `invalid`.
Offen (C1 §10): `secret.list` als gepushte, geschwärzte Collection statt Lesebefehl.

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.secret.put` | `({command_id, name, value:{secret_intake:true}}) → void` | `await ctx.cap.secret.put({command_id: command.id, name:'SELLIFY_TOKEN', value:{secret_intake:true}})` | **neu** (E §3 führt keine Secret-Capability) |
| `ctx.cap.secret.list` | `() → {credentials[] (name, description, is_set, updated_at), …}` | `await ctx.cap.secret.list({})` | **neu** |
| `ctx.cap.secret.delete` | `({name}) → void` | `await ctx.cap.secret.delete({name:'SELLIFY_TOKEN'})` | **neu** |
