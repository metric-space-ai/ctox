import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { businessDeepLink } from "@ctox-business/ui";
import {
  buildJournalDraft,
  computePayslip,
  type PayrollAdditional,
  type PayrollAuditEntry,
  type PayrollComponent,
  type PayrollEmployee,
  type PayrollFrequency,
  type PayrollJournalDraft,
  type PayrollPayslip,
  type PayrollPayslipStatus,
  type PayrollPeriod,
  type PayrollRun,
  type PayrollRunStatus,
  type PayrollStructure,
  type PayrollStructureAssignment
} from "@ctox-business/payroll";
import { getWorkforceSnapshot } from "./workforce-runtime";

export type PayrollPostedJournal = {
  id: string;
  payslipId: string;
  draft: PayrollJournalDraft;
  postedAt: string;
  postedBy: string;
};

export type PayrollCtoxPayload = {
  module: "payroll";
  submodule: "runs";
  recordId: string;
  recordType:
    | "payroll_run"
    | "payroll_payslip"
    | "payroll_payslip_line"
    | "payroll_structure_assignment"
    | "payroll_component";
  selectedFields: Record<string, unknown>;
  allowedActions: string[];
};

export type PayrollEvent = {
  id: string;
  at: string;
  command: string;
  entityType: PayrollAuditEntry["entityType"] | "payroll_structure" | "payroll_component" | "payroll_period" | "payroll_additional";
  entityId: string;
  message: string;
};

export type PayrollSnapshot = {
  source: "file" | "seed";
  companyId: string;
  employees: PayrollEmployee[];
  components: PayrollComponent[];
  structures: PayrollStructure[];
  assignments: PayrollStructureAssignment[];
  periods: PayrollPeriod[];
  additionals: PayrollAdditional[];
  runs: PayrollRun[];
  payslips: PayrollPayslip[];
  audit: PayrollAuditEntry[];
  events: PayrollEvent[];
  postedJournals: PayrollPostedJournal[];
  ctoxPayloads: PayrollCtoxPayload[];
};

export type PayrollCommand =
  | "create_period"
  | "lock_period"
  | "create_component"
  | "update_component"
  | "delete_component"
  | "create_structure"
  | "update_structure"
  | "duplicate_structure"
  | "create_structure_assignment"
  | "update_structure_assignment"
  | "end_structure_assignment"
  | "create_additional"
  | "delete_additional"
  | "propose_additional_via_ctox"
  | "create_run"
  | "queue_run"
  | "cancel_run"
  | "recompute_run"
  | "bulk_mark_review"
  | "bulk_post_run"
  | "update_payslip_line"
  | "mark_payslip_review"
  | "mark_payslip_draft"
  | "mark_payslip_withheld"
  | "post_payslip"
  | "cancel_payslip"
  | "install_country_pack";

export type PayrollMutationRequest = {
  command: PayrollCommand;
  idempotencyKey?: string;
  actor?: string;
  payload?: Record<string, unknown>;
};

export type PayrollMutationResult = {
  ok: true;
  command: PayrollCommand;
  ctoxPayload?: PayrollCtoxPayload;
  event: PayrollEvent;
  snapshot: PayrollSnapshot;
};

const STORE_DIR = ".ctox-business";
const STORE_FILE = "payroll.json";

export async function getPayrollSnapshot(): Promise<PayrollSnapshot> {
  const seed = buildPayrollSeed();
  try {
    const stored = await readFile(storePath(), "utf8");
    const parsed = JSON.parse(stored) as Partial<PayrollSnapshot>;
    return normalizeSnapshot({ ...seed, ...parsed, source: "file" });
  } catch {
    await persistPayrollSnapshot(seed);
    return seed;
  }
}

