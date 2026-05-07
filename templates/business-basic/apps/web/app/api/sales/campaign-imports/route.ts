import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { NextResponse } from "next/server";

export async function GET() {
  const { loadSalesAutomationStore } = await loadSalesAutomationRuntime();
  return NextResponse.json(await loadSalesAutomationStore());
}

export async function POST(request: Request) {
  const form = await request.formData();
  const file = form.get("sourceFile");
  const sourceType = parseSourceType(form.get("sourceType"));
  const campaignName = stringValue(form.get("campaignName")) || "AI Vermittlung fuer Personalvermittler";
  const description = stringValue(form.get("description"))
    || stringValue(form.get("assignmentPrompt"))
    || stringValue(form.get("sourceHint"))
    || "Consulting campaign for staffing agencies and recruiters to build AI employee placement as a new business line.";
  const sourceText = stringValue(form.get("sourceText"));
  const { importSalesCampaignSource } = await loadSalesAutomationRuntime();
  const result = await importSalesCampaignSource({
    campaignId: stringValue(form.get("campaignId")),
    campaignName,
    description,
    sourceType,
    sourceName: file instanceof File && file.name ? file.name : stringValue(form.get("sourceUrl")) || "manual-source",
    file: file instanceof File ? file : undefined,
    sourceText
  });

  return NextResponse.json(result);
}

function parseSourceType(value: FormDataEntryValue | null) {
  return value === "URL" || value === "PDF" || value === "Text" || value === "Excel" ? value : "Excel";
}

function stringValue(value: FormDataEntryValue | null) {
  return typeof value === "string" ? value.trim() : "";
}
