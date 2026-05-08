"use client";

/**
 * Payroll workbench — follows the Business OS layout philosophy used in workforce-workbench.tsx:
 * permanent left rail (intake) + central main view (slip kanban for the selected run) +
 * permanent right rail (review/handoff lanes), with edit-heavy surfaces in slide-in drawers
 * (`.wf2-side-drawer` for master data + handoff, `.wf2-bottom` for the selected payslip).
 *
 * CSS classes are reused from Workforce (`.wf2-shell`, `.wf2-rail`, `.wf2-board`, `.wf2-side-drawer`,
 * `.wf2-bottom`) so the visual feel matches the rest of the OS without duplicating styles.
 */

import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import type {
  PayrollAdditional,
  PayrollCommand,
  PayrollComponent,
  PayrollPayslip,
  PayrollPayslipStatus,
  PayrollSnapshot,
  PayrollStructure,
  PayrollStructureAssignment
} from "../lib/payroll-runtime";

type PayrollWorkbenchProps = {
  query: {
    recordId?: string;
    theme?: string;
    locale?: string;
  };
  snapshot: PayrollSnapshot;
  submoduleId?: string;
};

type MutationState = "idle" | "saving" | "saved" | "error";
type SideDrawer = "stamm" | "handoff" | null;

const STATUS_LANES: { key: PayrollPayslipStatus; label: string }[] = [
  { key: "Draft", label: "Entwurf" },
  { key: "Review", label: "Prüfung" },
  { key: "Withheld", label: "Zurückgestellt" },
  { key: "Posted", label: "Gebucht" },
  { key: "Cancelled", label: "Storniert" }
];

function isSmokeId(id: string) {
  return id.startsWith("smoke_") || id.startsWith("pc-smoke-");
}

function filterSmoke(snap: PayrollSnapshot): PayrollSnapshot {
  return {
    ...snap,
    components: snap.components.filter((c) => !isSmokeId(c.id)),
    structures: snap.structures.filter((s) => !isSmokeId(s.id)),
    assignments: snap.assignments.filter((a) => !isSmokeId(a.id)),
    periods: snap.periods.filter((p) => !isSmokeId(p.id)),
    runs: snap.runs.filter((r) => !isSmokeId(r.id)),
    payslips: snap.payslips.filter((s) => !isSmokeId(s.id)),
    additionals: snap.additionals.filter((a) => !isSmokeId(a.id))
  };
}