export async function executePayrollCommand(request: PayrollMutationRequest): Promise<PayrollMutationResult> {
  const snapshot = await getPayrollSnapshot();
  const payload = request.payload ?? {};
  const actor = request.actor ?? "operator";
  const at = nowIso();
  let event: PayrollEvent;
  let ctoxPayload: PayrollCtoxPayload | undefined;

  switch (request.command) {
    case "create_period": {
      const period: PayrollPeriod = {
        id: stringValue(payload.id) ?? `period_${cryptoRandom()}`,
        companyId: snapshot.companyId,
        frequency: payrollFrequency(payload.frequency) ?? "monthly",
        startDate: requireString(payload.startDate, "startDate"),
        endDate: requireString(payload.endDate, "endDate"),
        locked: false,
        createdAt: at
      };
      assertDateOrder(period.startDate, period.endDate);
      snapshot.periods = upsert(snapshot.periods, period, (p) => p.id);
      event = makeEvent("payroll_period", period.id, request.command, `Periode angelegt: ${period.startDate} – ${period.endDate}`);
      break;
    }
    case "lock_period": {
      const id = requireString(payload.id, "id");
      const period = snapshot.periods.find((p) => p.id === id);
      if (!period) throw new Error("period_not_found");
      period.locked = true;
      event = makeEvent("payroll_period", period.id, request.command, `Periode gesperrt: ${period.startDate}`);
      break;
    }
    case "create_component":
    case "update_component": {
      const component: PayrollComponent = {
        id: stringValue(payload.id) ?? `pc_${cryptoRandom()}`,
        code: requireString(payload.code, "code"),
        label: requireString(payload.label, "label"),
        type: payload.type === "deduction" ? "deduction" : "earning",
        taxable: booleanValue(payload.taxable, true),
        dependsOnPaymentDays: booleanValue(payload.dependsOnPaymentDays, false),
        accountId: requireString(payload.accountId, "accountId"),
        formulaKind: (payload.formulaKind === "percent_of" || payload.formulaKind === "formula") ? payload.formulaKind : "fix",
        formulaAmount: numberValue(payload.formulaAmount),
        formulaBase: stringValue(payload.formulaBase),
        formulaPercent: numberValue(payload.formulaPercent),
        formulaExpression: stringValue(payload.formulaExpression),
        sequence: numberValue(payload.sequence) ?? 100,
        disabled: booleanValue(payload.disabled, false)
      };
      snapshot.components = upsert(snapshot.components, component, (c) => c.id);
      event = makeEvent("payroll_component", component.id, request.command, `Komponente gespeichert: ${component.label}`);
      break;
    }
    case "create_structure":
    case "update_structure": {
      const structure: PayrollStructure = {
        id: stringValue(payload.id) ?? `ps_${cryptoRandom()}`,
        companyId: snapshot.companyId,
        label: requireString(payload.label, "label"),
        frequency: payrollFrequency(payload.frequency) ?? "monthly",
        currency: stringValue(payload.currency) ?? "EUR",
        isActive: booleanValue(payload.isActive, true),
        modeOfPayment: payload.modeOfPayment === "cash" || payload.modeOfPayment === "manual" ? payload.modeOfPayment : "bank",
        componentIds: stringArray(payload.componentIds)
      };
      snapshot.structures = upsert(snapshot.structures, structure, (s) => s.id);
      event = makeEvent("payroll_run", structure.id, request.command, `Lohnstruktur gespeichert: ${structure.label}`);
      break;
    }
    case "duplicate_structure": {
      const sourceId = requireString(payload.id, "id");
      const source = snapshot.structures.find((s) => s.id === sourceId);
      if (!source) throw new Error("structure_not_found");
      const duplicate: PayrollStructure = {
        ...source,
        id: stringValue(payload.newId) ?? `ps_${cryptoRandom()}`,
        label: stringValue(payload.label) ?? `${source.label} -kopie`,
        componentIds: [...source.componentIds]
      };
      snapshot.structures = upsert(snapshot.structures, duplicate, (s) => s.id);
      event = makeEvent("payroll_run", duplicate.id, request.command, `Lohnstruktur dupliziert: ${duplicate.label}`);
      break;
    }
    case "delete_component": {
      const id = requireString(payload.id, "id");
      const component = snapshot.components.find((c) => c.id === id);
      if (!component) throw new Error("component_not_found");
      const referencingActiveStructures = snapshot.structures.filter(
        (s) => s.isActive && s.componentIds.includes(id)
      );
      if (referencingActiveStructures.length > 0) {
        throw new Error("component_in_use");
      }
      snapshot.components = snapshot.components.filter((c) => c.id !== id);
      event = makeEvent("payroll_component", id, request.command, `Komponente gelöscht: ${component.label}`);
      break;
    }
    case "create_structure_assignment": {
      const assignment: PayrollStructureAssignment = {
        id: stringValue(payload.id) ?? `psa_${cryptoRandom()}`,
        employeeId: requireString(payload.employeeId, "employeeId"),
        structureId: requireString(payload.structureId, "structureId"),
        baseSalary: numberValue(payload.baseSalary) ?? 0,
        currency: stringValue(payload.currency) ?? "EUR",
        fromDate: requireString(payload.fromDate, "fromDate"),
        toDate: stringValue(payload.toDate),
        createdAt: at,
        createdBy: actor
      };
      snapshot.assignments = upsert(snapshot.assignments, assignment, (a) => a.id);
      event = makeEvent("payroll_run", assignment.id, request.command, `Strukturzuweisung: ${assignment.employeeId}`);
      break;
    }
    case "end_structure_assignment": {
      const id = requireString(payload.id, "id");
      const assignment = snapshot.assignments.find((a) => a.id === id);
      if (!assignment) throw new Error("assignment_not_found");
      assignment.toDate = stringValue(payload.toDate) ?? at.slice(0, 10);
      event = makeEvent("payroll_run", assignment.id, request.command, `Zuweisung beendet: ${assignment.employeeId}`);
      break;
    }
    case "update_structure_assignment": {
      const id = requireString(payload.id, "id");
      const assignment = snapshot.assignments.find((a) => a.id === id);
      if (!assignment) throw new Error("assignment_not_found");
      const newBase = numberValue(payload.baseSalary);
      if (newBase !== undefined) assignment.baseSalary = newBase;
      const newCurrency = stringValue(payload.currency);
      if (newCurrency) assignment.currency = newCurrency;
      const newStructureId = stringValue(payload.structureId);
      if (newStructureId) assignment.structureId = newStructureId;
      const newFromDate = stringValue(payload.fromDate);
      if (newFromDate) assignment.fromDate = newFromDate;
      event = makeEvent("payroll_run", assignment.id, request.command, `Zuweisung aktualisiert: ${assignment.employeeId}`);
      break;
    }
    case "create_additional": {
      const additional: PayrollAdditional = {
        id: stringValue(payload.id) ?? `padd_${cryptoRandom()}`,
        employeeId: requireString(payload.employeeId, "employeeId"),
        periodId: requireString(payload.periodId, "periodId"),
        componentId: requireString(payload.componentId, "componentId"),
        amount: numberValue(payload.amount) ?? 0,
        note: stringValue(payload.note)
      };
      snapshot.additionals = upsert(snapshot.additionals, additional, (a) => a.id);
      event = makeEvent("payroll_additional", additional.id, request.command, `Zusatzposten: ${additional.employeeId}`);
      break;
    }
    case "delete_additional": {
      const id = requireString(payload.id, "id");
      snapshot.additionals = snapshot.additionals.filter((a) => a.id !== id);
      event = makeEvent("payroll_additional", id, request.command, `Zusatzposten gelöscht`);
      break;
    }
    case "propose_additional_via_ctox": {
      const employeeId = requireString(payload.employeeId, "employeeId");
      const periodId = requireString(payload.periodId, "periodId");
      const componentId = requireString(payload.componentId, "componentId");
      const amount = numberValue(payload.amount) ?? 0;
      const slipId = stringValue(payload.payslipId);
      const proposalId = `prop_${cryptoRandom()}`;
      const queuePayload = {
        proposalId,
        kind: "payroll_additional",
        moduleId: "payroll",
        submoduleId: "runs",
        recordType: slipId ? "payroll_payslip" : "payroll_additional",
        recordId: slipId ?? proposalId,
        proposed: { employeeId, periodId, componentId, amount, note: stringValue(payload.note) ?? "" }
      };
      event = makeEvent("payroll_additional", proposalId, request.command, `Vorschlag an CTOX: ${employeeId} ${componentId}`);
      event.message = `${event.message} | queueProposal=${JSON.stringify(queuePayload)}`;
      break;
    }
    case "bulk_mark_review": {
      const runId = requireString(payload.id, "id");
      const targets = snapshot.payslips.filter((s) => s.runId === runId && s.status === "Draft");
      let count = 0;
      for (const slip of targets) {
        transitionSlip(snapshot, slip, "Review", actor, at);
        count += 1;
      }
      event = makeEvent("payroll_run", runId, request.command, `Bulk-Prüfung: ${count}`);
      break;
    }
    case "bulk_post_run": {
      const runId = requireString(payload.id, "id");
      const run = snapshot.runs.find((r) => r.id === runId);
      if (!run) throw new Error("run_not_found");
      const targets = snapshot.payslips.filter((s) => s.runId === runId && s.status === "Review");
      let posted = 0;
      let failed = 0;
      for (const slip of targets) {
        if (slip.netPay < 0) {
          slip.notes = (slip.notes ?? "") + " negative_net";
          pushAudit(snapshot, "payroll_payslip", slip.id, slip.status, slip.status, actor, at, "negative_net_pay_blocks_post");
          failed += 1;
          continue;
        }
        try {
          const draft = buildJournalDraft({
            payslip: slip,
            components: snapshot.components,
            payableAccountId: run.payableAccountId,
            postingDate: run.postingDate
          });
          const journal: PayrollPostedJournal = {
            id: `je_${slip.id}`,
            payslipId: slip.id,
            draft,
            postedAt: at,
            postedBy: actor
          };
          snapshot.postedJournals = upsert(snapshot.postedJournals, journal, (p) => p.id);
          transitionSlip(snapshot, slip, "Posted", actor, at);
          slip.journalEntryId = journal.id;
          slip.postedAt = at;
          slip.postedBy = actor;
          posted += 1;
        } catch (err) {
          slip.notes = (slip.notes ?? "") + ` ${err instanceof Error ? err.message : String(err)}`;
          pushAudit(snapshot, "payroll_payslip", slip.id, slip.status, slip.status, actor, at, "post_failed");
          failed += 1;
        }
      }
      event = makeEvent("payroll_run", runId, request.command, `Bulk-Buchung: ${posted} gebucht, ${failed} blockiert`);
      break;
    }
    case "install_country_pack": {
      const country = requireString(payload.country, "country");
      if (country !== "DE") throw new Error("unsupported_country_pack");
      const pack = await import("@ctox-business/payroll-de");
      const installed = pack.installIntoSnapshot(snapshot, { actor, at });
      event = makeEvent("payroll_component", "payroll-de", request.command, `Country pack DE: ${installed.componentsAdded} Komponenten, ${installed.structuresAdded} Strukturen`);
      break;
    }
    case "create_run": {
      const periodId = requireString(payload.periodId, "periodId");
      const period = snapshot.periods.find((p) => p.id === periodId);
      if (!period) throw new Error("period_not_found");
      if (period.locked) throw new Error("period_locked");
      const frequency = payrollFrequency(payload.frequency) ?? period.frequency;
      const employees = stringArray(payload.selectedEmployeeIds);
      const conflict = snapshot.runs.find((r) => r.periodId === periodId && r.frequency === frequency && r.status !== "Cancelled");
      if (conflict) throw new Error("run_already_exists_for_period");
      const run: PayrollRun = {
        id: stringValue(payload.id) ?? `prun_${cryptoRandom()}`,
        companyId: snapshot.companyId,
        periodId,
        frequency,
        status: "Draft",
        selectedEmployeeIds: employees,
        payableAccountId: stringValue(payload.payableAccountId) ?? "1755",
        postingDate: stringValue(payload.postingDate) ?? period.endDate,
        createdBy: actor,
        createdAt: at
      };
      snapshot.runs = upsert(snapshot.runs, run, (r) => r.id);
      pushAudit(snapshot, "payroll_run", run.id, "—", run.status, actor, at);
      event = makeEvent("payroll_run", run.id, request.command, `Run angelegt für ${period.startDate}`);
      break;
    }
    case "queue_run": {
      const id = requireString(payload.id, "id");
      const run = snapshot.runs.find((r) => r.id === id);
      if (!run) throw new Error("run_not_found");
      const period = snapshot.periods.find((p) => p.id === run.periodId);
      if (!period) throw new Error("period_not_found");
      if (run.status !== "Draft" && run.status !== "Failed") throw new Error("invalid_run_state");
      const fromStatus = run.status;
      run.status = "Queued";
      pushAudit(snapshot, "payroll_run", run.id, fromStatus, run.status, actor, at);
      const generated = await generateSlipsForRun(snapshot, run, period, actor, at);
      run.status = "Submitted";
      run.submittedAt = at;
      pushAudit(snapshot, "payroll_run", run.id, "Queued", run.status, actor, at, `Slips: ${generated}`);
      event = makeEvent("payroll_run", run.id, request.command, `Run abgeschickt: ${generated} Lohnzettel erzeugt`);
      break;
    }
    case "cancel_run": {
      const id = requireString(payload.id, "id");
      const run = snapshot.runs.find((r) => r.id === id);
      if (!run) throw new Error("run_not_found");
      const fromStatus = run.status;
      run.status = "Cancelled";
      pushAudit(snapshot, "payroll_run", run.id, fromStatus, run.status, actor, at);
      for (const slip of snapshot.payslips.filter((s) => s.runId === run.id && s.status !== "Posted")) {
        const sFrom = slip.status;
        slip.status = "Cancelled";
        pushAudit(snapshot, "payroll_payslip", slip.id, sFrom, slip.status, actor, at, "Run cancelled");
      }
      event = makeEvent("payroll_run", run.id, request.command, "Run abgebrochen");
      break;
    }
    case "recompute_run": {
      const id = requireString(payload.id, "id");
      const run = snapshot.runs.find((r) => r.id === id);
      if (!run) throw new Error("run_not_found");
      const period = snapshot.periods.find((p) => p.id === run.periodId);
      if (!period) throw new Error("period_not_found");
      const recomputed = await recomputeDraftSlips(snapshot, run, period);
      event = makeEvent("payroll_run", run.id, request.command, `Slips neu berechnet: ${recomputed}`);
      break;
    }
    case "update_payslip_line": {
      const slipId = requireString(payload.payslipId, "payslipId");
      const lineId = requireString(payload.lineId, "lineId");
      const amount = numberValue(payload.amount);
      if (amount === undefined) throw new Error("amount_required");
      const slip = snapshot.payslips.find((s) => s.id === slipId);
      if (!slip) throw new Error("payslip_not_found");
      if (slip.status === "Posted" || slip.status === "Cancelled") throw new Error("payslip_immutable");
      const line = slip.lines.find((l) => l.id === lineId);
      if (!line) throw new Error("line_not_found");
      line.amount = round2(amount);
      line.rate = line.amount;
      recomputeSlipTotals(slip);
      event = makeEvent("payroll_payslip", slip.id, request.command, `Position aktualisiert: ${line.componentLabel}`);
      break;
    }
    case "mark_payslip_review": {
      const slip = requireSlip(snapshot, payload);
      if (slip.status !== "Draft" && slip.status !== "Withheld") throw new Error("invalid_payslip_state");
      transitionSlip(snapshot, slip, "Review", actor, at);
      event = makeEvent("payroll_payslip", slip.id, request.command, "Zur Prüfung freigegeben");
      break;
    }
    case "mark_payslip_draft": {
      const slip = requireSlip(snapshot, payload);
      if (slip.status !== "Review" && slip.status !== "Withheld") throw new Error("invalid_payslip_state");
      transitionSlip(snapshot, slip, "Draft", actor, at, stringValue(payload.note));
      event = makeEvent("payroll_payslip", slip.id, request.command, "Zurück zu Entwurf");
      break;
    }
    case "mark_payslip_withheld": {
      const slip = requireSlip(snapshot, payload);
      if (slip.status !== "Review" && slip.status !== "Draft") throw new Error("invalid_payslip_state");
      transitionSlip(snapshot, slip, "Withheld", actor, at, stringValue(payload.note));
      event = makeEvent("payroll_payslip", slip.id, request.command, `Zurückgestellt`);
      break;
    }
    case "post_payslip": {
      const slip = requireSlip(snapshot, payload);
      if (slip.status === "Posted") {
        event = makeEvent("payroll_payslip", slip.id, request.command, "Bereits gebucht (idempotent)");
        break;
      }
      if (slip.status !== "Review") throw new Error("invalid_payslip_state");
      if (slip.netPay < 0) throw new Error("negative_net_pay_blocks_post");
      const run = snapshot.runs.find((r) => r.id === slip.runId);
      if (!run) throw new Error("run_not_found");
      const draft = buildJournalDraft({
        payslip: slip,
        components: snapshot.components,
        payableAccountId: run.payableAccountId,
        postingDate: run.postingDate
      });
      const posted: PayrollPostedJournal = {
        id: `je_${slip.id}`,
        payslipId: slip.id,
        draft,
        postedAt: at,
        postedBy: actor
      };
      snapshot.postedJournals = upsert(snapshot.postedJournals, posted, (p) => p.id);
      transitionSlip(snapshot, slip, "Posted", actor, at);
      slip.journalEntryId = posted.id;
      slip.postedAt = at;
      slip.postedBy = actor;
      event = makeEvent("payroll_payslip", slip.id, request.command, `Gebucht (${posted.id})`);
      break;
    }
    case "cancel_payslip": {
      const slip = requireSlip(snapshot, payload);
      if (slip.status === "Posted") {
        const original = snapshot.postedJournals.find((p) => p.payslipId === slip.id);
        if (original) {
          snapshot.postedJournals.push({
            id: `${original.id}_reversal`,
            payslipId: slip.id,
            draft: {
              refType: "payroll_payslip",
              refId: `${original.draft.refId}_reversal`,
              postingDate: at.slice(0, 10),
              currency: original.draft.currency,
              narration: `Storno: ${original.draft.narration}`,
              lines: original.draft.lines.map((l) => ({
                accountId: l.accountId,
                debit: l.credit,
                credit: l.debit,
                componentCode: l.componentCode,
                partyId: l.partyId,
                narration: l.narration
              }))
            },
            postedAt: at,
            postedBy: actor
          });
        }
      }
      transitionSlip(snapshot, slip, "Cancelled", actor, at, stringValue(payload.note));
      event = makeEvent("payroll_payslip", slip.id, request.command, `Storniert`);
      break;
    }
    default: {
      const _exhaustive: never = request.command;
      void _exhaustive;
      throw new Error(`unknown_payroll_command_${String(request.command)}`);
    }
  }

  ctoxPayload = buildCtoxPayload(snapshot, payload);
  if (ctoxPayload) {
    snapshot.ctoxPayloads = [ctoxPayload, ...snapshot.ctoxPayloads.filter((c) => c.recordId !== ctoxPayload!.recordId)].slice(0, 12);
  }
  snapshot.events = [event, ...snapshot.events].slice(0, 80);
  await persistPayrollSnapshot(snapshot);
  return { ok: true, command: request.command, ctoxPayload, event, snapshot };
}

