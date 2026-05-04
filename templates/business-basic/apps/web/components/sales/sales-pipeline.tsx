import type { CSSProperties } from "react";
import {
  pipelineCrmDataset,
  pipelineDate,
  pipelineDateTime,
  pipelineMoney,
  pipelineStatus,
  localizePipelineStageLines,
  localizePipelineStageName,
  localizePipelineStageText,
  type PipelineDataset,
  type PipelineLocale,
  type PipelineStage,
  type PipelineTask,
  type PipelineTransitionMessage,
  type PipelineTransitionRun,
  type TransitionStatus
} from "../../lib/pipeline-crm";

type QueryState = {
  locale?: string;
  mode?: string;
  selectedId?: string;
  theme?: string;
};

type Mode = "kanban" | "list";

type SalesCard = {
  id: string;
  kind: "deal";
  stageId: string;
  title: string;
  organization: string;
  contactName: string;
  contactEmail: string;
  value: number;
  currency: string;
  ownerName: string;
  nextStep: string;
  nextStepDueAt: string;
  lastActivityAt: string;
  closeDate: string;
  source: string;
  forecastCategory: string;
  tags: string[];
  transitionReadiness: TransitionStatus;
  transitionBlockers: string[];
  activeRun?: PipelineTransitionRun;
  blockerSummary: string;
  transitionTarget: string;
  lastTransitionLog?: string;
};

type TodoItem = {
  id: string;
  source: "task" | "next_step";
  title: string;
  dueAt: string;
  related: string;
  priority: "low" | "medium" | "high";
  cardId?: string;
};

export function SalesPipelineView({ query }: { query: QueryState }) {
  const locale = query.locale === "en" ? "en" : "de";
  const mode: Mode = query.mode === "list" ? "list" : "kanban";
  const selectedId = query.selectedId;
  const data = pipelineCrmDataset;
  const cards = buildSalesCards(data);
  const selected = selectedId ? cards.find((card) => card.id === selectedId) : undefined;
  const todos = buildTodos(data.tasks, cards).sort((left, right) => Date.parse(left.dueAt) - Date.parse(right.dueAt));
  const openDealValue = cards.reduce((sum, card) => sum + card.value, 0);
  const runningTransitions = cards.filter((card) => card.transitionReadiness === "running").length;
  const readyTransitions = cards.filter((card) => card.transitionReadiness === "ready").length;
  const overdue = todos.filter((todo) => isOverdue(todo.dueAt)).length;

  return (
    <section className="sales-pipeline" data-context-module="sales" data-context-submodule="pipeline">
      <input className="drawer-check" id="pipeline-todo-drawer-toggle" type="checkbox" />
      <header className="pipeline-work-header">
        <div className="pipeline-work-title">
          <h1>Pipeline</h1>
          <p>{locale === "en" ? "Current state, gates and next actions." : "Ist-Zustand, Gates und nächste Aktionen."}</p>
        </div>
        <section className="pipeline-work-summary" aria-label="Sales summary">
          <span><strong>{cards.length}</strong> Deals</span>
          <span><strong>{pipelineMoney(openDealValue, "EUR", locale)}</strong> Pipeline</span>
          <span><strong>{readyTransitions}</strong> {locale === "en" ? "ready" : "bereit"}</span>
          <span><strong>{runningTransitions}</strong> {locale === "en" ? "running" : "läuft"}</span>
          <span><strong>{overdue}</strong> {locale === "en" ? "overdue" : "überfällig"}</span>
        </section>
        <div className="pipeline-workspace-actions">
          <nav className="pipeline-segmented" aria-label="Pipeline view mode">
            <a className={mode === "kanban" ? "active" : ""} href={pipelineHref(query, "kanban")}>Kanban</a>
            <a className={mode === "list" ? "active" : ""} href={pipelineHref(query, "list")}>{locale === "en" ? "List" : "Liste"}</a>
          </nav>
          <label className="pipeline-drawer-button" htmlFor="pipeline-todo-drawer-toggle">
            {locale === "en" ? "Next actions" : "Nächste Aktionen"} <span>{todos.length}</span>
          </label>
        </div>
      </header>

      <main className="pipeline-sales-main">
        {mode === "list"
          ? <CardTable cards={cards} stages={data.stages} query={query} locale={locale} />
          : <KanbanBoard cards={cards} stages={data.stages} query={query} locale={locale} />}
      </main>

      <label className="drawer-scrim" htmlFor="pipeline-todo-drawer-toggle" aria-label={locale === "en" ? "Close" : "Schließen"} />
      <aside className="todo-drawer-panel" aria-label={locale === "en" ? "Next actions" : "Nächste Aktionen"}>
        <div className="drawer-head">
          <strong>{locale === "en" ? "Next actions" : "Nächste Aktionen"}</strong>
          <label htmlFor="pipeline-todo-drawer-toggle">{locale === "en" ? "Close" : "Schließen"}</label>
        </div>
        <TodoPanel todos={todos} locale={locale} query={query} />
      </aside>

      {selected ? (
        <aside className="bottom-drawer open" aria-label={`${selected.title} details`}>
          <div className="drawer-head">
            <strong>{locale === "en" ? "Shard details" : "Shard Details"}</strong>
            <a href={pipelineHref(query, mode)}>{locale === "en" ? "Close" : "Schließen"}</a>
          </div>
          <Inspector card={selected} stages={data.stages} transitionMessages={data.transitionMessages} mode={mode} locale={locale} />
        </aside>
      ) : null}

      <script dangerouslySetInnerHTML={{ __html: pipelinePipelineScript(locale) }} />
    </section>
  );
}