export function PayrollWorkbench({ query, snapshot, submoduleId = "payroll" }: PayrollWorkbenchProps) {
  const [state, setState] = useState<MutationState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [showTestData, setShowTestData] = useState<boolean>(false);
  const [showCancelledLane, setShowCancelledLane] = useState<boolean>(false);
  const [drawer, setDrawer] = useState<SideDrawer>(null);
  const [rawSnap, setRawSnap] = useState<PayrollSnapshot>(snapshot);

  // ?showTestData=1 / ?showCancelled=1 / ?drawer=stamm in URL bootstraps state (browser proof).
  useEffect(() => {
    if (typeof window === "undefined") return;
    const search = new URLSearchParams(window.location.search);
    if (search.get("showTestData") === "1") setShowTestData(true);
    if (search.get("showCancelled") === "1") setShowCancelledLane(true);
    const d = search.get("drawer");
    if (d === "stamm" || d === "handoff") setDrawer(d);
  }, []);

  const snap = useMemo(() => (showTestData ? rawSnap : filterSmoke(rawSnap)), [rawSnap, showTestData]);

  const initialRunId = snap.runs.find((r) => r.status !== "Cancelled")?.id ?? snap.runs[0]?.id;
  const [selectedRunId, setSelectedRunId] = useState<string | undefined>(initialRunId);
  const [selectedSlipId, setSelectedSlipId] = useState<string | undefined>(query.recordId ?? snap.payslips[0]?.id);

  const selectedRun = useMemo(() => snap.runs.find((r) => r.id === selectedRunId), [snap.runs, selectedRunId]);
  const selectedPeriod = useMemo(
    () => (selectedRun ? snap.periods.find((p) => p.id === selectedRun.periodId) : undefined),
    [snap.periods, selectedRun]
  );
  const slipsForRun = useMemo(
    () => snap.payslips.filter((slip) => slip.runId === selectedRunId),
    [snap.payslips, selectedRunId]
  );
  const slipsByStatus = useMemo(() => {
    const groups: Record<PayrollPayslipStatus, PayrollPayslip[]> = {
      Draft: [],
      Review: [],
      Posted: [],
      Withheld: [],
      Cancelled: []
    };
    for (const slip of slipsForRun) groups[slip.status].push(slip);
    return groups;
  }, [slipsForRun]);
  const selectedSlip = useMemo(
    () => snap.payslips.find((slip) => slip.id === selectedSlipId),
    [snap.payslips, selectedSlipId]
  );

  const reviewSlipsAcrossRuns = useMemo(
    () => snap.payslips.filter((s) => s.status === "Review"),
    [snap.payslips]
  );
  const cancelledSlipsRecent = useMemo(
    () => snap.payslips.filter((s) => s.status === "Cancelled").slice(-5).reverse(),
    [snap.payslips]
  );
  const draftReviewForRun = slipsByStatus.Draft.concat(slipsByStatus.Review);

  async function dispatch(command: PayrollCommand, payload: Record<string, unknown> = {}) {
    setState("saving");
    setError(null);
    try {
      const res = await fetch("/api/operations/payroll", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ command, payload })
      });
      const data = await res.json();
      if (!data.ok) throw new Error(data.error ?? "payroll_command_failed");
      setRawSnap(data.snapshot as PayrollSnapshot);
      setState("saved");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setState("error");
    }
  }

  async function handleNewRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const periodId = String(form.get("periodId") ?? "");
    const payableAccountId = String(form.get("payableAccountId") ?? "1755");
    if (!periodId) return;
    await dispatch("create_run", { periodId, payableAccountId });
  }

  return (
    <div className="wf2-shell" onClick={() => undefined}>
      <aside className="wf2-rail wf2-rail-left">
        <header className="wf2-rail-header">
          <div>
            <span>Eingang</span>
            <h2>Lohnläufe & Stamm</h2>
          </div>
          <button type="button" onClick={() => setDrawer("stamm")}>Verwalten</button>
        </header>

        <section className="wf2-panel">
          <h3>Neuer Lohnlauf</h3>
          <form className="wf2-form" onSubmit={handleNewRun}>
            <select name="periodId" defaultValue={snap.periods.find((p) => !p.locked)?.id ?? snap.periods[0]?.id ?? ""} required>
              {snap.periods.length === 0 && <option value="">— keine Periode —</option>}
              {snap.periods.map((period) => (
                <option key={period.id} value={period.id} disabled={period.locked}>
                  {period.startDate} – {period.endDate}{period.locked ? " (gesperrt)" : ""}
                </option>
              ))}
            </select>
            <input name="payableAccountId" defaultValue="1755" placeholder="Verbindlichkeitskonto" />
            <button type="submit" disabled={state === "saving" || snap.periods.length === 0}>Run anlegen</button>
          </form>
        </section>

        <section className="wf2-panel">
          <h3>Offene Slips ({draftReviewForRun.length})</h3>
          <div className="wf2-stack">
            {draftReviewForRun.map((slip) => (
              <button
                className="wf2-demand"
                key={slip.id}
                onClick={() => setSelectedSlipId(slip.id)}
                type="button"
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_payslip"
                data-context-record-id={slip.id}
                data-context-label={`Lohnabrechnung ${slip.employeeName}`}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <strong>{slip.employeeName}</strong>
                <span>{currency(slip.netPay, slip.currency)} netto · {slip.status === "Draft" ? "Entwurf" : "Prüfung"}</span>
              </button>
            ))}
            {draftReviewForRun.length === 0 && <p className="wf2-muted">Keine offenen Slips für den ausgewählten Run.</p>}
          </div>
        </section>

        <section className="wf2-panel wf2-people">
          <h3>Mitarbeiter</h3>
          {snap.employees.map((employee) => {
            const assignment = snap.assignments.find((a) => a.employeeId === employee.id && !a.toDate);
            return (
              <button
                key={employee.id}
                onClick={() => setDrawer("stamm")}
                type="button"
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_employee"
                data-context-record-id={employee.id}
                data-context-label={employee.displayName}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <span>{initials(employee.displayName)}</span>
                <strong>{employee.displayName}</strong>
                <small>
                  {assignment
                    ? `${currency(assignment.baseSalary, assignment.currency)}/Monat`
                    : "ohne Zuweisung"}
                </small>
              </button>
            );
          })}
        </section>
      </aside>

      <main className="wf2-board">
        <header className="wf2-board-header">
          <div>
            <span>Lohnabrechnung</span>
            <h1>
              {selectedPeriod
                ? `${selectedPeriod.startDate} – ${selectedPeriod.endDate}`
                : "Kein Run ausgewählt"}
              {selectedRun && (
                <span className={`wf2-state ${selectedRun.status === "Posted" || selectedRun.status === "Submitted" ? "ok" : selectedRun.status === "Failed" || selectedRun.status === "Cancelled" ? "err" : ""}`}>
                  {selectedRun.status}
                </span>
              )}
            </h1>
          </div>
          <div className="wf2-status-row">
            {state === "saved" && <span className="wf2-state ok">Gespeichert</span>}
            {state === "error" && <span className="wf2-state err">Fehler: {error}</span>}
            {state === "saving" && <span className="wf2-state">Speichert</span>}
            <select
              value={selectedRunId ?? ""}
              onChange={(event) => setSelectedRunId(event.target.value || undefined)}
              aria-label="Run auswählen"
            >
              <option value="">— Run auswählen —</option>
              {snap.runs.filter((r) => showCancelledLane || r.status !== "Cancelled").map((run) => {
                const period = snap.periods.find((p) => p.id === run.periodId);
                return (
                  <option key={run.id} value={run.id}>
                    {period?.startDate ?? "?"} · {run.status}
                  </option>
                );
              })}
            </select>
            {selectedRunId && (
              <>
                <button
                  type="button"
                  onClick={() => dispatch("queue_run", { id: selectedRunId })}
                  disabled={state === "saving" || (selectedRun?.status !== "Draft" && selectedRun?.status !== "Failed")}
                >
                  Run abschicken
                </button>
                <button type="button" onClick={() => dispatch("recompute_run", { id: selectedRunId })} disabled={state === "saving"}>
                  Neu berechnen
                </button>
                <button type="button" onClick={() => dispatch("bulk_mark_review", { id: selectedRunId })} disabled={state === "saving"}>
                  Alle zur Prüfung
                </button>
                <button type="button" onClick={() => dispatch("bulk_post_run", { id: selectedRunId })} disabled={state === "saving"}>
                  Alle buchen
                </button>
              </>
            )}
            <label className="wf2-toggle" style={{display:"flex",alignItems:"center",gap:4,fontSize:12,color:"var(--muted, #6b7280)",marginLeft:8}}>
              <input type="checkbox" checked={showCancelledLane} onChange={(e) => setShowCancelledLane(e.target.checked)} />
              Stornierte
            </label>
            <label className="wf2-toggle" style={{display:"flex",alignItems:"center",gap:4,fontSize:12,color:"var(--muted, #6b7280)",marginLeft:8}}>
              <input type="checkbox" checked={showTestData} onChange={(e) => setShowTestData(e.target.checked)} />
              Test-Daten
            </label>
          </div>
        </header>

        <section className="wf2-roster" data-testid="payroll-kanban" style={{ gridTemplateColumns: `repeat(${STATUS_LANES.filter((l) => l.key !== "Cancelled" || showCancelledLane).length}, minmax(0, 1fr))` }}>
          <div className="wf2-roster-head">
            {STATUS_LANES.filter((l) => l.key !== "Cancelled" || showCancelledLane).map((lane) => (
              <div key={lane.key}>
                {lane.label} <span>{slipsByStatus[lane.key].length}</span>
              </div>
            ))}
          </div>
          <div className="wf2-row">
            {STATUS_LANES.filter((l) => l.key !== "Cancelled" || showCancelledLane).map((lane) => (
              <div className="wf2-cell" key={lane.key}>
                {slipsByStatus[lane.key].map((slip) => (
                  <article
                    key={slip.id}
                    className={`wf2-card${selectedSlipId === slip.id ? " is-selected" : ""}`}
                    onClick={() => setSelectedSlipId(slip.id)}
                    data-context-module="operations"
                    data-context-submodule={submoduleId}
                    data-context-record-type="payroll_payslip"
                    data-context-record-id={slip.id}
                    data-context-label={`Lohnabrechnung ${slip.employeeName}`}
                    data-context-skill="product_engineering/business-basic-module-development"
                  >
                    <strong>{slip.employeeName}</strong>
                    <span>Brutto {currency(slip.grossPay, slip.currency)}</span>
                    <span>Netto {currency(slip.netPay, slip.currency)}</span>
                    {slip.netPay < 0 && <small style={{ color: "#b91c1c" }}>Negativer Nettowert</small>}
                  </article>
                ))}
                {slipsByStatus[lane.key].length === 0 && <p className="wf2-muted">—</p>}
              </div>
            ))}
          </div>
          {!selectedRunId && (
            <p className="wf2-muted" style={{ padding: 16, textAlign: "center" }}>
              Wähle einen Run aus oder lege links einen neuen an, um Lohnzettel zu sehen.
            </p>
          )}
        </section>

        {selectedRun && (
          <article
            className="wf2-roster-meta" style={{display:"flex",gap:12,padding:"8px 12px",borderTop:"1px solid var(--border, #e5e7eb)",fontSize:12,color:"var(--muted, #6b7280)",alignItems:"center",flexWrap:"wrap"}}
            data-context-module="operations"
            data-context-submodule={submoduleId}
            data-context-record-type="payroll_run"
            data-context-record-id={selectedRun.id}
            data-context-label={`Lohnlauf ${selectedPeriod?.startDate ?? ""}`}
            data-context-skill="product_engineering/business-basic-module-development"
          >
            <span>{selectedRun.frequency}</span>
            <span>Verbindlichkeitskonto {selectedRun.payableAccountId}</span>
            <span>{slipsForRun.length} Slips</span>
            {selectedRun.error && <span style={{ color: "#b91c1c" }}>{selectedRun.error}</span>}
            <button type="button" onClick={() => dispatch("cancel_run", { id: selectedRun.id })} disabled={selectedRun.status === "Cancelled"}>
              Run abbrechen
            </button>
          </article>
        )}
      </main>

      <aside className="wf2-rail wf2-rail-right">
        <header className="wf2-rail-header">
          <div>
            <span>Ausgang</span>
            <h2>Prüfung & Buchung</h2>
          </div>
          <button type="button" onClick={() => setDrawer("handoff")}>Übergaben</button>
        </header>

        <section className="wf2-panel">
          <h3>Slips zur Prüfung ({reviewSlipsAcrossRuns.length})</h3>
          <div className="wf2-stack">
            {reviewSlipsAcrossRuns.map((slip) => (
              <article
                className="wf2-review"
                key={slip.id}
                onClick={() => setSelectedSlipId(slip.id)}
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_payslip"
                data-context-record-id={slip.id}
                data-context-label={`Lohnabrechnung ${slip.employeeName}`}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <strong>{slip.employeeName}</strong>
                <span>{slip.startDate} – {slip.endDate} · netto {currency(slip.netPay, slip.currency)}</span>
                <div>
                  <button
                    type="button"
                    onClick={(e) => { e.stopPropagation(); dispatch("post_payslip", { id: slip.id }); }}
                    disabled={slip.netPay < 0}
                    title={slip.netPay < 0 ? "Negativer Nettowert" : undefined}
                  >
                    Buchen
                  </button>
                  <button type="button" onClick={(e) => { e.stopPropagation(); dispatch("mark_payslip_withheld", { id: slip.id }); }}>
                    Zurückstellen
                  </button>
                </div>
              </article>
            ))}
            {reviewSlipsAcrossRuns.length === 0 && <p className="wf2-muted">Keine Slips in Prüfung.</p>}
          </div>
        </section>

        <section className="wf2-panel">
          <h3>Stornierungen</h3>
          <div className="wf2-stack">
            {cancelledSlipsRecent.map((slip) => (
              <article className="wf2-review is-blocked" key={slip.id} onClick={() => setSelectedSlipId(slip.id)}>
                <strong>{slip.employeeName}</strong>
                <span>{slip.startDate} – {slip.endDate} · netto {currency(slip.netPay, slip.currency)}</span>
              </article>
            ))}
            {cancelledSlipsRecent.length === 0 && <p className="wf2-muted">Keine Stornos.</p>}
          </div>
        </section>

        <section className="wf2-panel">
          <h3>Audit (letzte 6)</h3>
          <div className="wf2-stack">
            {snap.audit.slice(-6).reverse().map((entry) => (
              <article className="wf2-review" key={entry.id}>
                <strong>{entry.entityType}</strong>
                <span>{entry.fromStatus} → {entry.toStatus} · {entry.actor}</span>
                {entry.note && <small>{entry.note}</small>}
              </article>
            ))}
            {snap.audit.length === 0 && <p className="wf2-muted">Noch keine Statuswechsel.</p>}
          </div>
        </section>
      </aside>

      {selectedSlip && (
        <BottomPayslipDrawer
          slip={selectedSlip}
          components={snap.components}
          additionals={snap.additionals.filter((a) => a.employeeId === selectedSlip.employeeId && a.periodId === selectedSlip.periodId)}
          submoduleId={submoduleId}
          state={state}
          onClose={() => setSelectedSlipId(undefined)}
          onMarkReview={() => dispatch("mark_payslip_review", { id: selectedSlip.id })}
          onMarkDraft={() => dispatch("mark_payslip_draft", { id: selectedSlip.id })}
          onMarkWithheld={() => dispatch("mark_payslip_withheld", { id: selectedSlip.id })}
          onPost={() => dispatch("post_payslip", { id: selectedSlip.id })}
          onCancel={() => dispatch("cancel_payslip", { id: selectedSlip.id })}
          onUpdateLine={(lineId, amount) =>
            dispatch("update_payslip_line", { payslipId: selectedSlip.id, lineId, amount })
          }
          onCreateAdditional={(payload) =>
            dispatch("create_additional", {
              employeeId: selectedSlip.employeeId,
              periodId: selectedSlip.periodId,
              ...payload
            }).then(() => dispatch("recompute_run", { id: selectedSlip.runId }))
          }
          onProposeAdditional={(payload) =>
            dispatch("propose_additional_via_ctox", {
              employeeId: selectedSlip.employeeId,
              periodId: selectedSlip.periodId,
              payslipId: selectedSlip.id,
              ...payload
            })
          }
          onDeleteAdditional={(id) => dispatch("delete_additional", { id })}
        />
      )}

      {drawer === "stamm" && (
        <StammDrawer
          snap={snap}
          submoduleId={submoduleId}
          state={state}
          dispatch={dispatch}
          onClose={() => setDrawer(null)}
        />
      )}

      {drawer === "handoff" && (
        <HandoffDrawer
          snap={snap}
          state={state}
          onInstallDePack={() => dispatch("install_country_pack", { country: "DE" })}
          onExportCsv={(periodId) => {
            if (typeof window !== "undefined") {
              window.open(`/api/operations/payroll?view=export&periodId=${encodeURIComponent(periodId)}`, "_blank");
            }
          }}
          onClose={() => setDrawer(null)}
        />
      )}
    </div>
  );
}