export function payrollDeepLink(recordId: string, panel = "payroll-payslip") {
  return businessDeepLink({
    module: "payroll",
    submodule: "runs",
    recordId,
    panel,
    drawer: "bottom"
  });
}

export type PayrollPeriodComparisonRow = {
  employeeId: string;
  employeeName: string;
  periodId: string;
  start: string;
  end: string;
  gross: number;
  totalDeduction: number;
  net: number;
};

export type PayrollPeriodComparison = {
  employeeId: string;
  employeeName: string;
  rows: PayrollPeriodComparisonRow[];
  grossDeltas: number[];
  netDeltas: number[];
};

export async function buildPeriodComparison(employeeId: string, periodCount: number = 6): Promise<PayrollPeriodComparison> {
  const snapshot = await getPayrollSnapshot();
  const slips = snapshot.payslips
    .filter((s) => s.employeeId === employeeId && s.status === "Posted")
    .sort((a, b) => a.endDate.localeCompare(b.endDate))
    .slice(-periodCount);
  const rows: PayrollPeriodComparisonRow[] = slips.map((slip) => ({
    employeeId,
    employeeName: slip.employeeName,
    periodId: slip.periodId,
    start: slip.startDate,
    end: slip.endDate,
    gross: slip.grossPay,
    totalDeduction: slip.totalDeduction,
    net: slip.netPay
  }));
  const grossDeltas: number[] = [];
  const netDeltas: number[] = [];
  for (let i = 1; i < rows.length; i += 1) {
    grossDeltas.push(round2(rows[i].gross - rows[i - 1].gross));
    netDeltas.push(round2(rows[i].net - rows[i - 1].net));
  }
  return {
    employeeId,
    employeeName: rows[0]?.employeeName ?? employeeId,
    rows,
    grossDeltas,
    netDeltas
  };
}

