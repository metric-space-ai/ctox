import { type WorkSurfacePanelState } from "@ctox-business/ui";
import type { ReactNode } from "react";
import { getCtoxHarnessFlow, type CtoxHarnessFlowResult } from "../lib/ctox-core-bridge";
import { getCtoxBundle, getCtoxResource, type CtoxBundle } from "../lib/ctox-seed";
import { CtoxQueueButton } from "./ctox-actions";

type QueryState = {
  locale?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
  drawer?: string;
};

type Resource = keyof CtoxBundle | "harness" | "settings";

export async function CtoxWorkspace({
  companyName,
  submoduleId,
  query
}: {
  companyName: string;
  submoduleId: string;
  query: QueryState;
}) {
  const data = await getCtoxBundle();
  const resource = resolveResource(submoduleId);

  if (resource === "settings") return <SettingsView companyName={companyName} query={query} submoduleId={submoduleId} />;
  if (resource === "harness") return <HarnessView flow={await getCtoxHarnessFlow()} query={query} submoduleId={submoduleId} />;
  if (resource === "queue") return <QueueView data={data} query={query} submoduleId={submoduleId} />;
  if (resource === "knowledge") return <KnowledgeView data={data} query={query} submoduleId={submoduleId} />;
  if (resource === "bugs") return <BugsView data={data} query={query} submoduleId={submoduleId} />;
  if (resource === "sync") return <SyncView data={data} query={query} submoduleId={submoduleId} />;
  return <RunsView data={data} query={query} submoduleId={submoduleId} />;
}

export async function CtoxPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const resource = resolveResource(submoduleId);
  if (resource === "settings") return null;
  if (resource === "harness") return <HarnessPanel query={query} submoduleId={submoduleId} />;
  const record = (await getCtoxResource(resource))?.find((item) => item.id === panelState?.recordId);

  if (!record) return null;

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader query={query} submoduleId={submoduleId} title={recordTitle(record)} />
      <dl className="drawer-facts">
        <Fact label="Resource" value={resource} />
        {"status" in record ? <Fact label="Status" value={record.status} /> : null}
        {"moduleId" in record ? <Fact label="Module" value={record.moduleId} /> : null}
        {"submoduleId" in record ? <Fact label="Submodule" value={record.submoduleId} /> : null}
        {"priority" in record ? <Fact label="Priority" value={record.priority} /> : null}
        {"pending" in record ? <Fact label="Pending" value={String(record.pending)} /> : null}
        {"tags" in record && record.tags?.length ? <Fact label="Tags" value={record.tags.join(", ")} /> : null}
      </dl>
      <section className="ops-drawer-section">
        <h3>Context</h3>
        <div className="ops-mini-list">
          {recordLines(record).map((line) => <span key={line}>{line}</span>)}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>CTOX instruction</h3>
        <CtoxQueueButton
          instruction={`Work on CTOX ${resource} record ${recordTitle(record)} and keep the Business OS context synchronized.`}
          label={recordTitle(record)}
          recordId={record.id}
          recordType={resource}
          submoduleId={submoduleId}
        />
      </section>
    </div>
  );
}

