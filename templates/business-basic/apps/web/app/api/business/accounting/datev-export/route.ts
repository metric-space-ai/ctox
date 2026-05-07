import { NextResponse } from "next/server";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { prepareDatevExportForAccounting } from "@/lib/business-accounting";

export async function GET(request: Request) {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const exportBatch = data.bookkeeping[0];

  if (!exportBatch) {
    return NextResponse.json({ error: "datev_export_not_found" }, { status: 404 });
  }

  const accounting = prepareDatevExportForAccounting({ data, exportBatch });
  const persisted = Boolean(process.env.DATABASE_URL);
  if (process.env.DATABASE_URL) {
    await saveAccountingWorkflowSnapshot({
      audit: accounting.audit,
      datevExport: accounting.datevExport,
      outbox: accounting.outbox,
      proposal: accounting.proposal
    });
  }
  const filename = `${exportBatch.period}-datev.csv`;
  const url = new URL(request.url);

  if (url.searchParams.get("workflow") === "json") {
    return NextResponse.json({
      csv: accounting.csv,
      filename,
      persisted,
      workflow: {
        audit: accounting.audit,
        datevExport: accounting.datevExport,
        outbox: accounting.outbox,
        proposal: accounting.proposal
      }
    });
  }

  return new Response(accounting.csv, {
    headers: {
      "content-disposition": `attachment; filename="${filename}"`,
      "content-type": "text/csv; charset=utf-8",
      "x-accounting-workflow-persisted": String(persisted),
      "x-accounting-workflow-proposal": accounting.proposal.id
    }
  });
}