export async function buildCsvExport(periodId: string): Promise<string> {
  const snapshot = await getPayrollSnapshot();
  const slips = snapshot.payslips.filter((s) => s.periodId === periodId);
  const lines = ["employee_id,employee_name,gross,deductions,net,journal_id,status"];
  for (const slip of slips) {
    const employee = snapshot.employees.find((e) => e.id === slip.employeeId);
    lines.push([
      slip.employeeId,
      JSON.stringify(employee?.displayName ?? slip.employeeName),
      slip.grossPay.toFixed(2),
      slip.totalDeduction.toFixed(2),
      slip.netPay.toFixed(2),
      slip.journalEntryId ?? "",
      slip.status
    ].join(","));
  }
  return lines.join("\n") + "\n";
}

async function generateSlipsForRun(snapshot: PayrollSnapshot, run: PayrollRun, period: PayrollPeriod, actor: string, at: string): Promise<number> {
  const ids = run.selectedEmployeeIds.length > 0 ? run.selectedEmployeeIds : assignedEmployeeIdsFor(snapshot, run, period);
  const additionals = await additionalsWithWorkforce(snapshot, period);
  let count = 0;
  for (const employeeId of ids) {
    const employee = snapshot.employees.find((e) => e.id === employeeId);
    const assignment = activeAssignmentFor(snapshot, employeeId, period);
    if (!employee || !assignment) {
      run.error = `missing_employee_or_assignment:${employeeId}`;
      run.status = "Failed";
      pushAudit(snapshot, "payroll_run", run.id, "Queued", "Failed", actor, at, run.error);
      continue;
    }
    const structure = snapshot.structures.find((s) => s.id === assignment.structureId);
    if (!structure) {
      run.error = `missing_structure:${assignment.structureId}`;
      run.status = "Failed";
      pushAudit(snapshot, "payroll_run", run.id, "Queued", "Failed", actor, at, run.error);
      continue;
    }
    const computed = computePayslip({
      run: { id: run.id, periodId: run.periodId, postingDate: run.postingDate },
      period,
      employee,
      assignment,
      structure,
      components: snapshot.components,
      additionals
    });
    snapshot.payslips = upsert(snapshot.payslips, computed, (s) => s.id);
    pushAudit(snapshot, "payroll_payslip", computed.id, "—", computed.status, actor, at);
    count += 1;
  }
  return count;
}