function HarnessView({ flow, query, submoduleId }: { flow: CtoxHarnessFlowResult; query: QueryState; submoduleId: string }) {
  const finishBlock = flow.flow.blocks.find((block) => block.kind === "finish");
  const stateMachine = finishBlock?.branches.find((branch) => branch.kind === "state_machine");
  const guard = finishBlock?.branches.find((branch) => branch.kind === "guard");
  const processMining = finishBlock?.branches.find((branch) => branch.kind === "process_mining");

  return (
    <div className="ops-workspace ctox-harness-workspace">
      <Pane description={`Live harness flow from ${flow.mode}. Review, outcome, spawn, and process-mining checkpoints stay on the same path.`} title="Harness flow">
        <div className="ctox-harness-flow" data-context-item data-context-label="Harness state machine" data-context-module="ctox" data-context-record-id="harness-flow" data-context-record-type="harness_flow" data-context-submodule={submoduleId}>
          {flow.flow.blocks.map((block, index) => (
            <section className={`ctox-harness-block ctox-harness-block-${block.kind}`} key={`${block.title}-${index}`}>
              <div className="ctox-harness-spine">
                <span>{index + 1}</span>
              </div>
              <div className="ctox-harness-main">
                <h3>{block.title}</h3>
                {block.lines.map((line) => <p key={line}>{line}</p>)}
              </div>
              <div className="ctox-harness-branches">
                {block.branches.map((branch) => (
                  <article className={`ctox-harness-branch ctox-harness-branch-${branch.kind}`} key={branch.title}>
                    <strong>{branch.title}</strong>
                    {branch.lines.map((line) => <span key={line}>{line}</span>)}
                    <small>{branch.returns_to_spine ? "returns to main work" : "creates follow-up branch"}</small>
                  </article>
                ))}
              </div>
            </section>
          ))}
        </div>
      </Pane>
      <Pane description="Same flow in the terminal format used by the TUI and support workflows." title="ASCII mirror">
        <pre className="ctox-harness-ascii">{flow.ascii}</pre>
      </Pane>
      <Pane description="Kernel checkpoints that prevent LLM completion claims from becoming durable truth." title="Kernel gates">
        <SignalList
          items={[
            ["Review Gate", stateMachine?.lines[0] ?? "Review feedback returns to the main work item."],
            ["Outcome Witness", guard?.lines.find((line) => line.includes("outbound") || line.includes("artifact")) ?? "Terminal work requires a durable delivered artifact."],
            ["Spawn Discipline", stateMachine?.lines.find((line) => line.includes("Spawn")) ?? "Every spawned child needs a modeled parent edge and budget."],
            ["Forensics", processMining?.lines.at(-1) ?? "Process mining checks proofs, spawn edges, and conformance."]
          ]}
          recordType="harness_gate"
          submoduleId={submoduleId}
        />
        <div className="ops-pane-actions ctox-harness-actions">
          <CtoxQueueButton
            instruction="Run CTOX harness forensics for the current Business OS state. Check core-liveness, spawn-liveness, process-mining proofs, spawn-edges, harness-mining multiperspective, and report any missing outcome witness or rejected spawn edge."
            label="Run harness forensics"
            recordId="harness-flow"
            recordType="harness_flow"
            submoduleId={submoduleId}
          />
          <a href={panelHref(query, submoduleId, "harness", "harness-flow", "right")}>Open details</a>
        </div>
      </Pane>
    </div>
  );
}

function HarnessPanel({ query, submoduleId }: { query: QueryState; submoduleId: string }) {
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader query={query} submoduleId={submoduleId} title="Harness integrity" />
      <section className="ops-drawer-section">
        <h3>Completion contract</h3>
        <div className="ops-mini-list">
          <span>The agent completes work only after the required durable artifact exists.</span>
          <span>The Review Gate gives feedback but never performs the task.</span>
          <span>Every spawned task must have a modeled parent edge, checkpoint, and bounded budget.</span>
          <span>Process mining verifies proofs, outcome refs, spawn edges, and stuck cases.</span>
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>CTOX instruction</h3>
        <CtoxQueueButton
          instruction="Audit the current CTOX harness flow from the Business OS Harness page. Verify outcome witnesses, review checkpoints, spawn edges, and process-mining conformance before protected outbound work continues."
          label="Harness integrity audit"
          recordId="harness-flow"
          recordType="harness_flow"
          submoduleId={submoduleId}
        />
      </section>
    </div>
  );
}

