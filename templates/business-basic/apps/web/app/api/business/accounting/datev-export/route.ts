import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import { prepareDatevExportForAccounting } from "@/lib/business-accounting";

export async function GET() {
  const data = await getBusinessBundle();
  const exportBatch = data.bookkeeping[0];

  if (!exportBatch) {
    return NextResponse.json({ error: "datev_export_not_found" }, { status: 404 });
  }

  const accounting = prepareDatevExportForAccounting({ data, exportBatch });
  return new Response(accounting.csv, {
    headers: {
      "content-disposition": `attachment; filename="${exportBatch.period}-datev.csv"`,
      "content-type": "text/csv; charset=utf-8"
    }
  });
}