function BottomPayslipDrawer({
  slip,
  components,
  additionals,
  submoduleId,
  state,
  onClose,
  onMarkReview,
  onMarkDraft,
  onMarkWithheld,
  onPost,
  onCancel,
  onUpdateLine,
  onCreateAdditional,
  onProposeAdditional,
  onDeleteAdditional
}: {
  slip: PayrollPayslip;
  components: PayrollComponent[];
  additionals: PayrollAdditional[];
  submoduleId: string;
  state: MutationState;
  onClose: () => void;
  onMarkReview: () => void;
  onMarkDraft: () => void;
  onMarkWithheld: () => void;
  onPost: () => void;
  onCancel: () => void;
  onUpdateLine: (lineId: string, amount: number) => void;
  onCreateAdditional: (payload: { componentId: string; amount: number; note?: string }) => void;
  onProposeAdditional: (payload: { componentId: string; amount: number; note?: string }) => void;
  onDeleteAdditional: (id: string) => void;
}) {
  const editable = slip.status === "Draft" || slip.status === "Review";
  return (
    <section className="wf2-bottom" data-testid="payroll-bottom-drawer">
      <header>
        <div>
          <span>Lohnzettel</span>
          <h2>{slip.employeeName}</h2>
        </div>
        <strong>{currency(slip.netPay, slip.currency)} netto</strong>
        <button type="button" onClick={onClose}>Schließen</button>
      </header>
      <div className="wf2-bottom-grid">
        <section>
          <h3>Komponenten</h3>
          <table className="wf2-table" style={{width:"100%",borderCollapse:"collapse",fontSize:13}}>
            <thead>
              <tr>
                <th>Komponente</th>
                <th>Typ</th>
                <th style={{ textAlign: "right" }}>Betrag</th>
              </tr>
            </thead>
            <tbody>
              {slip.lines.map((line) => (
                <tr
                  key={line.id}
                  data-context-module="operations"
                  data-context-submodule={submoduleId}
                  data-context-record-type="payroll_payslip_line"
                  data-context-record-id={line.id}
                  data-context-label={line.componentLabel}
                  data-context-skill="product_engineering/business-basic-module-development"
                >
                  <td>{line.componentLabel}</td>
                  <td>{line.type === "earning" ? "Bezug" : "Abzug"}</td>
                  <td style={{ textAlign: "right" }}>
                    {editable ? (
                      <input
                        type="number"
                        step="0.01"
                        defaultValue={line.amount}
                        onBlur={(event) => {
                          const v = Number(event.currentTarget.value);
                          if (Number.isFinite(v) && v !== line.amount) onUpdateLine(line.id, v);
                        }}
                        style={{ width: 100, textAlign: "right" }}
                      />
                    ) : (
                      currency(line.amount, slip.currency)
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr><td>Brutto</td><td /><td style={{ textAlign: "right" }}>{currency(slip.grossPay, slip.currency)}</td></tr>
              <tr><td>Abzüge</td><td /><td style={{ textAlign: "right" }}>{currency(slip.totalDeduction, slip.currency)}</td></tr>
              <tr><td><strong>Netto</strong></td><td /><td style={{ textAlign: "right" }}><strong>{currency(slip.netPay, slip.currency)}</strong></td></tr>
            </tfoot>
          </table>
          {slip.netPay < 0 && (
            <p style={{ color: "#b91c1c", marginTop: 8 }}>
              Negativer Nettowert — Buchen ist gesperrt, bis die Zeilen korrigiert sind.
            </p>
          )}
          {slip.journalEntryId && (
            <p style={{ marginTop: 8, fontSize: 12 }}>
              Journalbeleg: <code>{slip.journalEntryId}</code>
            </p>
          )}
        </section>

        <section>
          <h3>Zusatzposten ({additionals.length})</h3>
          <div className="wf2-stack">
            {additionals.map((additional) => {
              const component = components.find((c) => c.id === additional.componentId);
              return (
                <article
                  key={additional.id}
                  className="wf2-review"
                  data-context-module="operations"
                  data-context-submodule={submoduleId}
                  data-context-record-type="payroll_additional"
                  data-context-record-id={additional.id}
                  data-context-label={additional.note ?? component?.label ?? "Zusatzposten"}
                  data-context-skill="product_engineering/business-basic-module-development"
                >
                  <strong>{component?.label ?? additional.componentId}</strong>
                  <span>{currency(additional.amount, slip.currency)}{additional.note ? ` · ${additional.note}` : ""}</span>
                  <div>
                    <button type="button" onClick={() => onDeleteAdditional(additional.id)}>Löschen</button>
                  </div>
                </article>
              );
            })}
            {additionals.length === 0 && <p className="wf2-muted">Keine Zusatzposten für diese Periode.</p>}
          </div>

          {editable && (
            <form
              className="wf2-form"
              onSubmit={(event) => {
                event.preventDefault();
                const data = new FormData(event.currentTarget);
                onCreateAdditional({
                  componentId: String(data.get("componentId") ?? ""),
                  amount: Number(data.get("amount") ?? 0),
                  note: String(data.get("note") ?? "")
                });
                event.currentTarget.reset();
              }}
            >
              <h4>Neuer Zusatzposten</h4>
              <select name="componentId" required defaultValue={components[0]?.id ?? ""}>
                {components.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
              </select>
              <div className="wf2-form-grid">
                <input name="amount" type="number" step="0.01" placeholder="Betrag" required />
                <input name="note" placeholder="Notiz (optional)" />
              </div>
              <div className="wf2-form-grid">
                <button type="submit" disabled={state === "saving"}>Anlegen</button>
                <button
                  type="button"
                  onClick={(event) => {
                    const form = (event.currentTarget.closest("form") as HTMLFormElement | null);
                    if (!form) return;
                    const data = new FormData(form);
                    onProposeAdditional({
                      componentId: String(data.get("componentId") ?? ""),
                      amount: Number(data.get("amount") ?? 0),
                      note: String(data.get("note") ?? "")
                    });
                  }}
                  disabled={state === "saving"}
                >
                  Prompt CTOX
                </button>
              </div>
            </form>
          )}

          <details>
            <summary><h4 style={{ display: "inline" }}>Periodenvergleich (letzte 6)</h4></summary>
            <PeriodComparisonPanel employeeId={slip.employeeId} />
          </details>
        </section>

        <section className="wf2-bottom-actions">
          <h3>Aktionen</h3>
          <button type="button" onClick={onMarkReview} disabled={slip.status !== "Draft" && slip.status !== "Withheld"}>
            Zur Prüfung
          </button>
          <button type="button" onClick={onMarkDraft} disabled={slip.status !== "Review" && slip.status !== "Withheld"}>
            Zurück zu Entwurf
          </button>
          <button type="button" onClick={onMarkWithheld} disabled={slip.status !== "Draft" && slip.status !== "Review"}>
            Zurückstellen
          </button>
          <button type="button" onClick={onPost} disabled={slip.status !== "Review" || slip.netPay < 0}>
            Buchen
          </button>
          <button type="button" onClick={onCancel} disabled={slip.status === "Cancelled"}>
            Stornieren
          </button>
          <div className="wf2-time-fact">
            <strong>Status</strong>
            <span>{slip.status}</span>
          </div>
        </section>
      </div>
    </section>
  );
}

function StammDrawer({
  snap,
  submoduleId,
  state,
  dispatch,
  onClose
}: {
  snap: PayrollSnapshot;
  submoduleId: string;
  state: MutationState;
  dispatch: (command: PayrollCommand, payload?: Record<string, unknown>) => void;
  onClose: () => void;
}): ReactNode {
  return (
    <aside className="wf2-side-drawer left" data-testid="payroll-stamm-drawer">
      <header>
        <h2>Stammdaten</h2>
        <button type="button" onClick={onClose}>Schließen</button>
      </header>

      <section>
        <h3>Perioden</h3>
        <div className="wf2-stack">
          {snap.periods.map((period) => (
            <article
              key={period.id}
              className="wf2-review"
              data-context-module="operations"
              data-context-submodule={submoduleId}
              data-context-record-type="payroll_period"
              data-context-record-id={period.id}
              data-context-label={`Periode ${period.startDate}`}
              data-context-skill="product_engineering/business-basic-module-development"
            >
              <strong>{period.startDate} – {period.endDate}</strong>
              <span>{period.frequency} · {period.locked ? "gesperrt" : "offen"}</span>
              {!period.locked && (
                <div>
                  <button type="button" onClick={() => dispatch("lock_period", { id: period.id })} disabled={state === "saving"}>
                    Sperren
                  </button>
                </div>
              )}
            </article>
          ))}
        </div>
        <form
          className="wf2-form"
          onSubmit={(event) => {
            event.preventDefault();
            const f = new FormData(event.currentTarget);
            dispatch("create_period", {
              startDate: String(f.get("startDate") ?? ""),
              endDate: String(f.get("endDate") ?? ""),
              frequency: String(f.get("frequency") ?? "monthly")
            });
            event.currentTarget.reset();
          }}
        >
          <h4>Periode anlegen</h4>
          <div className="wf2-form-grid">
            <input name="startDate" type="date" required />
            <input name="endDate" type="date" required />
          </div>
          <select name="frequency" defaultValue="monthly">
            <option value="monthly">monthly</option>
            <option value="bi-weekly">bi-weekly</option>
            <option value="weekly">weekly</option>
          </select>
          <button type="submit" disabled={state === "saving"}>Anlegen</button>
        </form>
      </section>

      <section>
        <h3>Komponenten</h3>
        <div className="wf2-stack">
          {snap.components.map((component) => (
            <article
              key={component.id}
              className="wf2-review"
              data-context-module="operations"
              data-context-submodule={submoduleId}
              data-context-record-type="payroll_component"
              data-context-record-id={component.id}
              data-context-label={component.label}
              data-context-skill="product_engineering/business-basic-module-development"
            >
              <strong>{component.label}{component.disabled ? " · deaktiviert" : ""}</strong>
              <span>{component.code} · {component.type} · Konto {component.accountId}</span>
              <div>
                <button
                  type="button"
                  onClick={() => dispatch("update_component", { ...component, disabled: !component.disabled })}
                  disabled={state === "saving"}
                >
                  {component.disabled ? "Aktivieren" : "Deaktivieren"}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    if (typeof window !== "undefined" && !window.confirm(`Komponente '${component.label}' löschen?`)) return;
                    dispatch("delete_component", { id: component.id });
                  }}
                  disabled={state === "saving"}
                >
                  Löschen
                </button>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section>
        <h3>Strukturen</h3>
        <div className="wf2-stack">
          {snap.structures.map((structure) => (
            <article
              key={structure.id}
              className="wf2-review"
              data-context-module="operations"
              data-context-submodule={submoduleId}
              data-context-record-type="payroll_structure"
              data-context-record-id={structure.id}
              data-context-label={structure.label}
              data-context-skill="product_engineering/business-basic-module-development"
            >
              <input
                defaultValue={structure.label}
                onBlur={(event) => {
                  const v = event.currentTarget.value.trim();
                  if (v && v !== structure.label) dispatch("update_structure", { ...structure, label: v });
                }}
                style={{ width: "100%" }}
              />
              <span>{structure.frequency} · {structure.componentIds.length} Komponenten · {structure.currency}</span>
              <div>
                <button type="button" onClick={() => dispatch("duplicate_structure", { id: structure.id })} disabled={state === "saving"}>
                  Duplizieren
                </button>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section>
        <h3>Strukturzuweisungen</h3>
        <div className="wf2-stack">
          {snap.assignments.map((assignment) => {
            const employee = snap.employees.find((e) => e.id === assignment.employeeId);
            const structure = snap.structures.find((s) => s.id === assignment.structureId);
            return (
              <article
                key={assignment.id}
                className="wf2-review"
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_structure_assignment"
                data-context-record-id={assignment.id}
                data-context-label={`Zuweisung ${employee?.displayName ?? assignment.employeeId}`}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <strong>{employee?.displayName ?? assignment.employeeId}</strong>
                <span>{structure?.label ?? assignment.structureId} · ab {assignment.fromDate}{assignment.toDate ? ` bis ${assignment.toDate}` : ""}</span>
                <div className="wf2-form-grid">
                  <input
                    type="number"
                    step="0.01"
                    defaultValue={assignment.baseSalary}
                    onBlur={(event) => {
                      const v = Number(event.currentTarget.value);
                      if (Number.isFinite(v) && v !== assignment.baseSalary) {
                        dispatch("update_structure_assignment", { id: assignment.id, baseSalary: v });
                      }
                    }}
                  />
                  {!assignment.toDate && (
                    <button
                      type="button"
                      onClick={() => dispatch("end_structure_assignment", { id: assignment.id })}
                      disabled={state === "saving"}
                    >
                      Beenden
                    </button>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      </section>
    </aside>
  );
}

function HandoffDrawer({
  snap,
  state,
  onInstallDePack,
  onExportCsv,
  onClose
}: {
  snap: PayrollSnapshot;
  state: MutationState;
  onInstallDePack: () => void;
  onExportCsv: (periodId: string) => void;
  onClose: () => void;
}): ReactNode {
  const postedJournalsByPeriod = useMemo(() => {
    const map = new Map<string, { count: number; gross: number; net: number; currency: string; startDate: string; endDate: string }>();
    for (const slip of snap.payslips) {
      if (slip.status !== "Posted") continue;
      const existing = map.get(slip.periodId);
      if (existing) {
        existing.count += 1;
        existing.gross += slip.grossPay;
        existing.net += slip.netPay;
      } else {
        map.set(slip.periodId, {
          count: 1,
          gross: slip.grossPay,
          net: slip.netPay,
          currency: slip.currency,
          startDate: slip.startDate,
          endDate: slip.endDate
        });
      }
    }
    return [...map.entries()].sort((a, b) => b[1].endDate.localeCompare(a[1].endDate));
  }, [snap.payslips]);

  return (
    <aside className="wf2-side-drawer right" data-testid="payroll-handoff-drawer">
      <header>
        <h2>Übergaben</h2>
        <button type="button" onClick={onClose}>Schließen</button>
      </header>

      <section>
        <h3>Country Pack</h3>
        <p className="wf2-muted">DE 2026 (vereinfacht): KV, RV, AV, PV, Lohnsteuer, Soli — Detail siehe RFC 0007.</p>
        <button type="button" onClick={onInstallDePack} disabled={state === "saving"}>
          DE-Pack installieren
        </button>
      </section>

      <section>
        <h3>Gebuchte Lohnläufe</h3>
        <div className="wf2-stack">
          {postedJournalsByPeriod.map(([periodId, summary]) => (
            <article className="wf2-review" key={periodId}>
              <strong>{summary.startDate} – {summary.endDate}</strong>
              <span>{summary.count} Slips · brutto {currency(summary.gross, summary.currency)} · netto {currency(summary.net, summary.currency)}</span>
              <div>
                <button type="button" onClick={() => onExportCsv(periodId)}>
                  CSV-Export
                </button>
              </div>
            </article>
          ))}
          {postedJournalsByPeriod.length === 0 && <p className="wf2-muted">Noch keine geposteten Lohnläufe.</p>}
        </div>
      </section>

      <section>
        <h3>Cross-Module (queued)</h3>
        <div className="wf2-stack">
          <article className="wf2-review">
            <strong>US-45 Ledger / DATEV</strong>
            <span>Wartet auf <code>business/ledger</code>-Render und DATEV-Export-Test.</span>
          </article>
          <article className="wf2-review">
            <strong>US-47 SEPA-Vorschlag</strong>
            <span>Wartet auf <code>business/payments</code>-Modul.</span>
          </article>
        </div>
      </section>
    </aside>
  );
}

function PeriodComparisonPanel({ employeeId }: { employeeId: string }) {
  const [rows, setRows] = useState<{ start: string; end: string; gross: number; net: number; totalDeduction: number }[] | null>(null);
  const [grossDeltas, setGrossDeltas] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/operations/payroll?view=comparison&employeeId=${encodeURIComponent(employeeId)}&periods=6`);
        const data = await res.json();
        if (cancelled) return;
        if (!data.ok) throw new Error(data.error ?? "comparison_failed");
        setRows(data.comparison.rows);
        setGrossDeltas(data.comparison.grossDeltas ?? []);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [employeeId]);
  if (error) return <p style={{ color: "#b91c1c", fontSize: 12 }}>{error}</p>;
  if (!rows) return <p style={{ color: "#6b7280", fontSize: 12 }}>Lade…</p>;
  if (rows.length === 0) return <p style={{ color: "#6b7280", fontSize: 12 }}>Keine geposteten Lohnzettel im Vergleichszeitraum.</p>;
  return (
    <table className="wf2-table" style={{width:"100%",borderCollapse:"collapse",fontSize:13}}>
      <thead>
        <tr>
          <th>Periode</th>
          <th style={{ textAlign: "right" }}>Brutto</th>
          <th style={{ textAlign: "right" }}>Netto</th>
          <th style={{ textAlign: "right" }}>Δ Brutto</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row, idx) => (
          <tr key={`${row.start}-${row.end}`}>
            <td>{row.start} – {row.end}</td>
            <td style={{ textAlign: "right" }}>{row.gross.toFixed(2)}</td>
            <td style={{ textAlign: "right" }}>{row.net.toFixed(2)}</td>
            <td style={{ textAlign: "right" }}>{idx === 0 ? "—" : (grossDeltas[idx - 1] ?? 0).toFixed(2)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function currency(value: number, code: string) {
  try {
    return new Intl.NumberFormat("de-DE", { style: "currency", currency: code, maximumFractionDigits: 2 }).format(value);
  } catch {
    return value.toFixed(2) + " " + code;
  }
}

function initials(name: string) {
  return name
    .split(" ")
    .map((part) => part[0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
}