function KanbanBoard({ cards, stages, query, locale }: { cards: SalesCard[]; stages: PipelineStage[]; query: QueryState; locale: PipelineLocale }) {
  return (
    <div className="kanban-wrap">
      <p className="kanban-status" aria-live="polite" data-kanban-status>
        {locale === "en" ? "Drag a ready deal into the next stage to start its transition." : "Ziehe einen bereiten Deal in die nächste Stufe, um die Transition zu starten."}
      </p>
      <div className="sales-board" aria-label="Sales kanban">
        {stages.sort((left, right) => left.sortOrder - right.sortOrder).map((stage) => {
          const stageCards = cards.filter((card) => card.stageId === stage.id);
          const stageName = localizePipelineStageName(locale, stage.name);
          const exitCriteria = localizePipelineStageLines(locale, stage, "exitCriteria");
          const agentTodos = localizePipelineStageLines(locale, stage, "transitionAgentTodos");
          return (
            <section className="sales-column" data-stage-id={stage.id} key={stage.id} style={{ "--stage-color": stage.color } as CSSProperties}>
              <header className="column-head">
                <div>
                  <h2>{stageName}</h2>
                  <p>{stageCards.length} Deals · {stageCards.filter((card) => card.transitionReadiness === "ready").length} {locale === "en" ? "ready" : "bereit"} · {pipelineMoney(stageCards.reduce((sum, card) => sum + card.value, 0), "EUR", locale)}</p>
                </div>
                <button className="stage-config-trigger" data-stage-config-open={stage.id} type="button" aria-label={`${locale === "en" ? "Configure automation" : "Automation konfigurieren"}: ${stageName}`}>⚙</button>
              </header>
              <div className="exit-criteria">{exitCriteria.map((criterion) => <span key={criterion}>{criterion}</span>)}</div>
              <section className="stage-agent-preview" aria-label={`${stageName} Agent-Todos`}>
                <p>Agent-Todos</p>
                <ol>{agentTodos.slice(0, 3).map((todo) => <li key={todo}>{todo}</li>)}</ol>
              </section>
              <div className="card-stack">
                {stageCards.map((card) => <CardLink card={card} query={query} locale={locale} key={card.id} />)}
              </div>
              <section className="stage-config-sheet" data-stage-config-panel={stage.id} aria-label={`${stageName} automation settings`} hidden>
                <form data-stage-config-form data-stage-id={stage.id} data-stage-name={stageName}>
                  <header className="drawer-head"><strong>{stageName} {locale === "en" ? "automation settings" : "Automation-Einstellungen"}</strong><button data-stage-config-close type="button">{locale === "en" ? "Close" : "Schließen"}</button></header>
                  <div className="stage-config-body">
                    <label>{locale === "en" ? "Gate prerequisites" : "Gate-Voraussetzungen"}<textarea name="transitionStartCriteria" defaultValue={stage.transitionStartCriteria} /></label>
                    <label>Agent-Todos<textarea name="transitionAgentTodos" defaultValue={agentTodos.join("\n")} /></label>
                    <label>Agent-Prompt<textarea name="transitionAgentPrompt" defaultValue={stage.transitionAgentPrompt} /></label>
                    <button className="button" type="submit">{locale === "en" ? "Save automation update" : "Automation-Update speichern"}</button>
                  </div>
                </form>
              </section>
            </section>
          );
        })}
      </div>
    </div>
  );
}

