"use client";

import { useEffect, useMemo, useState, type CSSProperties, type FormEvent } from "react";
import type {
  PayrollCommand,
  PayrollSnapshot
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

const copy = {
  intakeTitle: "Stamm und Periode",
  centerTitle: "Lohnläufe",
  drawerTitle: "Lohnabrechnung Detail",
  queueTitle: "Audit",
  status: "Status",
  period: "Periode",
  employee: "Mitarbeiter",
  gross: "Brutto",
  deductions: "Abzüge",
  net: "Netto",
  postingDate: "Buchungstag",
  startRun: "Run abschicken",
  recompute: "Neu berechnen",
  cancelRun: "Run abbrechen",
  newRun: "Neuen Run anlegen",
  toReview: "Zur Prüfung",
  withhold: "Zurückstellen",
  postSlip: "Buchen",
  cancelSlip: "Stornieren",
  saving: "Speichert ...",
  saved: "Gespeichert",
  errorPrefix: "Fehler:",
  noRun: "Kein Run ausgewählt",
  noSlip: "Wähle einen Lohnzettel aus, um Details zu sehen.",
  components: "Komponenten",
  audit: "Statuswechsel",
  posted: "Gebucht",
  draft: "Entwurf",
  review: "Prüfung",
  withheld: "Zurückgestellt",
  cancelled: "Storniert"
};

export function PayrollWorkbench({ query, snapshot, submoduleId = "runs" }: PayrollWorkbenchProps) {
  const [state, setState] = useState<MutationState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | undefined>(snapshot.runs[0]?.id);
  const [selectedSlipId, setSelectedSlipId] = useState<string | undefined>(query.recordId ?? snapshot.payslips[0]?.id);
  const [snap, setSnap] = useState<PayrollSnapshot>(snapshot);

  const runs = snap.runs;
  const slipsForRun = useMemo(
    () => snap.payslips.filter((slip) => slip.runId === selectedRunId),
    [snap.payslips, selectedRunId]
  );
  const selectedSlip = useMemo(
    () => snap.payslips.find((slip) => slip.id === selectedSlipId),
    [snap.payslips, selectedSlipId]
  );

  async function dispatch(command: PayrollCommand, payload: Record<string, unknown> = {}) {
    setState("saving");
    setError(null);
    try {
      const res = await fetch("/api/payroll", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ command, payload })
      });
      const data = await res.json();
      if (!data.ok) throw new Error(data.error ?? "payroll_command_failed");
      setSnap(data.snapshot as PayrollSnapshot);
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
    <div style={shell}>
      <aside style={zoneIntake}>
        <h2 style={zoneTitle}>{copy.intakeTitle}</h2>
        <section>
          <h3 style={subTitle}>{copy.period}</h3>
          <ul style={listReset}>
            {snap.periods.map((period) => (
              <li
                key={period.id}
                style={listItem}
                data-context-module="payroll"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_period"
                data-context-record-id={period.id}
                data-context-label={`Periode ${period.startDate}`}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <span>{period.startDate} – {period.endDate}</span>
                <span style={badgeStatus(period.locked ? "Posted" : "Draft")}>{period.locked ? "gesperrt" : "offen"}</span>
                {!period.locked && (
                  <button
                    type="button"
                    style={smallButton}
                    onClick={() => dispatch("lock_period", { id: period.id })}
                    disabled={state === "saving"}
                  >
                    Sperren
                  </button>
                )}
              </li>
            ))}
          </ul>
          <details style={formStack}>
            <summary style={subTitle}>Periode anlegen</summary>
            <form
              onSubmit={async (event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                await dispatch("create_period", {
                  startDate: String(form.get("startDate") ?? ""),
                  endDate: String(form.get("endDate") ?? ""),
                  frequency: String(form.get("frequency") ?? "monthly")
                });
                event.currentTarget.reset();
              }}
              style={formStack}
            >
              <label style={formLabel}>Start <input type="date" name="startDate" required style={formInput} /></label>
              <label style={formLabel}>Ende <input type="date" name="endDate" required style={formInput} /></label>
              <label style={formLabel}>Frequenz
                <select name="frequency" defaultValue="monthly" style={formInput}>
                  <option value="monthly">monthly</option>
                  <option value="bi-weekly">bi-weekly</option>
                  <option value="weekly">weekly</option>
                </select>
              </label>
              <button type="submit" style={primaryButton} disabled={state === "saving"}>Anlegen</button>
            </form>
          </details>
        </section>
        <section>
          <h3 style={subTitle}>{copy.components}</h3>
          <ul style={listReset}>
            {snap.components.map((component) => (
              <li
                key={component.id}
                style={listItem}
                data-context-module="payroll"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_component"
                data-context-record-id={component.id}
                data-context-label={component.label}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <span>
                  {component.label}
                  {component.disabled && <span style={muted}> · deaktiviert</span>}
                </span>
                <span style={muted}>{component.code} · {component.type}</span>
                <button
                  type="button"
                  style={smallButton}
                  onClick={() => dispatch("update_component", { ...component, disabled: !component.disabled })}
                  disabled={state === "saving"}
                >
                  {component.disabled ? "Aktivieren" : "Deaktivieren"}
                </button>
                <button
                  type="button"
                  style={dangerButton}
                  onClick={async () => {
                    if (!confirm(`Komponente '${component.label}' löschen? Bei Verwendung in aktiver Struktur wird Aktion abgewiesen.`)) return;
                    await dispatch("delete_component", { id: component.id });
                  }}
                  disabled={state === "saving"}
                >
                  Löschen
                </button>
              </li>
            ))}
          </ul>
          <details style={formStack}>
            <summary style={subTitle}>Komponente anlegen</summary>
            <form
              onSubmit={async (event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                const formulaKind = String(form.get("formulaKind") ?? "fix");
                await dispatch("create_component", {
                  code: String(form.get("code") ?? ""),
                  label: String(form.get("label") ?? ""),
                  type: String(form.get("type") ?? "earning"),
                  taxable: form.get("taxable") === "on",
                  dependsOnPaymentDays: form.get("dependsOnPaymentDays") === "on",
                  accountId: String(form.get("accountId") ?? ""),
                  formulaKind,
                  formulaAmount: formulaKind === "fix" ? Number(form.get("formulaAmount") ?? 0) : undefined,
                  formulaBase: formulaKind === "percent_of" ? String(form.get("formulaBase") ?? "base_salary") : undefined,
                  formulaPercent: formulaKind === "percent_of" ? Number(form.get("formulaPercent") ?? 0) : undefined,
                  formulaExpression: formulaKind === "formula" ? String(form.get("formulaExpression") ?? "") : undefined,
                  sequence: Number(form.get("sequence") ?? 100)
                });
                event.currentTarget.reset();
              }}
              style={formStack}
            >
              <label style={formLabel}>Code <input name="code" required style={formInput} /></label>
              <label style={formLabel}>Bezeichnung <input name="label" required style={formInput} /></label>
              <label style={formLabel}>Typ
                <select name="type" defaultValue="earning" style={formInput}>
                  <option value="earning">earning</option>
                  <option value="deduction">deduction</option>
                </select>
              </label>
              <label style={formLabel}>GL‑Konto <input name="accountId" required style={formInput} /></label>
              <label style={formLabel}>Sequenz <input name="sequence" type="number" defaultValue={100} style={formInput} /></label>
              <label style={formLabel}><input type="checkbox" name="taxable" defaultChecked /> taxable</label>
              <label style={formLabel}><input type="checkbox" name="dependsOnPaymentDays" /> depends on payment days</label>
              <label style={formLabel}>Formel‑Art
                <select name="formulaKind" defaultValue="fix" style={formInput}>
                  <option value="fix">fix</option>
                  <option value="percent_of">percent_of</option>
                  <option value="formula">formula</option>
                </select>
              </label>
              <label style={formLabel}>fix Betrag <input name="formulaAmount" type="number" step="0.01" style={formInput} /></label>
              <label style={formLabel}>percent_of Basis <input name="formulaBase" defaultValue="base_salary" style={formInput} /></label>
              <label style={formLabel}>percent_of % <input name="formulaPercent" type="number" step="0.01" style={formInput} /></label>
              <label style={formLabel}>formula Ausdruck <input name="formulaExpression" placeholder="base_salary * 0.05" style={formInput} /></label>
              <button type="submit" style={primaryButton} disabled={state === "saving"}>Anlegen</button>
            </form>
          </details>
        </section>
        <section>
          <h3 style={subTitle}>Strukturen</h3>
          <ul style={listReset}>
            {snap.structures.map((structure) => (
              <li
                key={structure.id}
                style={listItem}
                data-context-module="payroll"
                data-context-submodule={submoduleId}
                data-context-record-type="payroll_structure"
                data-context-record-id={structure.id}
                data-context-label={structure.label}
                data-context-skill="product_engineering/business-basic-module-development"
              >
                <input
                  defaultValue={structure.label}
                  onBlur={async (event) => {
                    const v = event.currentTarget.value.trim();
                    if (v && v !== structure.label) {
                      await dispatch("update_structure", { ...structure, label: v });
                    }
                  }}
                  style={{ ...formInput, flex: 1 }}
                />
                <span style={muted}>{structure.frequency} · {structure.componentIds.length}</span>
                <button
                  type="button"
                  style={smallButton}
                  onClick={() => dispatch("duplicate_structure", { id: structure.id })}
                  disabled={state === "saving"}
                >
                  Duplizieren
                </button>
              </li>
            ))}
          </ul>
        </section>
        <section>
          <h3 style={subTitle}>Strukturzuweisungen</h3>
          <ul style={listReset}>
            {snap.assignments.map((assignment) => {
              const employee = snap.employees.find((e) => e.id === assignment.employeeId);
              const structure = snap.structures.find((s) => s.id === assignment.structureId);
              return (
                <li
                  key={assignment.id}
                  style={listItem}
                  data-context-module="payroll"
                  data-context-submodule={submoduleId}
                  data-context-record-type="payroll_structure_assignment"
                  data-context-record-id={assignment.id}
                  data-context-label={`Zuweisung ${employee?.displayName ?? assignment.employeeId}`}
                  data-context-skill="product_engineering/business-basic-module-development"
                >
                  <span>{employee?.displayName ?? assignment.employeeId}</span>
                  <span style={muted}>{structure?.label ?? assignment.structureId}</span>
                  <input
                    type="number"
                    step="0.01"
                    defaultValue={assignment.baseSalary}
                    onBlur={async (event) => {
                      const v = Number(event.currentTarget.value);
                      if (Number.isFinite(v) && v !== assignment.baseSalary) {
                        await dispatch("update_structure_assignment", { id: assignment.id, baseSalary: v });
                      }
                    }}
                    style={{ ...formInput, width: 90 }}
                  />
                  {!assignment.toDate && (
                    <button
                      type="button"
                      style={smallButton}
                      onClick={() => dispatch("end_structure_assignment", { id: assignment.id })}
                      disabled={state === "saving"}
                    >
                      Beenden
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
          <details style={formStack}>
            <summary style={subTitle}>Zuweisung anlegen</summary>
            <form
              onSubmit={async (event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                await dispatch("create_structure_assignment", {
                  employeeId: String(form.get("employeeId") ?? ""),
                  structureId: String(form.get("structureId") ?? ""),
                  baseSalary: Number(form.get("baseSalary") ?? 0),
                  currency: String(form.get("currency") ?? "EUR"),
                  fromDate: String(form.get("fromDate") ?? "")
                });
                event.currentTarget.reset();
              }}
              style={formStack}
            >
              <label style={formLabel}>Mitarbeiter
                <select name="employeeId" required style={formInput}>
                  {snap.employees.map((e) => (
                    <option key={e.id} value={e.id}>{e.displayName}</option>
                  ))}
                </select>
              </label>
              <label style={formLabel}>Struktur
                <select name="structureId" required style={formInput}>
                  {snap.structures.map((s) => (
                    <option key={s.id} value={s.id}>{s.label}</option>
                  ))}
                </select>
              </label>
              <label style={formLabel}>Grundgehalt <input name="baseSalary" type="number" step="0.01" required style={formInput} /></label>
              <label style={formLabel}>Währung <input name="currency" defaultValue="EUR" style={formInput} /></label>
              <label style={formLabel}>ab <input name="fromDate" type="date" required style={formInput} /></label>
              <button type="submit" style={primaryButton} disabled={state === "saving"}>Anlegen</button>
            </form>
          </details>
        </section>
        <section>
          <h3 style={subTitle}>{copy.newRun}</h3>
          <form onSubmit={handleNewRun} style={formStack}>
            <label style={formLabel}>
              {copy.period}
              <select name="periodId" defaultValue={snap.periods[0]?.id ?? ""} style={formInput}>
                {snap.periods.map((period) => (
                  <option key={period.id} value={period.id}>
                    {period.startDate} – {period.endDate}
                  </option>
                ))}
              </select>
            </label>
            <label style={formLabel}>
              Verbindlichkeitskonto
              <input name="payableAccountId" defaultValue="1755" style={formInput} />
            </label>
            <button type="submit" style={primaryButton} disabled={state === "saving"}>
              {state === "saving" ? copy.saving : copy.newRun}
            </button>
          </form>
        </section>
      </aside>

      <section style={zoneCenter}>
        <header style={zoneHeader}>
          <h2 style={zoneTitle}>{copy.centerTitle}</h2>
          <div style={statusRow}>
            <button
              type="button"
              style={smallButton}
              onClick={() => dispatch("install_country_pack", { country: "DE" })}
              disabled={state === "saving"}
              title="Lohnsteuer/SV/Soli für DE installieren"
            >
              DE‑Pack installieren
            </button>
            {selectedRunId && (
              <>
                <button
                  type="button"
                  style={smallButton}
                  onClick={() => dispatch("bulk_mark_review", { id: selectedRunId })}
                  disabled={state === "saving"}
                >
                  Alle zur Prüfung
                </button>
                <button
                  type="button"
                  style={primaryButton}
                  onClick={() => dispatch("bulk_post_run", { id: selectedRunId })}
                  disabled={state === "saving"}
                >
                  Alle buchen
                </button>
                <a
                  href={`/api/payroll?view=export&periodId=${snap.runs.find((r) => r.id === selectedRunId)?.periodId ?? ""}`}
                  style={{ ...smallButton, textDecoration: "none", display: "inline-block" }}
                  download
                >
                  CSV‑Export
                </a>
              </>
            )}
            {state === "saved" && <span style={badgeOk}>{copy.saved}</span>}
            {state === "error" && <span style={badgeError}>{copy.errorPrefix} {error}</span>}
          </div>
        </header>

        <table style={runTable}>
          <thead>
            <tr>
              <th style={th}>{copy.period}</th>
              <th style={th}>Frequenz</th>
              <th style={th}>{copy.status}</th>
              <th style={th}>Slips</th>
              <th style={th}>Aktionen</th>
            </tr>
          </thead>
          <tbody>
            {runs.length === 0 && (
              <tr>
                <td colSpan={5} style={emptyRow}>{copy.noRun}</td>
              </tr>
            )}
            {runs.map((run) => {
              const period = snap.periods.find((p) => p.id === run.periodId);
              const slipCount = snap.payslips.filter((s) => s.runId === run.id).length;
              const selected = run.id === selectedRunId;
              return (
                <tr
                  key={run.id}
                  style={selected ? rowSelected : rowDefault}
                  onClick={() => setSelectedRunId(run.id)}
                  data-context-module="payroll"
                  data-context-submodule={submoduleId}
                  data-context-record-type="payroll_run"
                  data-context-record-id={run.id}
                  data-context-label={`Lohnlauf ${period?.startDate ?? "?"}`}
                  data-context-skill="product_engineering/business-basic-module-development"
                >
                  <td style={td}>{period?.startDate} – {period?.endDate}</td>
                  <td style={td}>{run.frequency}</td>
                  <td style={td}><span style={badgeStatus(run.status)}>{run.status}</span></td>
                  <td style={td}>{slipCount}</td>
                  <td style={td}>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("queue_run", { id: run.id }); }}
                      disabled={run.status !== "Draft" && run.status !== "Failed"}
                    >
                      {copy.startRun}
                    </button>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("recompute_run", { id: run.id }); }}
                      disabled={run.status === "Cancelled"}
                    >
                      {copy.recompute}
                    </button>
                    <button
                      type="button"
                      style={dangerButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("cancel_run", { id: run.id }); }}
                      disabled={run.status === "Cancelled"}
                    >
                      {copy.cancelRun}
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>

        <h3 style={subTitle}>Lohnzettel</h3>
        <table style={runTable}>
          <thead>
            <tr>
              <th style={th}>{copy.employee}</th>
              <th style={th}>{copy.gross}</th>
              <th style={th}>{copy.deductions}</th>
              <th style={th}>{copy.net}</th>
              <th style={th}>{copy.status}</th>
              <th style={th}>Aktionen</th>
            </tr>
          </thead>
          <tbody>
            {slipsForRun.length === 0 && (
              <tr>
                <td colSpan={6} style={emptyRow}>{copy.noRun}</td>
              </tr>
            )}
            {slipsForRun.map((slip) => {
              const selected = slip.id === selectedSlipId;
              return (
                <tr
                  key={slip.id}
                  style={selected ? rowSelected : rowDefault}
                  onClick={() => setSelectedSlipId(slip.id)}
                  data-context-module="payroll"
                  data-context-submodule={submoduleId}
                  data-context-record-type="payroll_payslip"
                  data-context-record-id={slip.id}
                  data-context-label={`Lohnabrechnung ${slip.employeeName}`}
                  data-context-skill="product_engineering/business-basic-module-development"
                >
                  <td style={td}>{slip.employeeName}</td>
                  <td style={tdNumber}>{currency(slip.grossPay, slip.currency)}</td>
                  <td style={tdNumber}>{currency(slip.totalDeduction, slip.currency)}</td>
                  <td style={tdNumber}>{currency(slip.netPay, slip.currency)}</td>
                  <td style={td}><span style={badgeStatus(slip.status)}>{slip.status}</span></td>
                  <td style={td}>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("mark_payslip_review", { id: slip.id }); }}
                      disabled={slip.status !== "Draft" && slip.status !== "Withheld"}
                    >
                      {copy.toReview}
                    </button>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("mark_payslip_draft", { id: slip.id }); }}
                      disabled={slip.status !== "Review" && slip.status !== "Withheld"}
                    >
                      Zurück zu Entwurf
                    </button>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("mark_payslip_withheld", { id: slip.id }); }}
                      disabled={!(slip.status === "Draft" || slip.status === "Review")}
                    >
                      {copy.withhold}
                    </button>
                    <button
                      type="button"
                      style={primaryButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("post_payslip", { id: slip.id }); }}
                      disabled={slip.status !== "Review" || slip.netPay < 0}
                      title={slip.netPay < 0 ? "Negativer Nettowert" : undefined}
                    >
                      {copy.postSlip}
                    </button>
                    <button
                      type="button"
                      style={dangerButton}
                      onClick={(e) => { e.stopPropagation(); dispatch("cancel_payslip", { id: slip.id }); }}
                      disabled={slip.status === "Cancelled"}
                    >
                      {copy.cancelSlip}
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      <aside style={zoneInspector}>
        <h2 style={zoneTitle}>{copy.drawerTitle}</h2>
        {!selectedSlip && <p style={muted}>{copy.noSlip}</p>}
        {selectedSlip && (
          <div>
            <p style={inspectorLine}>{copy.employee}: <strong>{selectedSlip.employeeName}</strong></p>
            <p style={inspectorLine}>{copy.period}: {selectedSlip.startDate} – {selectedSlip.endDate}</p>
            <p style={inspectorLine}>Bezahlte Tage: {selectedSlip.paymentDays}</p>
            <p style={inspectorLine}>{copy.status}: <span style={badgeStatus(selectedSlip.status)}>{selectedSlip.status}</span></p>
            {selectedSlip.journalEntryId && (
              <p style={inspectorLine}>Journalbeleg: <code>{selectedSlip.journalEntryId}</code></p>
            )}
            <table style={runTable}>
              <thead>
                <tr>
                  <th style={th}>Komponente</th>
                  <th style={th}>Typ</th>
                  <th style={th}>Betrag</th>
                </tr>
              </thead>
              <tbody>
                {selectedSlip.lines.map((line) => {
                  const editable = selectedSlip.status === "Draft" || selectedSlip.status === "Review";
                  return (
                    <tr
                      key={line.id}
                      data-context-module="payroll"
                      data-context-submodule={submoduleId}
                      data-context-record-type="payroll_payslip_line"
                      data-context-record-id={line.id}
                      data-context-label={line.componentLabel}
                      data-context-skill="product_engineering/business-basic-module-development"
                    >
                      <td style={td}>{line.componentLabel}</td>
                      <td style={td}>{line.type === "earning" ? "Bezug" : "Abzug"}</td>
                      <td style={tdNumber}>
                        {editable ? (
                          <input
                            type="number"
                            step="0.01"
                            defaultValue={line.amount}
                            onBlur={async (event) => {
                              const v = Number(event.currentTarget.value);
                              if (Number.isFinite(v) && v !== line.amount) {
                                await dispatch("update_payslip_line", { payslipId: selectedSlip.id, lineId: line.id, amount: v });
                              }
                            }}
                            style={{ ...formInput, width: 110, textAlign: "right" }}
                          />
                        ) : (
                          currency(line.amount, selectedSlip.currency)
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
              <tfoot>
                <tr><td style={tfTd}>Brutto</td><td style={tfTd} /><td style={tfTdNumber}>{currency(selectedSlip.grossPay, selectedSlip.currency)}</td></tr>
                <tr><td style={tfTd}>Abzüge</td><td style={tfTd} /><td style={tfTdNumber}>{currency(selectedSlip.totalDeduction, selectedSlip.currency)}</td></tr>
                <tr><td style={tfTd}><strong>Netto</strong></td><td style={tfTd} /><td style={tfTdNumber}><strong>{currency(selectedSlip.netPay, selectedSlip.currency)}</strong></td></tr>
              </tfoot>
            </table>
            {selectedSlip.netPay < 0 && (
              <p style={{ ...badgeError, marginTop: 8 }}>
                Negativer Nettowert — Buchen ist gesperrt, bis die Zeilen korrigiert sind.
              </p>
            )}
            <h3 style={subTitle}>Zusatzposten ({snap.additionals.filter((a) => a.employeeId === selectedSlip.employeeId && a.periodId === selectedSlip.periodId).length})</h3>
            <ul style={listReset}>
              {snap.additionals
                .filter((a) => a.employeeId === selectedSlip.employeeId && a.periodId === selectedSlip.periodId)
                .map((additional) => {
                  const component = snap.components.find((c) => c.id === additional.componentId);
                  return (
                    <li
                      key={additional.id}
                      style={listItem}
                      data-context-module="payroll"
                      data-context-submodule={submoduleId}
                      data-context-record-type="payroll_additional"
                      data-context-record-id={additional.id}
                      data-context-label={`${additional.note ?? component?.label ?? "Zusatzposten"}`}
                      data-context-skill="product_engineering/business-basic-module-development"
                    >
                      <span>{component?.label ?? additional.componentId}</span>
                      <span>{currency(additional.amount, selectedSlip.currency)}</span>
                      <button type="button" style={smallButton} onClick={() => dispatch("delete_additional", { id: additional.id })}>Löschen</button>
                    </li>
                  );
                })}
            </ul>
            {selectedSlip.status !== "Posted" && selectedSlip.status !== "Cancelled" && (
              <details style={formStack}>
                <summary style={subTitle}>Zusatzposten anlegen</summary>
                <form
                  onSubmit={async (event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    await dispatch("create_additional", {
                      employeeId: selectedSlip.employeeId,
                      periodId: selectedSlip.periodId,
                      componentId: String(form.get("componentId") ?? ""),
                      amount: Number(form.get("amount") ?? 0),
                      note: String(form.get("note") ?? "")
                    });
                    event.currentTarget.reset();
                    await dispatch("recompute_run", { id: selectedSlip.runId });
                  }}
                  style={formStack}
                >
                  <label style={formLabel}>Komponente
                    <select name="componentId" required style={formInput}>
                      {snap.components.map((c) => (
                        <option key={c.id} value={c.id}>{c.label}</option>
                      ))}
                    </select>
                  </label>
                  <label style={formLabel}>Betrag <input name="amount" type="number" step="0.01" required style={formInput} /></label>
                  <label style={formLabel}>Notiz <input name="note" style={formInput} /></label>
                  <div style={{ display: "flex", gap: 4 }}>
                    <button type="submit" style={primaryButton} disabled={state === "saving"}>Anlegen</button>
                    <button
                      type="button"
                      style={smallButton}
                      onClick={async (event) => {
                        const form = (event.currentTarget.closest("form") as HTMLFormElement | null);
                        if (!form) return;
                        const data = new FormData(form);
                        await dispatch("propose_additional_via_ctox", {
                          employeeId: selectedSlip.employeeId,
                          periodId: selectedSlip.periodId,
                          payslipId: selectedSlip.id,
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
              </details>
            )}
            <details style={formStack}>
              <summary style={subTitle}>Periodenvergleich (letzte 6)</summary>
              <PeriodComparisonPanel employeeId={selectedSlip.employeeId} />
            </details>
            {selectedSlip.status === "Cancelled" && snap.postedJournals.some((p) => p.payslipId === selectedSlip.id && p.id.endsWith("_reversal")) && (
              <p style={{ marginTop: 8, fontSize: 12, color: "#a16207" }}>
                Stornierungsbeleg: <code>{snap.postedJournals.find((p) => p.payslipId === selectedSlip.id && p.id.endsWith("_reversal"))?.id}</code>
              </p>
            )}
          </div>
        )}
        <h3 style={subTitle}>{copy.queueTitle}</h3>
        <ul style={listReset}>
          {snap.audit.slice(-12).reverse().map((entry) => (
            <li key={entry.id} style={auditEntry}>
              <span style={muted}>{entry.at.slice(11, 16)}</span>
              <span>{entry.entityType} {entry.fromStatus} → {entry.toStatus}</span>
              {entry.note && <span style={muted}>{entry.note}</span>}
            </li>
          ))}
        </ul>
      </aside>
    </div>
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
        const res = await fetch(`/api/payroll?view=comparison&employeeId=${encodeURIComponent(employeeId)}&periods=6`);
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
  if (error) return <p style={{ ...badgeError, fontSize: 12 }}>{error}</p>;
  if (!rows) return <p style={{ color: "#6b7280", fontSize: 12 }}>Lade Periodenvergleich…</p>;
  if (rows.length === 0) return <p style={{ color: "#6b7280", fontSize: 12 }}>Keine geposteten Lohnzettel im Vergleichszeitraum.</p>;
  return (
    <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
      <thead>
        <tr>
          <th style={{ textAlign: "left", padding: "4px 6px", borderBottom: "1px solid #d1d5db" }}>Periode</th>
          <th style={{ textAlign: "right", padding: "4px 6px", borderBottom: "1px solid #d1d5db" }}>Brutto</th>
          <th style={{ textAlign: "right", padding: "4px 6px", borderBottom: "1px solid #d1d5db" }}>Netto</th>
          <th style={{ textAlign: "right", padding: "4px 6px", borderBottom: "1px solid #d1d5db" }}>Δ Brutto</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row, idx) => (
          <tr key={`${row.start}-${row.end}`}>
            <td style={{ padding: "4px 6px" }}>{row.start} – {row.end}</td>
            <td style={{ padding: "4px 6px", textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{row.gross.toFixed(2)}</td>
            <td style={{ padding: "4px 6px", textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{row.net.toFixed(2)}</td>
            <td style={{ padding: "4px 6px", textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{idx === 0 ? "—" : (grossDeltas[idx - 1] ?? 0).toFixed(2)}</td>
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

const shell: CSSProperties = {
  display: "grid",
  gridTemplateColumns: "260px minmax(0, 1fr) 360px",
  gap: 12,
  alignItems: "start"
};
const zoneIntake: CSSProperties = { borderRight: "1px solid var(--border, #e5e7eb)", padding: "12px 16px", fontSize: 13 };
const zoneCenter: CSSProperties = { padding: "12px 16px", overflow: "hidden" };
const zoneInspector: CSSProperties = { borderLeft: "1px solid var(--border, #e5e7eb)", padding: "12px 16px", fontSize: 13 };
const zoneTitle: CSSProperties = { fontSize: 13, textTransform: "uppercase", letterSpacing: 0.4, color: "var(--muted, #6b7280)", marginBottom: 8 };
const subTitle: CSSProperties = { fontSize: 12, textTransform: "uppercase", letterSpacing: 0.4, color: "var(--muted, #6b7280)", margin: "12px 0 6px" };
const zoneHeader: CSSProperties = { display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 };
const statusRow: CSSProperties = { display: "flex", gap: 8 };
const listReset: CSSProperties = { listStyle: "none", padding: 0, margin: 0 };
const listItem: CSSProperties = { display: "flex", justifyContent: "space-between", padding: "4px 0", borderBottom: "1px solid var(--border-soft, #f1f5f9)" };
const muted: CSSProperties = { color: "var(--muted, #6b7280)", fontSize: 12 };
const formStack: CSSProperties = { display: "flex", flexDirection: "column", gap: 6 };
const formLabel: CSSProperties = { display: "flex", flexDirection: "column", fontSize: 12, gap: 2 };
const formInput: CSSProperties = { padding: "4px 6px", border: "1px solid var(--border, #d1d5db)", borderRadius: 4, fontSize: 13 };
const primaryButton: CSSProperties = { background: "var(--accent, #2563eb)", color: "white", padding: "4px 10px", border: 0, borderRadius: 4, fontSize: 12, cursor: "pointer", marginRight: 4 };
const smallButton: CSSProperties = { background: "var(--surface, #f3f4f6)", color: "var(--text, #111827)", padding: "4px 8px", border: "1px solid var(--border, #d1d5db)", borderRadius: 4, fontSize: 12, cursor: "pointer", marginRight: 4 };
const dangerButton: CSSProperties = { background: "transparent", color: "#b91c1c", padding: "4px 8px", border: "1px solid #fecaca", borderRadius: 4, fontSize: 12, cursor: "pointer", marginRight: 4 };
const runTable: CSSProperties = { width: "100%", borderCollapse: "collapse", fontSize: 13 };
const th: CSSProperties = { textAlign: "left", borderBottom: "1px solid var(--border, #d1d5db)", padding: "6px 8px", fontWeight: 600, color: "var(--muted, #6b7280)" };
const td: CSSProperties = { borderBottom: "1px solid var(--border-soft, #f1f5f9)", padding: "6px 8px", verticalAlign: "top" };
const tdNumber: CSSProperties = { ...td, textAlign: "right", fontVariantNumeric: "tabular-nums" };
const tfTd: CSSProperties = { padding: "6px 8px", borderTop: "1px solid var(--border, #d1d5db)" };
const tfTdNumber: CSSProperties = { ...tfTd, textAlign: "right", fontVariantNumeric: "tabular-nums" };
const rowDefault: CSSProperties = { cursor: "pointer" };
const rowSelected: CSSProperties = { background: "var(--surface-hover, #eef2ff)", cursor: "pointer" };
const emptyRow: CSSProperties = { padding: 16, color: "var(--muted, #6b7280)", textAlign: "center" };
const inspectorLine: CSSProperties = { margin: "4px 0", fontSize: 13 };
const auditEntry: CSSProperties = { display: "flex", flexDirection: "column", gap: 0, padding: "4px 0", borderBottom: "1px solid var(--border-soft, #f1f5f9)", fontSize: 12 };

function badgeStatus(status: string): CSSProperties {
  const map: Record<string, string> = {
    Draft: "#6b7280",
    Review: "#2563eb",
    Posted: "#16a34a",
    Cancelled: "#b91c1c",
    Withheld: "#a16207",
    Submitted: "#16a34a",
    Queued: "#2563eb",
    Failed: "#b91c1c",
    Running: "#2563eb"
  };
  const fg = map[status] ?? "#6b7280";
  return { display: "inline-block", padding: "2px 6px", borderRadius: 4, color: fg, border: `1px solid ${fg}`, fontSize: 11, fontWeight: 600 };
}
const badgeOk: CSSProperties = { color: "#16a34a", fontSize: 12 };
const badgeError: CSSProperties = { color: "#b91c1c", fontSize: 12 };