async function recomputeDraftSlips(snapshot: PayrollSnapshot, run: PayrollRun, period: PayrollPeriod): Promise<number> {
  const additionals = await additionalsWithWorkforce(snapshot, period);
  let count = 0;
  for (const slip of snapshot.payslips.filter((s) => s.runId === run.id && (s.status === "Draft" || s.status === "Review"))) {
    const employee = snapshot.employees.find((e) => e.id === slip.employeeId);
    const assignment = snapshot.assignments.find((a) => a.id === slip.assignmentId);
    const structure = assignment ? snapshot.structures.find((s) => s.id === assignment.structureId) : undefined;
    if (!employee || !assignment || !structure) continue;
    const computed = computePayslip({
      run: { id: run.id, periodId: run.periodId, postingDate: run.postingDate },
      period,
      employee,
      assignment,
      structure,
      components: snapshot.components,
      additionals
    });
    slip.lines = computed.lines;
    slip.grossPay = computed.grossPay;
    slip.totalDeduction = computed.totalDeduction;
    slip.netPay = computed.netPay;
    count += 1;
  }
  return count;
}

async function additionalsWithWorkforce(snapshot: PayrollSnapshot, period: PayrollPeriod): Promise<PayrollAdditional[]> {
  const workforce = await getWorkforceSnapshot().catch(() => null);
  if (!workforce) return snapshot.additionals;
  const workforceAdditionals: PayrollAdditional[] = workforce.payrollCandidates
    .filter((candidate) => candidate.status === "prepared" && candidate.periodId === period.id)
    .map((candidate) => ({
      id: `wf_${candidate.id}`,
      employeeId: candidate.employeeId,
      periodId: period.id,
      componentId: candidate.componentId,
      amount: candidate.amount,
      note: `Workforce ${candidate.assignmentId}: ${candidate.hours.toFixed(2)}h x ${candidate.hourlyRate.toFixed(2)}`
    }));
  return [...snapshot.additionals, ...workforceAdditionals];
}

