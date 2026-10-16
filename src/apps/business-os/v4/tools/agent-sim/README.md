# Automatisierungsknöpfe im Zielbild (Kern §16.30/§16.34) – Kurzanleitung für die App-Umbau-Stränge

Ein Knopf startet einen Agenten mit einem Template-Prompt und einem Skill der eigenen App; das Ergebnis kommt als
Befehl an die eigene App zurück. Kein Typ-Wissen im Core, kein Status-Polling, kein `business_os.chat.task`.

```
Knopf (UI) ─ api.command('<app>.<x>.run', payload)
          └▶ Handler handlers/<app>.<x>.run.js
               1. Template füllen:   renderPrompt(ctx, '<name>', werte)            (prompts/<name>.md)
               2. Task anlegen:      ctx.cap.agent.task.enqueue({ title, instruction, skill: '<app>/<skill>',
                                       payload, idempotency_key, on_complete: '<app>.<x>.complete',
                                       on_failed: '<app>.<x>.failed' })            → {task_id}
               3. eigenen Datensatz schreiben (z. B. {state:'queued', task_id})
          ── Harness arbeitet mit skills/<skill>/SKILL.md aus dem aktiven App-Artefakt ──
          └▶ Core schreibt GENAU EINEN Befehl (höchstens 1× je task_id):
               {type: on_complete|on_failed, payload: {task_id, status, result, error?, workspace_refs?}}
          └▶ Handler handlers/<app>.<x>.complete.js / .failed.js schreibt in die EIGENEN Collections
             (bei Bedarf ctx.cap.agent.task.workspace.read({task_id, path}))
```

Sichtbarer Zustand nur über die eigenen Collections (live bei allen Clients) und den Befehls-/Task-Status in der
Zustandszone. Keine Fehlerzeilen, Zähler oder Status-Pills in der App.

## Paket der App

```
<app>/skills/<skill>/SKILL.md          Skill (Name: [a-z0-9-]+, Ordnername = Name), optional references/…
<app>/prompts/<name>.md                Template mit {{feld}} bzw. {{a.b}}
```

`v4/tools/manifest/gen-manifest.mjs` erzeugt daraus `skills:[{name,path}]` und `prompts:[{name,path}]` (Feld fehlt =
leer) und prüft jeden `skill:'<app>/<name>'` im Handler-Code: nicht deklariert oder fremder Namensraum → `_todo` und
Exit 1. Ordnernamen `test`/`tests` und Dateien mit „test“/„smoke“ im Namen fallen aus `files` und sind verboten.
Globale Namen ohne `/` sind System-Skills des Core (`src/skills/**`); app-spezifische Skills ziehen ins Paket.

## Template füllen (`ctox:shared/prompt-template.js`)

```js
import { renderPrompt } from 'ctox:shared/prompt-template.js';
const instruction = await renderPrompt(ctx, 'issue', { topic: topic.title, sources: JSON.stringify(list) });
```

Liest über `ctx.asset('prompts/issue.md')` (§16.34). Streng: fehlender Wert (undefined/null) → Fehler `invalid` mit
allen Namen (`err.detail.missing`); Objekte/Arrays selbst serialisieren; `\{{` = wörtliches `{{`; kaputte `{{` → Fehler.

## Handler-Test mit der Simulation

```js
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
const sim = createAgentTaskSim({ appId: 'news', appDir, ctxFactory: () => makeCtx() }); // appDir = Paketordner
const ctx = sim.attach(makeCtx(), command);        // setzt ctx.cap.agent.task und ctx.asset
await handler(command, ctx);
const [{ task_id, args, skill }] = sim.enqueued;   // aufgezeichneter enqueue (instruction, skill-Auflösung, …)
const d = await sim.complete(task_id, { text: '…' }, { workspace: { 'out/report.md': '…' } });
// d.command = Callback-Befehl, d.ctx = ctx des Complete-Handlers (darin die Schreibvorgänge prüfen)
await sim.complete(task_id, {});                   // → {delivered:false, reason:'duplicate'}, in sim.log
```

Die Simulation lehnt mit `code:'invalid'` ab: unbekannte Felder (`prompt`, `suggested_skill`, `reply_to`, …), fehlende
`title`/`instruction`/`payload`, Skill nicht im Manifest oder Datei fehlt, fremder Namensraum, unbekannter
System-Skill, `on_complete`/`on_failed` ohne Handler der App. `asset()` liest nur Artefaktdateien (keine Tests,
kein `..`/absolut/Symlink nach außen, ≤ 1 MiB, UTF-8). `foreignTriggerAllowed()` prüft Trigger auf fremde Collections
(Eigentümer exportiert `watch`, App deklariert `watch` in `foreign_collections`). Optionen: `manifest`, `files`
(Paket im Speicher), `handlers` (Typ → Funktion), `coreSkills`.

## Wächter

`node v4/tools/agent-sim/check-skills.mjs` meldet Phantom-Skills, fremde Namensräume und `agent.task.enqueue` ohne
`on_complete`/`on_failed`; Ratchet über `check-skills.baseline.json` (Ziel `[]`). Nach einem Umbau
`--write-baseline` laufen lassen, damit die Baseline schrumpft. Mutationsbeleg: `node v4/tools/agent-sim/mutation-proof.mjs`.


Festlegung (Orchestrator, 15.10.): Callback-Payload-Feld heißt `workspace_refs` (§16.30 Nr. 3, Liste der Pfade). `cancel` löst
`on_failed` mit `status:'cancelled'` aus, damit die App den Abbruch erfährt (beim Kern angefragt).

## System-Skills des Core (ohne Präfix, generisch – geprüft je Strang)

Ein Name ohne `/` ist nur zulässig, wenn der Skill wirklich generisch ist (Fähigkeit, keine App-Logik). Die Simulation
prüft die Existenz über `src/skills/**/SKILL.md`; diese Liste hält fest, WARUM ein Skill im Core bleibt.

| Skill | Ort | Beleg „generisch“ | Nutzer (App: Datei) |
|---|---|---|---|
| `universal-scraping` | `src/skills/system/communication/universal-scraping/` | Methode für Scraping-Workflows (Ziel, Skript, Revision, Lauf) ohne App-Bezug; einziger App-Verweis ist die Ausführungsregel Z. 105 („nur in einem Outbound-Rechercheauftrag … source_policy“), keine App-Daten/-Rückwege | outbound-lead-generation: `handlers/_lib/research-source-adapter.js` (Adapter-Erzeugung), `handlers/outbound.research.adapters.reconcile.js`; news: `news.source.adapter`; outbound: `scrape-generation.js` (Bestandsaufnahme F §2: „bleiben Core“) |
| `web-unlock` | `src/skills/system/communication/web-unlock/` | Reparatur des CTOX-Browser-Stealth-Stacks (`stealth_init.js`, `google_browser_runner.mjs`, `tools/web-stack/src/browser.rs`); 0 Treffer für „outbound“/„lead“ im SKILL.md | outbound-lead-generation: nur als Begleit-Skill in `required_skills` der UI-Payloads (index.js), kein eigener Agentenstart (Strang SKL-C, 15.10.) |

App-spezifisch und deshalb im Paket: `outbound-lead-generation/research` (Quelle `src/skills/system/research/outbound-lead-generation-research/`,
zugeschnitten auf den Callback-Rückweg; die Core-Kopie entfernt der Kern-Abbau-Strang).
