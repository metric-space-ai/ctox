import { NextResponse } from "next/server";
import { executeWorkforceCommand, getWorkforceSnapshot, type WorkforceMutationRequest } from "@/lib/workforce-runtime";

const COMMANDS = new Set([
  "create_person",
  "update_person",
  "toggle_person_active",
  "create_shift_type",
  "rename_shift_type",
  "create_location_slot",
  "rename_location_slot",
  "create_assignment",
  "update_assignment",
  "move_assignment",
  "duplicate_assignment",
  "archive_assignment",
  "resolve_blocker",
  "create_time_entry",
  "update_time_entry",
  "approve_time_entry",
  "request_correction",
  "create_absence",
  "approve_absence",
  "cancel_absence",
  "create_recurring_shift_pattern",
  "materialize_recurring_shift_pattern",
  "prepare_payroll_candidate",
  "prepare_invoice_candidate",
  "create_invoice_draft"
]);

export async function GET() {
  return NextResponse.json({
    ok: true,
    resource: "workforce",
    snapshot: await getWorkforceSnapshot()
  });
}

export async function POST(request: Request) {
  const body = (await request.json().catch(() => ({}))) as Partial<WorkforceMutationRequest>;
  if (!body.command || !COMMANDS.has(body.command)) {
    return NextResponse.json({ ok: false, error: "unknown_workforce_command" }, { status: 400 });
  }

  try {
    const result = await executeWorkforceCommand({
      command: body.command,
      actor: body.actor,
      idempotencyKey: body.idempotencyKey,
      payload: body.payload
    });
    return NextResponse.json(result);
  } catch (error) {
    return NextResponse.json(
      { ok: false, error: error instanceof Error ? error.message : String(error) },
      { status: 400 }
    );
  }
}