function activeAssignmentFor(snapshot: PayrollSnapshot, employeeId: string, period: PayrollPeriod): PayrollStructureAssignment | undefined {
  return snapshot.assignments
    .filter((a) => a.employeeId === employeeId)
    .filter((a) => a.fromDate <= period.endDate && (!a.toDate || a.toDate >= period.startDate))
    .sort((a, b) => b.fromDate.localeCompare(a.fromDate))[0];
}

function assignedEmployeeIdsFor(snapshot: PayrollSnapshot, _run: PayrollRun, period: PayrollPeriod): string[] {
  const set = new Set<string>();
  for (const a of snapshot.assignments) {
    if (a.fromDate <= period.endDate && (!a.toDate || a.toDate >= period.startDate)) {
      set.add(a.employeeId);
    }
  }
  return [...set];
}

function transitionSlip(snapshot: PayrollSnapshot, slip: PayrollPayslip, to: PayrollPayslipStatus, actor: string, at: string, note?: string) {
  const from = slip.status;
  slip.status = to;
  pushAudit(snapshot, "payroll_payslip", slip.id, from, to, actor, at, note);
}

function pushAudit(
  snapshot: PayrollSnapshot,
  entityType: PayrollAuditEntry["entityType"],
  entityId: string,
  fromStatus: string,
  toStatus: string,
  actor: string,
  at: string,
  note?: string
) {
  snapshot.audit.push({
    id: `aud_${cryptoRandom()}`,
    entityType,
    entityId,
    fromStatus,
    toStatus,
    actor,
    at,
    note
  });
}

