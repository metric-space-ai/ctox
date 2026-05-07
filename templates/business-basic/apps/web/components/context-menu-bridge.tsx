"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { usePathname } from "next/navigation";

type ContextItem = {
  action?: string;
  currentUrl: string;
  label: string;
  moduleId: string;
  recordId: string;
  recordType: string;
  selectedText?: string;
  submoduleId: string;
};

type MenuState = {
  href?: string;
  item: ContextItem;
  actions: ContextMenuAction[];
  x: number;
  y: number;
};

type ContextMenuAction = {
  description: string;
  id: string;
  label: string;
  prompt: (item: ContextItem) => string;
};

type QueueResponse = {
  ok?: boolean;
  core?: { mode?: string; taskId?: string | null };
  error?: string;
};

const copy = {
  de: {
    close: "Schliessen",
    context: "Kontext",
    defaultPrompt: (item: ContextItem) => {
      const selected = item.selectedText ? `\nAuswahl: ${item.selectedText}` : "";
      return `Bearbeite diesen Kontext in CTOX: ${item.label}. Modul: ${item.moduleId}/${item.submoduleId}. Datensatz: ${item.recordType} ${item.recordId}.${selected}`;
    },
    open: "Details oeffnen",
    prompt: "CTOX Prompt",
    done: "OK",
    failed: "Aktion fehlgeschlagen",
    submit: "An CTOX uebergeben"
  },
  en: {
    close: "Close",
    context: "Context",
    defaultPrompt: (item: ContextItem) => {
      const selected = item.selectedText ? `\nSelection: ${item.selectedText}` : "";
      return `Work on this context in CTOX: ${item.label}. Module: ${item.moduleId}/${item.submoduleId}. Record: ${item.recordType} ${item.recordId}.${selected}`;
    },
    open: "Open details",
    prompt: "CTOX Prompt",
    done: "OK",
    failed: "Action failed",
    submit: "Send to CTOX"
  }
};

export function ContextMenuBridge({ locale }: { locale?: string }) {
  const pathname = usePathname();
  const activeLocale = locale === "de" ? "de" : "en";
  const activeLocaleRef = useRef<"de" | "en">(activeLocale);
  const t = copy[activeLocale];
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [prompt, setPrompt] = useState<{ item: ContextItem; text: string } | null>(null);
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  useEffect(() => {
    activeLocaleRef.current = activeLocale;
  }, [activeLocale]);

  useEffect(() => {
    const close = () => setMenu(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenu(null);
        setPrompt(null);
      }
    };
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  useEffect(() => {
    setMenu(null);
    setPrompt(null);
  }, [pathname]);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      const source = event.target instanceof Element ? event.target : null;
      if (!source || source.closest(".ops-svar-gantt-shell, .context-menu, .ctox-prompt-panel")) return;
      if (source.closest("input, textarea, select, [contenteditable='true']")) return;

      const target =
        source.closest<HTMLElement>("[data-context-item]") ??
        source.closest<HTMLElement>("[data-context-scope]") ??
        source.closest<HTMLElement>("[data-context-module][data-context-submodule]");
      if (!target) return;

      const item = contextItemFromElement(target);
      if (!item) return;

      event.preventDefault();
      event.stopPropagation();
      const actions = contextMenuActions(item, activeLocaleRef.current);
      setPrompt(null);
      setMenu({
        actions,
        href: target instanceof HTMLAnchorElement ? target.href : target.closest<HTMLAnchorElement>("a")?.href,
        item,
        ...fitMenuPosition(event.clientX, event.clientY, actions.length)
      });
    };

    window.addEventListener("contextmenu", onContextMenu, true);
    return () => window.removeEventListener("contextmenu", onContextMenu, true);
  }, []);

  const contextTokens = useMemo(() => {
    if (!prompt) return [];
    return [
      prompt.item.moduleId,
      prompt.item.submoduleId,
      prompt.item.recordType,
      prompt.item.recordId
    ].filter(Boolean);
  }, [prompt]);
  const menuActions = menu?.actions ?? [];

  return (
    <>
      {menu ? (
        <div className="context-menu" role="menu" style={{ left: menu.x, top: menu.y }}>
          <button
            className="context-menu-primary"
            onClick={(event) => {
              event.stopPropagation();
              setMenu(null);
              setStatus("idle");
              setMessage("");
              setPrompt({ item: menu.item, text: t.defaultPrompt(menu.item) });
            }}
            role="menuitem"
            type="button"
          >
            <span>{t.prompt}</span>
            <small>{menu.item.label}</small>
          </button>
          {menu.href ? (
            <button
              onClick={(event) => {
                event.stopPropagation();
                setMenu(null);
                window.dispatchEvent(new CustomEvent("ctox:navigate", { detail: { href: menu.href } }));
              }}
              role="menuitem"
              type="button"
            >
              <span>{t.open}</span>
              <small>{menu.item.recordType}</small>
            </button>
          ) : null}
          {menuActions.map((action) => (
            <button
              key={action.id}
              onClick={(event) => {
                event.stopPropagation();
                setMenu(null);
                setStatus("idle");
                setMessage("");
                setPrompt({ item: menu.item, text: action.prompt(menu.item) });
              }}
              role="menuitem"
              type="button"
            >
              <span>{action.label}</span>
              <small>{action.description}</small>
            </button>
          ))}
        </div>
      ) : null}

      {prompt ? (
        <section className="ctox-prompt-panel" aria-label={t.prompt}>
          <header>
            <div>
              <strong>{t.prompt}</strong>
              <span>{prompt.item.label}</span>
            </div>
            <button onClick={() => setPrompt(null)} type="button">{t.close}</button>
          </header>
          <div className="ctox-prompt-context" aria-label={t.context}>
            {contextTokens.map((token) => <span key={token}>{token}</span>)}
          </div>
          <textarea
            onChange={(event) => setPrompt((current) => current ? { ...current, text: event.target.value } : current)}
            value={prompt.text}
          />
          <footer>
            <button
              disabled={status === "submitting" || prompt.text.trim().length === 0}
              onClick={async () => {
                setStatus("submitting");
                setMessage("");
                const result = await postQueue({
                  instruction: prompt.text,
                  context: {
                    source: "right-click-context",
                    currentUrl: prompt.item.currentUrl,
                    items: [prompt.item]
                  }
                });
                if (result.ok) {
                  setStatus("queued");
                  setMessage(t.done);
                } else {
                  setStatus("error");
                  setMessage(result.error ?? t.failed);
                }
              }}
              type="button"
            >
              {status === "submitting" ? "..." : t.submit}
            </button>
            {message ? <span>{message}</span> : null}
          </footer>
        </section>
      ) : null}
    </>
  );
}

