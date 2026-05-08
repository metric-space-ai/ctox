import { NextResponse } from "next/server";
import {
  buildCsvExport,
  buildPeriodComparison,
  executePayrollCommand,
  getPayrollSnapshot,
  type PayrollMutationRequest
} from "@/lib/payroll-runtime";

const COMMANDS = new Set([
  "create_period",
  "lock_period",
  "create_component",
  "update_component",
  "delete_component",
  "create_structure",
  "update_structure",
  "duplicate_structure",
  "create_structure_assignment",
  "update_structure_assignment",
  "end_structure_assignment",
  "create_additional",
  "delete_additional",
  "propose_additional_via_ctox",
  "create_run",
  "queue_run",
  "cancel_run",
  "recompute_run",
  "bulk_mark_review",
  "bulk_post_run",
  "update_payslip_line",
  "mark_payslip_review",
  "mark_payslip_draft",
  "mark_payslip_withheld",
  "post_payslip",
  "cancel_payslip",
  "install_country_pack"
]);

export async function GET(request: Request) {
  const url = new URL(request.url);
  const view = url.searchParams.get("view");
  if (view === "comparison") {
    const employeeId = url.searchParams.get("employeeId") ?? "";
    if (!employeeId) {
      return NextResponse.json({ ok: false, error: "employeeId_required" }, { status: 400 });
    }
    const periodCount = Number(url.searchParams.get("periods") ?? "6");
    const comparison = await buildPeriodComparison(employeeId, Number.isFinite(periodCount) ? periodCount : 6);
    return NextResponse.json({ ok: true, comparison });
  }
  if (view === "export") {
    const periodId = url.searchParams.get("periodId") ?? "";
    if (!periodId) {
      return NextResponse.json({ ok: false, error: "periodId_required" }, { status: 400 });
    }
    const csv = await buildCsvExport(periodId);
    return new NextResponse(csv, {
      status: 200,
      headers: {
        "content-type": "text/csv; charset=utf-8",
        "content-disposition": `attachment; filename="payroll-${periodId}.csv"`
      }
    });
  }
  return NextResponse.json({
    ok: true,
    resource: "payroll",
    snapshot: await getPayrollSnapshot()
  });
}

export async function POST(request: Request) {
  const body = (await request.json().catch(() => ({}))) as Partial<PayrollMutationRequest>;
  if (!body.command || !COMMANDS.has(body.command)) {
    return NextResponse.json({ ok: false, error: "unknown_payroll_command" }, { status: 400 });
  }

  try {
    const result = await executePayrollCommand({
      command: body.command,
      idempotencyKey: body.idempotencyKey,
      actor: body.actor,
      payload: body.payload
    });
    return NextResponse.json(result);
  } catch (error) {
    return NextResponse.json(
      {
        ok: false,
        error: error instanceof Error ? error.message : String(error)
      },
      { status: 400 }
    );
  }
}