function recomputeSlipTotals(slip: PayrollPayslip) {
  slip.grossPay = round2(slip.lines.filter((l) => l.type === "earning").reduce((a, l) => a + l.amount, 0));
  slip.totalDeduction = round2(slip.lines.filter((l) => l.type === "deduction").reduce((a, l) => a + l.amount, 0));
  slip.netPay = round2(slip.grossPay - slip.totalDeduction);
}

function buildCtoxPayload(snapshot: PayrollSnapshot, payload: Record<string, unknown>): PayrollCtoxPayload | undefined {
  const slipId = stringValue(payload.payslipId) ?? stringValue(payload.id);
  const slip = slipId ? snapshot.payslips.find((s) => s.id === slipId) : undefined;
  if (slip) {
    return {
      module: "payroll",
      submodule: "runs",
      recordId: slip.id,
      recordType: "payroll_payslip",
      selectedFields: {
        employeeId: slip.employeeId,
        employeeName: slip.employeeName,
        period: `${slip.startDate}..${slip.endDate}`,
        gross: slip.grossPay,
        net: slip.netPay,
        status: slip.status
      },
      allowedActions: ["review", "post", "cancel", "withhold", "explain", "recompute"]
    };
  }
  const runId = stringValue(payload.id);
  const run = runId ? snapshot.runs.find((r) => r.id === runId) : undefined;
  if (run) {
    return {
      module: "payroll",
      submodule: "runs",
      recordId: run.id,
      recordType: "payroll_run",
      selectedFields: { period: run.periodId, frequency: run.frequency, status: run.status },
      allowedActions: ["queue", "cancel", "recompute", "explain"]
    };
  }
  return undefined;
}

function requireSlip(snapshot: PayrollSnapshot, payload: Record<string, unknown>): PayrollPayslip {
  const id = stringValue(payload.payslipId) ?? stringValue(payload.id);
  if (!id) throw new Error("payslip_id_required");
  const slip = snapshot.payslips.find((s) => s.id === id);
  if (!slip) throw new Error("payslip_not_found");
  return slip;
}

function makeEvent(entityType: PayrollEvent["entityType"], entityId: string, command: string, message: string): PayrollEvent {
  return {
    id: `pe_${cryptoRandom()}`,
    at: nowIso(),
    command,
    entityType,
    entityId,
    message
  };
}

function normalizeSnapshot(snapshot: PayrollSnapshot): PayrollSnapshot {
  const components = ensureWorkforcePayrollComponent(snapshot.components ?? []);
  const structures = (snapshot.structures ?? []).map((structure) => ({
    ...structure,
    componentIds: structure.componentIds.includes("pc-workforce-hours")
      ? structure.componentIds
      : [...structure.componentIds.slice(0, 1), "pc-workforce-hours", ...structure.componentIds.slice(1)]
  }));
  return {
    source: snapshot.source ?? "file",
    companyId: snapshot.companyId ?? "ctox-business",
    employees: snapshot.employees ?? [],
    components,
    structures,
    assignments: snapshot.assignments ?? [],
    periods: snapshot.periods ?? [],
    additionals: snapshot.additionals ?? [],
    runs: snapshot.runs ?? [],
    payslips: snapshot.payslips ?? [],
    audit: snapshot.audit ?? [],
    events: snapshot.events ?? [],
    postedJournals: snapshot.postedJournals ?? [],
    ctoxPayloads: snapshot.ctoxPayloads ?? []
  };
}

function ensureWorkforcePayrollComponent(components: PayrollComponent[]): PayrollComponent[] {
  if (components.some((component) => component.id === "pc-workforce-hours")) return components;
  return [
    ...components,
    {
      id: "pc-workforce-hours",
      code: "workforce_hours",
      label: "Freigegebene Workforce-Stunden",
      type: "earning",
      taxable: true,
      dependsOnPaymentDays: false,
      accountId: "6020",
      formulaKind: "fix",
      formulaAmount: 0,
      sequence: 15,
      disabled: false
    }
  ];
}