function CardLink({ card, query, locale }: { card: SalesCard; query: QueryState; locale: PipelineLocale }) {
  return (
    <a
      className={`sales-card ${query.selectedId === card.id ? "selected" : ""}`}
      data-active-run={card.activeRun ? "true" : "false"}
      data-card-id={card.id}
      data-card-kind={card.kind}
      data-card-stage-id={card.stageId}
      data-card-title={card.title}
      data-context-item
      data-context-label={card.title}
      data-context-module="sales"
      data-context-record-id={card.id}
      data-context-record-type="opportunity"
      data-context-submodule="pipeline"
      data-transition-blockers={card.transitionBlockers.join(", ")}
      data-transition-readiness={card.transitionReadiness}
      draggable
      href={pipelineHref(query, "kanban", card.id)}
    >
      <span className="card-topline"><strong>{card.organization}</strong><span>{pipelineMoney(card.value, card.currency, locale)}</span></span>
      <span className="deal-name">{card.title}</span>
      <span className="card-context">{card.contactName} · {card.ownerName}</span>
      <span className="card-next"><small>{locale === "en" ? "Next action" : "Nächste Aktion"}</small>{card.nextStep}<time className={isOverdue(card.nextStepDueAt) ? "due overdue" : "due"}>{pipelineDateTime(card.nextStepDueAt, locale)}</time></span>
      <span className={`gate-line ${card.transitionReadiness}`}>
        <TransitionBadge status={card.transitionReadiness} blockers={card.transitionBlockers} locale={locale} />
        <span>{card.transitionReadiness === "blocked" ? card.blockerSummary : card.transitionReadiness === "running" ? card.lastTransitionLog ?? "Gate criteria, agent todos and agent prompt active." : `${locale === "en" ? "ready for" : "bereit für Wechsel zu"} ${localizePipelineStageName(locale, card.transitionTarget).toLowerCase()}`}</span>
      </span>
      {card.activeRun ? <Progress value={card.activeRun.progress} compact /> : null}
    </a>
  );
}

