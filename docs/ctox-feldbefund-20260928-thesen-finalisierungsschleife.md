# Feldbefund 28.09.2026: thesen – Sellify-Vermerk-Tasks in einer Finalisierungsschleife

Gemessen hat die Crew-UX-Sitzung, ausschließlich lesend. Übergeben an den Outbound/Digest-Owner über den Crew-Thread, Codex-Nachricht `01a0e804-7d12-7ff3-aae3-29025aee6306`. Queue und Harness auf thesen sind unverändert.

## Messung (thesen, 12:15–12:36 UTC)

- `ctox status`: running, `busy=false`, `worker_active_count=0` bei den Stichproben, 6 pending.
- 6 Queue-Zeilen (`ctox_business_os__ctox_queue_tasks__v3`) sind betroffen. Sie stehen dauerhaft auf `status=running` und `route_status=leased`, obwohl `execution_phase=terminal` und `terminal_status=failed` gesetzt sind:

| Queue-Task (`queue:system::…`) | attempt | failure_attempt_count |
|---|---|---|
| `26e06c6e3510717ec28eb1cf` | 135 | 3 |
| `13bc98afc431f0e63fe6afcc` | 52 | 1 |
| `94fc8beda5896a278ae247a6` | 47 | 1 |
| `db66a2e70b97da3d30d3671e` | 44 | 3 |
| `dab422507e191c7372e46bfc` | 31 | 0 |
| `caf92ebeef9c5a097d91055d` | 23 | 0 |

- Takt der Neuvergabe: `caf92ebe` wurde um 12:16:20 geleast und um 12:32:49 erneut geleast (Versuch 22 → 23).
- Journal von `ctox.service` über 90 Minuten: 31-mal `ctox prompt worker start source=queue`. Darauf folgten:
  - 22-mal `finalization-error=task execution plan is incomplete (0/1 steps completed)`,
  - 5-mal `(6/7)`,
  - 3-mal `(1/2)`.
- Ein Worker-Lauf dauert 1–1,5 Minuten und ist ein voller Modell-Turn. Ein solcher Task zeigte rund 105.000 gelesene Tokens.

## Ablauf der Schleife

1. `lease_business_queue_capacity` (`service_queue_capacity.rs`) least den Task.
2. `start_prompt_worker` startet den Worker, und der Agent arbeitet.
3. Der Agent markiert den einzigen Planschritt nicht als erledigt. Die Finalisierung verweigert deshalb mit „plan incomplete“.
4. Der Lease bleibt ohne Worker zurück. Der „Orphaned queue lease sweep“ gibt ihn frei, und der Task wird neu geleast. Das geht zurück zu Schritt 1.

Der Finalisierungsfehler erhöht `failure_attempt_count` nicht, und es gibt keine Obergrenze. Die Schleife läuft deshalb unbegrenzt und verbrennt dabei Modell-Kontingent.

## Empfehlung an den Owner

- Eine Obergrenze für Finalisierungsfehler einführen: Nach N Fehlern wird der Task `blocked` mit Grund statt erneut eingereiht.
- Skill oder Prompt der Sellify-Vermerk-Prüfung so anpassen, dass der Planschritt abgeschlossen wird.

## Was die Crew-UX dazu geändert hat

Ab Shell v418 zeigen Crew-App, Crew-Leiste und die Präsenz an den App-Symbolen nur noch Tasks als „arbeitet“, die ein Worker gerade wirklich ausführt. Grundlage ist `ctox_harness_status.active_task_ids`. Die Schleife selbst bleibt beim Owner.