async function persistPayrollSnapshot(snapshot: PayrollSnapshot) {
  const path = storePath();
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify({ ...snapshot, source: "file" }, null, 2), "utf8");
}

function storePath() {
  return join(process.cwd(), STORE_DIR, STORE_FILE);
}

function buildPayrollSeed(): PayrollSnapshot {
  const at = nowIso();
  const periodId = "period_2026_04";
  const periods: PayrollPeriod[] = [
    {
      id: periodId,
      companyId: "ctox-business",
      frequency: "monthly",
      startDate: "2026-04-01",
      endDate: "2026-04-30",
      locked: false,
      createdAt: at
    }
  ];
  const employees: PayrollEmployee[] = [
    { id: "emp-anna", displayName: "Anna Müller", contractType: "fulltime", taxId: "DE123456789", bankAccountIban: "DE89370400440532013000" },
    { id: "emp-ben", displayName: "Ben Schreiber", contractType: "parttime", taxId: "DE987654321", bankAccountIban: "DE12500105170648489890" }
  ];
  const components: PayrollComponent[] = [
    {
      id: "pc-base",
      code: "base",
      label: "Grundgehalt",
      type: "earning",
      taxable: true,
      dependsOnPaymentDays: true,
      accountId: "6020",
      formulaKind: "fix",
      formulaAmount: 4000,
      sequence: 10,
      disabled: false
    },
    {
      id: "pc-workforce-hours",
      code: "workforce_hours",
      label: "Freigegebene Workforce-Stunden",
      type: "earning",
      taxable: true,
      dependsOnPaymentDays: false,
      accountId: "6020",
      formulaKind: "fix",
      formulaAmount: 0,
      sequence: 15,
      disabled: false
    },
    {
      id: "pc-social",
      code: "social_employee",
      label: "Sozialversicherung AN",
      type: "deduction",
      taxable: false,
      dependsOnPaymentDays: false,
      accountId: "1742",
      formulaKind: "percent_of",
      formulaBase: "base",
      formulaPercent: 20,
      sequence: 20,
      disabled: false
    },
    {
      id: "pc-tax",
      code: "tax_employee",
      label: "Lohnsteuer AN",
      type: "deduction",
      taxable: false,
      dependsOnPaymentDays: false,
      accountId: "1741",
      formulaKind: "percent_of",
      formulaBase: "base",
      formulaPercent: 18,
      sequence: 30,
      disabled: false
    }
  ];
  const structures: PayrollStructure[] = [
    {
      id: "ps-default",
      companyId: "ctox-business",
      label: "Standard Monat (Vollzeit)",
      frequency: "monthly",
      currency: "EUR",
      isActive: true,
      modeOfPayment: "bank",
      componentIds: ["pc-base", "pc-workforce-hours", "pc-social", "pc-tax"]
    }
  ];
  const assignments: PayrollStructureAssignment[] = [
    {
      id: "psa-anna",
      employeeId: "emp-anna",
      structureId: "ps-default",
      baseSalary: 4000,
      currency: "EUR",
      fromDate: "2026-01-01",
      createdAt: at,
      createdBy: "operator"
    },
    {
      id: "psa-ben",
      employeeId: "emp-ben",
      structureId: "ps-default",
      baseSalary: 2400,
      currency: "EUR",
      fromDate: "2026-01-01",
      createdAt: at,
      createdBy: "operator"
    }
  ];
  return {
    source: "seed",
    companyId: "ctox-business",
    employees,
    components,
    structures,
    assignments,
    periods,
    additionals: [],
    runs: [],
    payslips: [],
    audit: [],
    events: [],
    postedJournals: [],
    ctoxPayloads: []
  };
}

function payrollFrequency(value: unknown): PayrollFrequency | undefined {
  if (value === "monthly" || value === "bi-weekly" || value === "weekly") return value;
  return undefined;
}

function upsert<T>(list: T[], item: T, key: (i: T) => string): T[] {
  const k = key(item);
  const filtered = list.filter((existing) => key(existing) !== k);
  return [...filtered, item];
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}
function numberValue(value: unknown): number | undefined {
  if (typeof value === "number" && !Number.isNaN(value)) return value;
  if (typeof value === "string" && value !== "") {
    const n = Number(value);
    if (!Number.isNaN(n)) return n;
  }
  return undefined;
}
function booleanValue(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") return value;
  if (value === "true") return true;
  if (value === "false") return false;
  return fallback;
}
function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((v): v is string => typeof v === "string");
}
function requireString(value: unknown, label: string): string {
  const s = stringValue(value);
  if (!s) throw new Error(`${label}_required`);
  return s;
}
function assertDateOrder(start: string, end: string) {
  if (Date.parse(end) < Date.parse(start)) throw new Error("end_must_be_on_or_after_start");
}
function nowIso(): string {
  return new Date().toISOString();
}
function cryptoRandom(): string {
  return Math.random().toString(36).slice(2, 10);
}
function round2(value: number): number {
  return Math.round(value * 100) / 100;
}