function CardTable({ cards, stages, query, locale }: { cards: SalesCard[]; stages: PipelineStage[]; query: QueryState; locale: PipelineLocale }) {
  const sorted = [...cards].sort((left, right) => Date.parse(left.nextStepDueAt) - Date.parse(right.nextStepDueAt));
  return (
    <section className="pipeline-table-wrap" aria-label="Pipeline list">
      <table>
        <thead><tr><th>{locale === "en" ? "Record" : "Datensatz"}</th><th>{locale === "en" ? "Stage" : "Stufe"}</th><th>Gate</th><th>{locale === "en" ? "Next step" : "Nächster Schritt"}</th><th>{locale === "en" ? "Due" : "Fällig"}</th><th>Owner</th><th>{locale === "en" ? "Value" : "Wert"}</th></tr></thead>
        <tbody>
          {sorted.map((card) => {
            const stage = stages.find((item) => item.id === card.stageId);
            return (
              <tr className={`${card.transitionReadiness === "blocked" ? "blocked-row" : ""} ${query.selectedId === card.id ? "selected-row" : ""}`} key={card.id}>
                <td><a href={pipelineHref(query, "list", card.id)}><strong>{card.title}</strong></a><br /><span>{card.organization} · {card.contactName}</span></td>
                <td>{stage ? localizePipelineStageName(locale, stage.name) : ""}</td>
                <td><TransitionBadge status={card.transitionReadiness} blockers={card.transitionBlockers} locale={locale} /></td>
                <td>{card.nextStep}</td>
                <td><span className={isOverdue(card.nextStepDueAt) ? "due overdue" : "due"}>{pipelineDateTime(card.nextStepDueAt, locale)}</span></td>
                <td>{card.ownerName}</td>
                <td>{pipelineMoney(card.value, card.currency, locale)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}

function TodoPanel({ todos, locale, query }: { todos: TodoItem[]; locale: PipelineLocale; query: QueryState }) {
  return (
    <aside className="todo-panel">
      <header className="panel-head"><div><h2>{locale === "en" ? "Next actions" : "Nächste Aktionen"}</h2><p>{locale === "en" ? "Oldest due date first." : "Älteste Fälligkeit zuerst."}</p></div><span>{todos.length}</span></header>
      <div className="todo-list">
        {todos.map((todo) => (
          <a className="todo" href={todo.cardId ? pipelineHref(query, query.mode === "list" ? "list" : "kanban", todo.cardId) : pipelineHref(query, query.mode === "list" ? "list" : "kanban")} key={todo.id}>
            <span aria-hidden="true">✓</span>
            <span><strong>{todo.title}</strong><small>{todo.related} · <span className={isOverdue(todo.dueAt) ? "due overdue" : "due"}>{pipelineDateTime(todo.dueAt, locale)}</span></small></span>
            <em className={`priority ${todo.priority}`}>{todo.priority}</em>
          </a>
        ))}
      </div>
    </aside>
  );
}

function Inspector({ card, stages, transitionMessages, mode, locale }: { card: SalesCard; stages: PipelineStage[]; transitionMessages: PipelineTransitionMessage[]; mode: Mode; locale: PipelineLocale }) {
  const currentStage = stages.find((stage) => stage.id === card.stageId);
  const nextStage = getNextStage(stages, card.stageId);
  const currentStageName = currentStage ? localizePipelineStageName(locale, currentStage.name) : "Stage";
  const nextStageName = nextStage ? localizePipelineStageName(locale, nextStage.name) : undefined;
  const currentStartCriteria = currentStage ? localizePipelineStageText(locale, currentStage, "transitionStartCriteria") : "";
  const currentAgentTodos = currentStage ? localizePipelineStageLines(locale, currentStage, "transitionAgentTodos") : [];
  const currentAgentPrompt = currentStage ? localizePipelineStageText(locale, currentStage, "transitionAgentPrompt") : "";
  const activeRun = card.activeRun;
  const runMessages = activeRun ? transitionMessages.filter((message) => message.runId === activeRun.id) : [];
  const canStart = card.transitionReadiness === "ready" && Boolean(nextStage) && !activeRun;
  const labels = inspectorLabels[locale];

  return (
    <aside className="inspector" aria-label="Selected card inspector">
      <header className="panel-head">
        <div><h2>{card.title}</h2><p>{card.organization} · {card.contactName}</p></div>
        <a className="button secondary" href={pipelineHref({ locale }, mode, card.id)}>{labels.open}</a>
      </header>
      <form className="inspector-fields" data-card-form data-card-id={card.id} data-card-title={card.title}>
        <label>{labels.stage}
          <select name="stageId" defaultValue={card.stageId}>
            {stages.map((stage) => <option value={stage.id} key={stage.id}>{localizePipelineStageName(locale, stage.name)}</option>)}
          </select>
        </label>
        <label>{labels.nextAction}<input name="nextStep" defaultValue={card.nextStep} /></label>
        <label>{labels.dueDate}<input name="nextStepDueAt" type="datetime-local" defaultValue={toDatetimeLocal(card.nextStepDueAt)} /></label>
        <label>Forecast
          <select name="forecastCategory" defaultValue={card.forecastCategory}>
            <option value="pipeline">Pipeline</option>
            <option value="best_case">Best case</option>
            <option value="commit">Commit</option>
            <option value="closed">Closed</option>
          </select>
        </label>
        <button className="button" type="submit">{labels.saveShard}</button>
      </form>
      <section className="automation-panel" aria-label="Transition automation">
        <header className="section-head">
          <div><h3>Transition</h3><p>{currentStageName}{nextStageName ? ` -> ${nextStageName}` : ` · ${locale === "en" ? "final stage" : "finale Stufe"}`}</p></div>
          <TransitionBadge status={card.transitionReadiness} blockers={card.transitionBlockers} locale={locale} />
        </header>
        {card.transitionBlockers.length > 0 ? <ul className="blocker-list">{card.transitionBlockers.map((blocker) => <li key={blocker}>{blocker}</li>)}</ul> : <p className="gate-ok">{labels.gateOk}</p>}

        {currentStage ? (
          <form className="stage-config" data-stage-config-form data-stage-id={currentStage.id} data-stage-name={currentStageName}>
            <label>{labels.gatePrerequisites}<textarea name="transitionStartCriteria" defaultValue={currentStartCriteria} /></label>
            <label>Agent-Todos<textarea name="transitionAgentTodos" defaultValue={currentAgentTodos.join("\n")} /></label>
            <label>Agent-Prompt<textarea name="transitionAgentPrompt" defaultValue={currentAgentPrompt} /></label>
            <button className="button secondary" type="submit">{labels.saveStageGate}</button>
          </form>
        ) : null}

        {activeRun ? (
          <div className="run-panel">
            <div className="run-head"><strong>{labels.runActive}</strong><span>{activeRun.progress}%</span></div>
            <Progress value={activeRun.progress} />
            <dl className="compact-defs"><dt>{labels.criteria}</dt><dd>{activeRun.criteriaSnapshot}</dd><dt>Prompt</dt><dd>{activeRun.agentPromptSnapshot}</dd></dl>
            <ol className="transition-log">{activeRun.log.map((entry, index) => <li key={`${entry.at}-${index}`}><time>{pipelineDateTime(entry.at, locale)}</time><span className={entry.level}>{entry.message}</span></li>)}</ol>
            <div className="transition-chat">
              {runMessages.map((message) => <p className={`chat-line ${message.role}`} key={message.id}><strong>{message.role}</strong>{message.body}</p>)}
            </div>
            <form className="chat-form" data-transition-chat-form data-run-id={activeRun.id} data-card-id={card.id}>
              <input name="body" placeholder={labels.transitionChatPlaceholder} />
              <button className="button secondary" type="submit">{labels.send}</button>
            </form>
          </div>
        ) : (
          <form data-start-transition-form data-card-id={card.id} data-card-title={card.title} data-from-stage-id={card.stageId} data-to-stage-id={nextStage?.id ?? ""}>
            <button className="button" type="submit" disabled={!canStart}>{canStart ? labels.startTransition : nextStage ? labels.gateBlocked : labels.finalStage}</button>
          </form>
        )}
      </section>
      <dl className="compact-defs"><dt>Email</dt><dd>{card.contactEmail}</dd><dt>{labels.source}</dt><dd>{card.source}</dd><dt>{labels.last}</dt><dd>{pipelineDateTime(card.lastActivityAt, locale)}</dd><dt>Close</dt><dd>{pipelineDate(card.closeDate, locale)}</dd></dl>
    </aside>
  );
}

const inspectorLabels: Record<PipelineLocale, Record<string, string>> = {
  de: {
    criteria: "Kriterien",
    dueDate: "Fälligkeitsdatum",
    finalStage: "finale Stufe",
    gateBlocked: "Gate blockiert",
    gateOk: "Gate-Voraussetzungen sind erfüllt.",
    gatePrerequisites: "Gate-Voraussetzungen",
    last: "Letzte Aktivität",
    nextAction: "Nächste Aktion",
    open: "Öffnen",
    runActive: "Run aktiv",
    saveShard: "Shard speichern",
    saveStageGate: "Stage-Gate speichern",
    send: "Senden",
    source: "Quelle",
    stage: "Stufe",
    startTransition: "Transition starten",
    transitionChatPlaceholder: "Frage stellen oder Kontext zur Transition ergänzen"
  },
  en: {
    criteria: "Criteria",
    dueDate: "Due date",
    finalStage: "final stage",
    gateBlocked: "Gate blocked",
    gateOk: "Gate prerequisites are fulfilled.",
    gatePrerequisites: "Gate prerequisites",
    last: "Last",
    nextAction: "Next action",
    open: "Open",
    runActive: "Run active",
    saveShard: "Save shard",
    saveStageGate: "Save stage gate",
    send: "Send",
    source: "Source",
    stage: "Stage",
    startTransition: "Start transition",
    transitionChatPlaceholder: "Ask or add context to this transition"
  }
};

function TransitionBadge({ status, blockers, locale }: { status: TransitionStatus; blockers: string[]; locale: PipelineLocale }) {
  return <span className={`transition-badge ${status}`} title={blockers.join("\n")}>{pipelineStatus(locale, status)}</span>;
}

function Progress({ value, compact = false }: { value: number; compact?: boolean }) {
  const safe = Math.max(0, Math.min(100, value));
  return <span className={compact ? "transition-progress compact" : "transition-progress"}><span style={{ width: `${safe}%` }} /></span>;
}

function buildSalesCards(data: PipelineDataset): SalesCard[] {
  const ownerName = (ownerId: string) => data.users.find((user) => user.id === ownerId)?.name ?? "Unassigned";
  return data.opportunities.filter((opportunity) => opportunity.status === "open").map((opportunity) => {
    const account = data.accounts.find((item) => item.id === opportunity.accountId);
    const contact = data.contacts.find((item) => item.id === opportunity.primaryContactId);
    const activeRun = opportunity.activeRun ?? data.transitionRuns.find((run) => run.recordId === opportunity.id && run.status === "running");
    const nextStage = getNextStage(data.stages, opportunity.stageId);
    const lastLog = activeRun?.log.at(-1)?.message;
    return {
      id: opportunity.id,
      kind: "deal" as const,
      stageId: opportunity.stageId,
      title: opportunity.name,
      organization: account?.name ?? "Unknown account",
      contactName: contact ? `${contact.firstName} ${contact.lastName}` : "No contact",
      contactEmail: contact?.email ?? "",
      value: opportunity.amount,
      currency: opportunity.currency,
      ownerName: ownerName(opportunity.ownerId),
      nextStep: opportunity.nextStep,
      nextStepDueAt: opportunity.nextStepDueAt,
      lastActivityAt: opportunity.lastActivityAt,
      closeDate: opportunity.closeDate,
      source: opportunity.source,
      forecastCategory: opportunity.forecastCategory,
      tags: opportunity.tags,
      transitionReadiness: opportunity.transitionReadiness,
      transitionBlockers: opportunity.transitionBlockers,
      activeRun,
      blockerSummary: opportunity.transitionBlockers[0] ?? "",
      transitionTarget: nextStage ? nextStage.name : "",
      lastTransitionLog: lastLog
    };
  }).sort((left, right) => Date.parse(left.nextStepDueAt) - Date.parse(right.nextStepDueAt));
}

function buildTodos(tasks: PipelineTask[], cards: SalesCard[]): TodoItem[] {
  return [
    ...tasks.filter((task) => task.status !== "done").map((task) => ({
      id: task.id,
      source: "task" as const,
      title: task.subject,
      dueAt: task.dueAt,
      related: task.relatedType,
      priority: task.priority
    })),
    ...cards.map((card) => ({
      id: `next-${card.id}`,
      source: "next_step" as const,
      title: card.nextStep,
      dueAt: card.nextStepDueAt,
      related: `deal: ${card.title}`,
      priority: isOverdue(card.nextStepDueAt) ? "high" as const : "medium" as const,
      cardId: card.id
    }))
  ];
}

function getNextStage(stages: PipelineStage[], stageId: string) {
  const sorted = [...stages].sort((left, right) => left.sortOrder - right.sortOrder);
  return sorted[sorted.findIndex((stage) => stage.id === stageId) + 1];
}

function isOverdue(value: string) {
  return Date.parse(value) < Date.parse("2026-04-29T23:59:59.000Z");
}

function toDatetimeLocal(value: string) {
  return new Date(value).toISOString().slice(0, 16);
}

function pipelineHref(query: QueryState, mode: Mode, selectedId?: string) {
  const params = new URLSearchParams();
  params.set("mode", mode);
  if (selectedId) params.set("selectedId", selectedId);
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  return `/app/sales/pipeline?${params.toString()}`;
}

function pipelinePipelineScript(locale: PipelineLocale) {
  const messages = locale === "en" ? {
    failed: "Transition failed.",
    gateBlocked: "Gate blocked: ",
    moveRejected: "Move rejected. A shard can only transition into the next funnel stage.",
    queued: "Transition started in CTOX.",
    requirementsMissing: "requirements are not fulfilled.",
    starting: "Starting transition for "
  } : {
    failed: "Transition konnte nicht gestartet werden.",
    gateBlocked: "Gate blockiert: ",
    moveRejected: "Move abgelehnt. Ein Shard kann nur in die nächste Funnel-Stufe wechseln.",
    queued: "Transition in CTOX gestartet.",
    requirementsMissing: "Voraussetzungen sind nicht erfüllt.",
    starting: "Transition wird gestartet für "
  };
  return `(() => {
  if (window.__pipelineCtoxPipeline) return;
  window.__pipelineCtoxPipeline = true;
  const messages = ${JSON.stringify(messages)};
  const setStatus = (message) => {
    const status = document.querySelector("[data-kanban-status]");
    if (status) status.textContent = message;
  };
  const queueSync = async (body) => fetch("/api/sales/opportunities", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body)
  });
  let draggedCardId = null;
  document.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const stageConfig = target.closest("[data-stage-config-open]");
    if (stageConfig) {
      const id = stageConfig.dataset.stageConfigOpen;
      document.querySelectorAll("[data-stage-config-panel]").forEach((panel) => {
        panel.hidden = panel.dataset.stageConfigPanel !== id || !panel.hidden;
      });
    }
    if (target.closest("[data-stage-config-close]")) {
      document.querySelectorAll("[data-stage-config-panel]").forEach((panel) => { panel.hidden = true; });
    }
  });
  document.addEventListener("submit", async (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    if (form.matches("[data-card-form]")) {
      event.preventDefault();
      const formData = new FormData(form);
      await queueSync({
        action: "sync",
        recordId: form.dataset.cardId || "sales-card",
        title: "Pipeline shard update: " + (form.dataset.cardTitle || form.dataset.cardId || "deal"),
        instruction: "Persist edits from the original Pipeline CRM bottom drawer in CTOX Sales.",
        payload: Object.fromEntries(formData.entries())
      });
      setStatus(messages.queued);
      return;
    }
    if (form.matches("[data-start-transition-form]")) {
      event.preventDefault();
      await queueSync({
        action: "sync",
        recordId: form.dataset.cardId || "pipeline-transition",
        title: "Pipeline transition: " + (form.dataset.cardTitle || form.dataset.cardId || "deal"),
        instruction: "Start the original Pipeline CRM pipeline transition and keep CTOX synchronized.",
        payload: {
          opportunityId: form.dataset.cardId,
          fromStageId: form.dataset.fromStageId,
          toStageId: form.dataset.toStageId
        }
      });
      setStatus(messages.queued);
      return;
    }
    if (form.matches("[data-transition-chat-form]")) {
      event.preventDefault();
      const formData = new FormData(form);
      const body = String(formData.get("body") || "").trim();
      if (!body) return;
      await queueSync({
        action: "sync",
        recordId: form.dataset.cardId || form.dataset.runId || "pipeline-transition-message",
        title: "Pipeline transition message",
        instruction: "Add this transition chat context to the original Pipeline CRM run.",
        payload: { runId: form.dataset.runId, body }
      });
      form.reset();
      setStatus(messages.queued);
      return;
    }
    if (!form.matches("[data-stage-config-form]")) return;
    event.preventDefault();
    const stageName = form.dataset.stageName || "stage";
    const formData = new FormData(form);
    const payload = {
      transitionStartCriteria: formData.get("transitionStartCriteria"),
      transitionAgentTodos: formData.get("transitionAgentTodos"),
      transitionAgentPrompt: formData.get("transitionAgentPrompt")
    };
    await queueSync({
        action: "sync",
        recordId: form.dataset.stageId || "stage-automation",
        title: "Pipeline stage automation: " + stageName,
        instruction: "Update Pipeline CRM stage automation configuration for " + stageName + ".",
        payload
    });
    const sheet = form.closest("[data-stage-config-panel]");
    if (sheet) sheet.hidden = true;
    setStatus(messages.queued);
  });
  document.addEventListener("dragstart", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const card = target.closest("[data-card-id]");
    if (!card) return;
    draggedCardId = card.dataset.cardId || null;
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", draggedCardId || "");
  });
  document.addEventListener("dragover", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const column = target.closest(".sales-column");
    if (!column) return;
    event.preventDefault();
    column.classList.add("drop-target");
  });
  document.addEventListener("dragleave", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const column = target.closest(".sales-column");
    if (column && !column.contains(event.relatedTarget)) column.classList.remove("drop-target");
  });
  document.addEventListener("drop", async (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const targetColumn = target.closest(".sales-column");
    if (!targetColumn) return;
    event.preventDefault();
    document.querySelectorAll(".sales-column.drop-target").forEach((column) => column.classList.remove("drop-target"));
    const payloadId = event.dataTransfer.getData("text/plain") || draggedCardId;
    const card = payloadId ? document.querySelector('[data-card-id="' + payloadId + '"]') : null;
    if (!card) return;
    const sourceColumn = card.closest(".sales-column");
    const targetStageId = targetColumn.dataset.stageId;
    const sourceStageId = card.dataset.cardStageId || sourceColumn?.dataset.stageId;
    const nextColumn = sourceColumn?.nextElementSibling?.matches(".sales-column") ? sourceColumn.nextElementSibling : null;
    if (!targetStageId || !sourceStageId || targetStageId === sourceStageId) return;
    if (!nextColumn || targetStageId !== nextColumn.dataset.stageId) {
      setStatus(messages.moveRejected);
      return;
    }
    if (card.dataset.transitionReadiness !== "ready") {
      setStatus(messages.gateBlocked + (card.dataset.transitionBlockers || messages.requirementsMissing));
      return;
    }
    card.classList.add("pending");
    setStatus(messages.starting + (card.dataset.cardTitle || "deal") + "...");
    const response = await queueSync({
        action: "sync",
        recordId: payloadId,
        title: "Pipeline transition: " + (card.dataset.cardTitle || payloadId),
        instruction: "Start the original Pipeline CRM pipeline transition and keep CTOX synchronized.",
        payload: { opportunityId: payloadId, fromStageId: sourceStageId, toStageId: targetStageId }
    });
    card.classList.remove("pending");
    setStatus(response.ok ? messages.queued : messages.failed);
  });
})();`;
}
