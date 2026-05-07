import type { WorkSurfacePanelState } from "@ctox-business/ui";
import { getPayrollSnapshot } from "../lib/payroll-runtime";
import { PayrollWorkbench } from "./payroll-workbench";

type QueryState = {
  drawer?: string;
  filePath?: string;
  group?: string;
  locale?: string;
  panel?: string;
  recordId?: string;
  runbookId?: string;
  skillbookId?: string;
  skillId?: string;
  theme?: string;
};

export async function PayrollWorkspace({
  submoduleId,
  query
}: {
  submoduleId: string;
  query: QueryState;
}) {
  const snapshot = await getPayrollSnapshot();
  return <PayrollWorkbench query={query} snapshot={snapshot} submoduleId={submoduleId} />;
}

export async function PayrollPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const recordId = panelState?.recordId ?? query.recordId;
  if (!recordId) return null;
  const snapshot = await getPayrollSnapshot();
  const slip = snapshot.payslips.find((s) => s.id === recordId);
  const run = snapshot.runs.find((r) => r.id === recordId);
  const additional = snapshot.additionals.find((a) => a.id === recordId);
  const component = snapshot.components.find((c) => c.id === recordId);
  const period = snapshot.periods.find((p) => p.id === recordId);
  const assignment = snapshot.assignments.find((a) => a.id === recordId);

  if (slip) {
    const employee = snapshot.employees.find((e) => e.id === slip.employeeId);
    return (
      <div
        className="drawer-content payroll-drawer"
        data-context-module="payroll"
        data-context-submodule={submoduleId}
        data-context-record-type="payroll_payslip"
        data-context-record-id={slip.id}
        data-context-label={`Lohnabrechnung ${slip.employeeName}`}
        data-context-skill="product_engineering/business-basic-module-development"
      >
        <h3 style={{ margin: "0 0 8px" }}>Lohnabrechnung {slip.employeeName}</h3>
        <dl className="drawer-facts" style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: 4, fontSize: 13 }}>
          <dt>Status</dt><dd>{slip.status}</dd>
          <dt>Periode</dt><dd>{slip.startDate} – {slip.endDate}</dd>
          <dt>Brutto</dt><dd>{slip.grossPay.toFixed(2)} {slip.currency}</dd>
          <dt>Abzüge</dt><dd>{slip.totalDeduction.toFixed(2)} {slip.currency}</dd>
          <dt>Netto</dt><dd>{slip.netPay.toFixed(2)} {slip.currency}</dd>
          {slip.journalEntryId && <><dt>Beleg</dt><dd><code>{slip.journalEntryId}</code></dd></>}
          {employee?.taxId && <><dt>Steuer-ID</dt><dd>{employee.taxId}</dd></>}
        </dl>
      </div>
    );
  }
  if (run) {
    const period = snapshot.periods.find((p) => p.id === run.periodId);
    return (
      <div
        className="drawer-content payroll-drawer"
        data-context-module="payroll"
        data-context-submodule={submoduleId}
        data-context-record-type="payroll_run"
        data-context-record-id={run.id}
        data-context-label={`Lohnlauf ${period?.startDate ?? run.periodId}`}
        data-context-skill="product_engineering/business-basic-module-development"
      >
        <h3 style={{ margin: "0 0 8px" }}>Lohnlauf {period?.startDate ?? run.periodId}</h3>
        <dl className="drawer-facts" style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: 4, fontSize: 13 }}>
          <dt>Status</dt><dd>{run.status}</dd>
          <dt>Frequenz</dt><dd>{run.frequency}</dd>
          <dt>Konto</dt><dd>{run.payableAccountId}</dd>
          {run.error && <><dt>Fehler</dt><dd>{run.error}</dd></>}
        </dl>
      </div>
    );
  }
  if (additional) return panelKv("payroll_additional", additional.id, "Zusatzposten", additional.note ?? additional.componentId, submoduleId);
  if (component) return panelKv("payroll_component", component.id, "Komponente", component.label, submoduleId);
  if (period) return panelKv("payroll_period", period.id, "Periode", `${period.startDate} – ${period.endDate}`, submoduleId);
  if (assignment) return panelKv("payroll_structure_assignment", assignment.id, "Strukturzuweisung", assignment.employeeId, submoduleId);
  return null;
}

function panelKv(recordType: string, recordId: string, title: string, label: string, submoduleId: string) {
  return (
    <div
      className="drawer-content payroll-drawer"
      data-context-module="payroll"
      data-context-submodule={submoduleId}
      data-context-record-type={recordType}
      data-context-record-id={recordId}
      data-context-label={label}
      data-context-skill="product_engineering/business-basic-module-development"
    >
      <h3 style={{ margin: "0 0 8px" }}>{title}</h3>
      <p style={{ fontSize: 13 }}>{label}</p>
    </div>
  );
}