function RunsView({ data, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-planning-workspace">
      <Pane description="Agent executions and verification loops attached to business modules." title="Agent runs">
        <div className="ops-table ops-meeting-table">
          <div className="ops-table-head"><span>Run</span><span>Status</span><span>Model</span></div>
          {data.runs.map((run) => (
            <ContextRow href={panelHref(query, submoduleId, "run", run.id, "right")} key={run.id} label={run.title} recordId={run.id} recordType="agent_run" submoduleId={submoduleId}>
              <strong>{run.title}</strong>
              <small>{run.moduleId} / {run.submoduleId} · {run.startedAt}</small>
              <span>{run.status}</span>
              <span>{run.model}</span>
            </ContextRow>
          ))}
        </div>
      </Pane>
      <Pane description="Current run outcomes and next verification work." title="Run context">
        <SignalList items={data.runs.map((run) => [run.title, run.summary])} recordType="run_signal" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function QueueView({ data, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-board-workspace">
      {["queued", "running", "blocked", "done"].map((status) => (
        <Pane description={`${status} CTOX tasks`} key={status} title={status}>
          <div className="ops-card-stack">
            {data.queue.filter((item) => item.status === status).map((item) => (
              <Card href={panelHref(query, submoduleId, "queue", item.id, "right")} key={item.id} label={item.title} recordId={item.id} recordType="queue_item" submoduleId={submoduleId}>
                <strong>{item.title}</strong>
                <small>{item.priority} · {item.source}</small>
                <span>{item.target}</span>
              </Card>
            ))}
          </div>
        </Pane>
      ))}
    </div>
  );
}

function KnowledgeView({ data, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-knowledge-workspace">
      <Pane description="Records CTOX can use as durable implementation and product context." title="Knowledge records">
        <div className="ops-note-feed">
          {data.knowledge.map((item) => (
            <a
              data-context-item
              data-context-label={item.title}
              data-context-module="ctox"
              data-context-record-id={item.id}
              data-context-record-type="knowledge_record"
              data-context-submodule={submoduleId}
              href={panelHref(query, submoduleId, "knowledge", item.id, "right")}
              key={item.id}
            >
              <strong>{item.title}</strong>
              <span>{item.summary}</span>
              <small>{item.moduleId} · {item.recordType} · {item.updatedAt}</small>
            </a>
          ))}
        </div>
      </Pane>
      <Pane description="Business records that should stay linked to the knowledge store." title="Linked records">
        <SignalList items={data.knowledge.map((item) => [item.title, item.linkedRecords.join(", ")])} recordType="knowledge_link" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function BugsView({ data, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-project-workspace">
      <Pane description="Bug reports from the integrated reporter and follow-up triage." title="Bug reports">
        <div className="ops-table ops-work-table">
          <div className="ops-table-head"><span>Bug</span><span>Status</span><span>Severity</span><span>Module</span></div>
          {data.bugs.map((bug) => (
            <ContextRow href={panelHref(query, submoduleId, "bug", bug.id, "right")} key={bug.id} label={bug.title} recordId={bug.id} recordType="bug_report" submoduleId={submoduleId}>
              <strong>{bug.title}</strong>
              <small>{bug.summary}{bug.tags?.length ? ` · ${bug.tags.join(" · ")}` : ""}</small>
              <span>{bug.status}</span>
              <span>{bug.severity}</span>
              <span>{bug.moduleId}</span>
            </ContextRow>
          ))}
        </div>
      </Pane>
      <Pane description="Reporter and prompt issues that should create CTOX tasks." title="Triage rail">
        <SignalList items={data.bugs.map((bug) => [bug.title, `${bug.status} · ${bug.createdAt}`])} recordType="bug_signal" submoduleId={submoduleId} />
      </Pane>
      <Pane description="Use the global Bug button for new reports with screenshots and markup." title="Reporter">
        <div className="ops-signal-list">
          <div className="ops-signal">
            <span>Screenshot area</span>
            <strong>On</strong>
          </div>
          <div className="ops-signal">
            <span>Pen markup</span>
            <strong>On</strong>
          </div>
          <div className="ops-signal">
            <span>Queue task</span>
            <strong>On</strong>
          </div>
        </div>
      </Pane>
    </div>
  );
}

function SyncView({ data, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-knowledge-workspace">
      <Pane description="Bridge state between Postgres business data and CTOX SQLite-held core runtime." title="Sync health">
        <div className="ops-table ops-knowledge-table">
          <div className="ops-table-head"><span>Module</span><span>Status</span><span>Pending</span></div>
          {data.sync.map((sync) => (
            <ContextRow href={panelHref(query, submoduleId, "sync", sync.id, "right")} key={sync.id} label={sync.moduleId} recordId={sync.id} recordType="sync_event" submoduleId={submoduleId}>
              <strong>{sync.moduleId}</strong>
              <small>{sync.lastEvent} · {sync.lastSyncedAt}</small>
              <span>{sync.status}</span>
              <span>{String(sync.pending)}</span>
            </ContextRow>
          ))}
        </div>
      </Pane>
      <Pane description="The business app mirrors only events and tasks into CTOX core." title="Boundary">
        <SignalList
          items={[
            ["SQLite", "CTOX core runtime, queue, skills, thread state"],
            ["Postgres", "Business app data, module records, customer customization"],
            ["Bridge", "Events, bug reports, right-click prompts, deep links"]
          ]}
          recordType="sync_boundary"
          submoduleId={submoduleId}
        />
      </Pane>
    </div>
  );
}

function SettingsView({
  companyName,
  query,
  submoduleId
}: {
  companyName: string;
  query: QueryState;
  submoduleId: string;
}) {
  const brandName = `${companyName} Business OS`;

  return (
    <div className="ops-workspace ctox-settings-workspace">
      <Pane description="Workspace identity used by the shell, public app entry, prompts, and module context." title="Settings">
        <form className="ctox-settings-form" action="/api/settings/company" method="post">
          <input name="next" type="hidden" value={baseHref(query, submoduleId)} />
          <label>
            <span>Company name</span>
            <input autoComplete="organization" maxLength={80} name="companyName" required type="text" defaultValue={companyName} />
          </label>
          <div className="ctox-settings-preview">
            <span>Visible product name</span>
            <strong>{brandName}</strong>
            <small>CTOX is only the default placeholder. The app appends Business OS automatically.</small>
          </div>
          <button type="submit">Save settings</button>
        </form>
      </Pane>
      <Pane description="Create tenant-owned Postgres placeholders from mission and vision. Real customer data stays in the tenant database." title="Mission bootstrap">
        <form className="ctox-settings-form" action="/api/settings/bootstrap-demo" method="post">
          <input name="next" type="hidden" value={baseHref(query, submoduleId)} />
          <input name="companyName" type="hidden" value={companyName} />
          <label>
            <span>Mission</span>
            <textarea name="mission" rows={4} placeholder="What the company exists to do." />
          </label>
          <label>
            <span>Vision</span>
            <textarea name="vision" rows={4} placeholder="What future state the company wants to create." />
          </label>
          <label>
            <span>Bootstrap mode</span>
            <select name="mode" defaultValue="demo">
              <option value="demo">Generate module placeholders</option>
              <option value="guided">Preview bootstrap plan only</option>
              <option value="empty">Create tenant settings only</option>
            </select>
          </label>
          <button type="submit">Initialize Business OS</button>
        </form>
      </Pane>
      <Pane description="Where this name is reused across the app." title="Brand propagation">
        <div className="ops-signal-list">
          <div className="ops-signal">
            <span>Shell header</span>
            <small>{brandName}</small>
          </div>
          <div className="ops-signal">
            <span>Public entry page</span>
            <small>{brandName}</small>
          </div>
          <div className="ops-signal">
            <span>CTOX prompts</span>
            <small>Business OS context uses the configured company identity.</small>
          </div>
        </div>
      </Pane>
    </div>
  );
}

function Pane({ children, description, title }: { children: ReactNode; description: string; title: string }) {
  return (
    <section className="ops-pane">
      <div className="ops-pane-head">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </div>
      {children}
    </section>
  );
}

function Card({ children, href, label, recordId, recordType, submoduleId }: ContextProps & { children: ReactNode }) {
  return (
    <a
      className="ops-work-card"
      data-context-item
      data-context-label={label}
      data-context-module="ctox"
      data-context-record-id={recordId}
      data-context-record-type={recordType}
      data-context-submodule={submoduleId}
      href={href}
    >
      {children}
    </a>
  );
}

function ContextRow({ children, href, label, recordId, recordType, submoduleId }: ContextProps & { children: ReactNode }) {
  return (
    <a
      className="ops-table-row"
      data-context-item
      data-context-label={label}
      data-context-module="ctox"
      data-context-record-id={recordId}
      data-context-record-type={recordType}
      data-context-submodule={submoduleId}
      href={href}
    >
      {children}
    </a>
  );
}

function SignalList({ items, recordType, submoduleId }: { items: Array<[string, string]>; recordType: string; submoduleId: string }) {
  return (
    <div className="ops-signal-list">
      {items.map(([label, value]) => (
        <div
          data-context-item
          data-context-label={label}
          data-context-module="ctox"
          data-context-record-id={label.toLowerCase().replaceAll(" ", "-")}
          data-context-record-type={recordType}
          data-context-submodule={submoduleId}
          className="ops-signal"
          key={label}
        >
          <span>{label}</span>
          <small>{value}</small>
        </div>
      ))}
    </div>
  );
}

function DrawerHeader({ query, submoduleId, title }: { query: QueryState; submoduleId: string; title: string }) {
  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <a href={baseHref(query, submoduleId)}>Close</a>
    </div>
  );
}

function Fact({ label, value }: { label: string; value?: string }) {
  return value ? <div><dt>{label}</dt><dd>{value}</dd></div> : null;
}

function panelHref(query: QueryState, submoduleId: string, panel: string, recordId: string, drawer: "left-bottom" | "bottom" | "right") {
  if (query.panel === panel && query.recordId === recordId) return baseHref(query, submoduleId);
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  return `/app/ctox/${submoduleId}?${params.toString()}`;
}

function baseHref(query: QueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `/app/ctox/${submoduleId}?${queryString}` : `/app/ctox/${submoduleId}`;
}

function resolveResource(submoduleId: string): Resource {
  if (submoduleId === "harness") return "harness";
  if (submoduleId === "queue") return "queue";
  if (submoduleId === "knowledge") return "knowledge";
  if (submoduleId === "bugs") return "bugs";
  if (submoduleId === "sync") return "sync";
  if (submoduleId === "settings") return "settings";
  return "runs";
}

type CtoxRecord = CtoxBundle["runs"][number] | CtoxBundle["queue"][number] | CtoxBundle["knowledge"][number] | CtoxBundle["bugs"][number] | CtoxBundle["sync"][number];

function recordTitle(record: CtoxRecord) {
  if ("title" in record) return record.title;
  return record.moduleId;
}

function recordLines(record: CtoxRecord) {
  if ("lastEvent" in record) return [record.lastEvent, record.lastSyncedAt, `${record.pending} pending`];
  if ("target" in record) return [record.target, record.source, record.createdAt];
  if ("linkedRecords" in record) return [record.summary, record.linkedRecords.join(", ")];
  if ("pageUrl" in record) return [record.summary, record.expected ?? "", record.pageUrl ?? "", record.tags?.join(", ") ?? ""].filter(Boolean);
  if ("summary" in record) return [record.summary];
  return [];
}

type ViewProps = {
  data: CtoxBundle;
  query: QueryState;
  submoduleId: string;
};

type ContextProps = {
  href: string;
  label: string;
  recordId: string;
  recordType: string;
  submoduleId: string;
};