function contextItemFromElement(element: HTMLElement): ContextItem | null {
  const dataset = element.dataset;
  const moduleId = dataset.contextModule;
  const submoduleId = dataset.contextSubmodule;
  const recordType = dataset.contextRecordType ?? "workspace";
  const recordId = dataset.contextRecordId ?? `${moduleId}-${submoduleId}`;
  const label = dataset.contextLabel ?? element.getAttribute("aria-label") ?? element.textContent?.trim() ?? `${moduleId}/${submoduleId}`;
  if (!moduleId || !submoduleId || !recordType || !recordId || !label) return null;

  const selectedText = window.getSelection()?.toString().trim();
  return {
    action: dataset.contextAction,
    currentUrl: window.location.href,
    label,
    moduleId,
    recordId,
    recordType,
    selectedText: selectedText || undefined,
    submoduleId
  };
}

function contextMenuActions(item: ContextItem, locale: "de" | "en"): ContextMenuAction[] {
  const de = locale === "de";
  const actions: ContextMenuAction[] = [];
  const add = (id: string, label: string, description: string, prompt: (item: ContextItem) => string) => {
    actions.push({ id, label, description, prompt });
  };

  if (item.action === "create") {
    add(
      "create",
      de ? "Neu anlegen" : "Create new",
      item.recordType,
      (ctx) => de
        ? `Lege ${ctx.label} im Modul ${ctx.moduleId}/${ctx.submoduleId} an. Nutze den aktuellen Kontext und erstelle sinnvolle Pflichtfelder, naechste Schritte und Verknuepfungen.`
        : `Create ${ctx.label} in ${ctx.moduleId}/${ctx.submoduleId}. Use the current context and prepare required fields, next steps, and links.`
    );
  }

  if (item.moduleId === "sales") {
    if (item.submoduleId === "campaigns") {
      add("campaign-research", de ? "Research starten" : "Start research", de ? "Quelle, Firma, Kontakt, Touchpoint" : "Source, company, contact, touchpoint", (ctx) => de
        ? `Arbeite an dieser Kampagne: ${ctx.label}. Pruefe Importquelle, Firmenstammdaten, Ansprechpartner, Touchpoints, Ansprache und Versandbereitschaft. Gib klare Luecken und naechste Aktionen zurueck.`
        : `Work on this campaign: ${ctx.label}. Check source import, company data, contacts, touchpoints, outreach, and send readiness. Return gaps and next actions.`);
      add("campaign-send", de ? "Versand pruefen" : "Check sending", de ? "Mailkonto, Freigabe, Rate-Limits" : "Account, approval, rate limits", (ctx) => de
        ? `Pruefe den Versandkontext fuer ${ctx.label}. Beruecksichtige Mailkonto, Freigabe, Tageslimit, Ruhezeiten, Bounces, Abmeldungen und ob alle Anschreiben wirklich versandbereit sind.`
        : `Check the sending context for ${ctx.label}. Consider mail account, approval, daily cap, quiet hours, bounces, unsubscribes, and whether all messages are ready.`);
    } else if (item.submoduleId === "pipeline") {
      add("pipeline-transition", de ? "Transition pruefen" : "Check transition", de ? "Gate, Blocker, naechste Stufe" : "Gate, blockers, next stage", (ctx) => de
        ? `Pruefe diesen Pipeline-Kontext: ${ctx.label}. Sind Gate-Voraussetzungen, Blocker, naechste Aktion und Uebergabe in die naechste Stufe klar?`
        : `Check this pipeline context: ${ctx.label}. Are gate requirements, blockers, next action, and handoff to the next stage clear?`);
    } else if (item.submoduleId === "leads") {
      add("lead-plan", de ? "Plan-Schritt vorschlagen" : "Suggest plan step", de ? "Flow, Folgeaktion, Automatisierung" : "Flow, follow-up, automation", (ctx) => de
        ? `Schlage fuer diesen Lead-Flow den naechsten sinnvollen Plan-Schritt vor: ${ctx.label}. Unterscheide erledigte Evidenz von geplanten Aktionen und nenne Trigger fuer die Ausfuehrung.`
        : `Suggest the next useful plan step for this lead flow: ${ctx.label}. Distinguish completed evidence from planned actions and name execution triggers.`);
    } else if (item.submoduleId === "offers") {
      add("offer-review", de ? "Angebot pruefen" : "Review offer", de ? "Positionen, Steuer, Freigabe" : "Lines, tax, approval", (ctx) => de
        ? `Pruefe das Angebot ${ctx.label}: Positionen, Summen, Steuer, Gueltigkeit, Kundentext und Uebergabe nach Annahme.`
        : `Review offer ${ctx.label}: lines, totals, tax, validity, customer text, and handoff after acceptance.`);
    } else if (item.submoduleId === "customers") {
      add("customer-dossier", de ? "Dossier aktualisieren" : "Update dossier", de ? "Buying Center, Akte, Risiken" : "Buying center, file, risks", (ctx) => de
        ? `Aktualisiere das Kundendossier fuer ${ctx.label}. Ergaenze Stammdaten, Buying Center, offene Risiken, Operations-Kontext und naechste sinnvolle Pflegeaktion.`
        : `Update the customer dossier for ${ctx.label}. Add master data, buying center, open risks, operations context, and next maintenance action.`);
    }
  }

  if (item.moduleId === "marketing") {
    add("marketing-research", de ? "Research ableiten" : "Derive research", de ? "Signale, Quellen, Content" : "Signals, sources, content", (ctx) => de
      ? `Leite aus diesem Marketing-Kontext ${ctx.label} sinnvolle Research-, Content- und Kampagnenaktionen ab. Nenne Quellen, Risiken und verwertbare Assets.`
      : `Derive useful research, content, and campaign actions from this marketing context ${ctx.label}. Name sources, risks, and usable assets.`);
  }

  if (item.moduleId === "operations") {
    if (item.submoduleId === "projects") {
      add("project-plan", de ? "Projektplan pruefen" : "Check project plan", de ? "Gantt, Abhaengigkeiten, Risiken" : "Gantt, dependencies, risks", (ctx) => de
        ? `Pruefe den Projektkontext ${ctx.label}. Bewerte Gantt-Zeitraum, Abhaengigkeiten, Risiken, Blocker und naechste Work Items.`
        : `Check project context ${ctx.label}. Assess Gantt timeline, dependencies, risks, blockers, and next work items.`);
    } else if (item.submoduleId === "work-items") {
      add("work-status", de ? "Work Item klaeren" : "Clarify work item", de ? "Status, Owner, Blocker" : "Status, owner, blocker", (ctx) => de
        ? `Klaere dieses Work Item ${ctx.label}: aktueller Status, Owner, Blocker, Faelligkeit und konkreter naechster Schritt.`
        : `Clarify this work item ${ctx.label}: current status, owner, blocker, due date, and concrete next step.`);
    } else {
      add("operations-context", de ? "Operations-Kontext" : "Operations context", de ? "Strukturieren und verknuepfen" : "Structure and link", (ctx) => de
        ? `Strukturiere diesen Operations-Kontext ${ctx.label}. Verknuepfe relevante Projekte, Work Items, Wissen und Meetings.`
        : `Structure this operations context ${ctx.label}. Link relevant projects, work items, knowledge, and meetings.`);
    }
  }

  if (item.moduleId === "business") {
    if (item.submoduleId === "warehouse") {
      if (item.recordType === "warehouse_order") {
        add("warehouse-order-readiness", de ? "Auftrag pruefen" : "Check order", de ? "Material, Wertschritte, Versandgate" : "Material, value steps, shipping gate", (ctx) => de
          ? `Pruefe diesen Warehouse-Auftrag ${ctx.label}. Bewerte Materialdeckung, fehlende Einzelpositionen, Wertschritte, QA, Packstatus und ob Versand wirklich freigegeben werden darf.`
          : `Check this warehouse order ${ctx.label}. Assess material coverage, missing order lines, value steps, QA, packing status, and whether shipping can be released.`);
        add("warehouse-order-blockers", de ? "Blocker planen" : "Plan blockers", de ? "Fehlteile und Fertigung" : "Missing parts and production", (ctx) => de
          ? `Plane die Blocker fuer Auftrag ${ctx.label}. Leite konkrete Aktionen fuer fehlende Teile, Umlagerung, Auftragsfertigung, QA und Packfreigabe ab.`
          : `Plan blockers for order ${ctx.label}. Derive concrete actions for missing parts, transfers, order production, QA, and packing approval.`);
      } else if (item.recordType === "warehouse_order_line") {
        add("warehouse-line-source", de ? "Position matchen" : "Match line", de ? "Bestand, Fehlteil, Quelle" : "Stock, shortage, source", (ctx) => de
          ? `Matche diese Auftragsposition ${ctx.label}. Pruefe verfuegbaren Bestand, fehlende Menge, beste Lagerquelle, Umlagerung und Fertigungsbedarf.`
          : `Match this order line ${ctx.label}. Check available stock, missing quantity, best warehouse source, transfer, and production need.`);
        add("warehouse-line-escalate", de ? "Fehlteil klaeren" : "Resolve shortage", de ? "Einkauf, Fertigung, Ersatz" : "Purchase, production, substitute", (ctx) => de
          ? `Klaere Fehlteile fuer ${ctx.label}. Nenne Einkaufs-, Fertigungs-, Ersatzteil- oder Teillieferungsoptionen mit naechstem Schritt.`
          : `Resolve shortages for ${ctx.label}. Name purchasing, production, substitute, or partial delivery options with the next step.`);
      } else if (item.recordType === "warehouse_work_step") {
        add("warehouse-step-gate", de ? "Gate pruefen" : "Check gate", de ? "Owner, Status, Freigabe" : "Owner, status, approval", (ctx) => de
          ? `Pruefe den Wertschritt ${ctx.label}. Klaere Owner, Status, Voraussetzungen, Nachweis und Freigabe fuer den naechsten Schritt.`
          : `Check value step ${ctx.label}. Clarify owner, status, prerequisites, evidence, and approval for the next step.`);
      } else if (item.recordType === "warehouse_slot") {
        add("warehouse-slot-edit", de ? "Slot bearbeiten" : "Edit slot", de ? "Name, Status, Bestand" : "Name, status, stock", (ctx) => de
          ? `Bearbeite Lagerplatz ${ctx.label}. Pruefe Name, Zone, Pickbarkeit, Sperrgrund, Bestand, Reservierungen und Inventurstatus. Formuliere eine konkrete bestaetigungspflichtige Lageraktion.`
          : `Edit warehouse slot ${ctx.label}. Check name, zone, pickability, block reason, stock, reservations, and count state. Phrase a concrete warehouse action requiring confirmation.`);
        add("warehouse-slot-rename", de ? "Umbenennen" : "Rename", de ? "Slot-Code und Lage" : "Slot code and location", (ctx) => de
          ? `Benenne Lagerplatz ${ctx.label} sinnvoll um. Leite einen konsistenten Slot-Code aus Lager, Zone und Lage ab und nenne Konflikte vor der Aenderung.`
          : `Rename warehouse slot ${ctx.label}. Derive a consistent slot code from warehouse, zone, and physical position and list conflicts before the change.`);
        add("warehouse-slot-duplicate", de ? "Duplizieren" : "Duplicate", de ? "Gleiche Struktur" : "Same structure", (ctx) => de
          ? `Dupliziere Lagerplatz ${ctx.label} als neuen Slot. Uebernehme sinnvolle Slot-Eigenschaften, aber keinen Bestand, keine Reservierungen und keine offenen Inventurfaelle.`
          : `Duplicate warehouse slot ${ctx.label} as a new slot. Keep useful slot properties but no stock, reservations, or open count cases.`);
        add("warehouse-slot-block", de ? "Sperren/Entsperren" : "Block/unblock", de ? "Operativer Status" : "Operational status", (ctx) => de
          ? `Pruefe, ob Lagerplatz ${ctx.label} gesperrt oder entsperrt werden soll. Beruecksichtige Bestand, offene Picks, Reservierungen, Inventur und Sperrgrund.`
          : `Check whether warehouse slot ${ctx.label} should be blocked or unblocked. Consider stock, open picks, reservations, count state, and block reason.`);
      } else if (item.recordType === "stock_balance" || item.recordType === "warehouse_source_match") {
        add("warehouse-stock-move", de ? "Bestand verschieben" : "Move stock", de ? "Quelle, Ziel, Menge" : "Source, target, quantity", (ctx) => de
          ? `Bereite eine Bestandsverschiebung fuer ${ctx.label} vor. Klaere Quelle, Zielslot, Menge, Owner, Status, offene Reservierungen und ob die Bewegung regelkonform ist.`
          : `Prepare a stock move for ${ctx.label}. Clarify source, target slot, quantity, owner, status, open reservations, and whether the movement is valid.`);
        add("warehouse-stock-count", de ? "Inventur pruefen" : "Check count", de ? "Soll/Ist und Differenz" : "Expected/actual and variance", (ctx) => de
          ? `Pruefe diesen Bestand fuer eine Inventur- oder Schnellkorrektur: ${ctx.label}. Stelle Sollbestand, moeglichen Istbestand, Differenz, Grund und Audit-Folgeaktion dar.`
          : `Check this stock for count or quick correction: ${ctx.label}. Show expected stock, possible actual stock, variance, reason, and audit follow-up.`);
      } else if (item.recordType === "warehouse_source") {
        add("warehouse-source-load", de ? "Lagerquelle pruefen" : "Check source", de ? "Deckung, Engpass, Umlagerung" : "Coverage, bottleneck, transfer", (ctx) => de
          ? `Pruefe Lagerquelle ${ctx.label}. Bewerte Deckung, Engpaesse, Umlagerungsbedarf und betroffene Auftraege.`
          : `Check warehouse source ${ctx.label}. Assess coverage, bottlenecks, transfer needs, and affected orders.`);
      }
    }

    add("business-check", de ? "Business-Datensatz pruefen" : "Check business record", de ? "Stammdaten, Beleg, Folgeaktion" : "Master data, document, next action", (ctx) => de
      ? `Pruefe diesen Business-Kontext ${ctx.label}. Kontrolliere Stammdaten, Beleg-/Produktbezug, offene Folgeaktionen und Synchronisierung.`
      : `Check this business context ${ctx.label}. Review master data, document/product relation, open follow-ups, and synchronization.`);
  }

  if (item.moduleId === "ctox") {
    add("ctox-run", de ? "CTOX Lauf pruefen" : "Check CTOX run", de ? "Status, Kontext, Ergebnis" : "Status, context, result", (ctx) => de
      ? `Pruefe diesen CTOX-Kontext ${ctx.label}. Fasse Status, Input-Kontext, Ergebnis, Fehler und naechste Aktion zusammen.`
      : `Check this CTOX context ${ctx.label}. Summarize status, input context, result, errors, and next action.`);
  }

  add("summarize", de ? "Kurz zusammenfassen" : "Summarize", de ? "Kontext und naechste Aktion" : "Context and next action", (ctx) => de
    ? `Fasse diesen Kontext kurz zusammen und nenne die naechste sinnvolle Aktion: ${ctx.label}.`
    : `Briefly summarize this context and name the next useful action: ${ctx.label}.`);

  return actions.slice(0, 4);
}

function fitMenuPosition(clientX: number, clientY: number, actionCount = 0) {
  const width = 234;
  const height = 100 + actionCount * 42;
  return {
    x: Math.max(8, Math.min(clientX, window.innerWidth - width - 8)),
    y: Math.max(8, Math.min(clientY, window.innerHeight - height - 8))
  };
}

async function postQueue(body: Record<string, unknown>): Promise<QueueResponse> {
  const response = await fetch("/api/ctox/queue-tasks", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<QueueResponse>;
}
