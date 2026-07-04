// firmenabc.at - prospect.v1 extractor.
//
// This initial target script delegates discovery and page extraction to the
// existing web-stack source module. The value of making it a scrape target is
// lifecycle: CTOX can register, execute, materialize, test, and later revise
// this script through universal-scraping without rebuilding Rust.

"use strict";

const { execFileSync } = require("child_process");

const SOURCE_ID = "firmenabc.at";
const MAX_HITS = 4;

function readInput() {
  const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
  if (!raw) return { company: "", country: "AT" };
  try {
    return JSON.parse(raw);
  } catch (err) {
    process.stderr.write("invalid CTOX_SCRAPE_INPUT_JSON: " + err.message + "\n");
    return { company: "", country: "AT" };
  }
}

function ctoxBin() {
  return process.env.CTOX_BIN || "ctox";
}

function runCtox(args) {
  try {
    const out = execFileSync(ctoxBin(), args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 32 * 1024 * 1024,
    });
    return JSON.parse(out);
  } catch (_err) {
    return null;
  }
}

function searchHits(company, country) {
  const args = ["web", "search", "--query", company, "--source", SOURCE_ID, "--include-sources"];
  if (country) args.push("--country", country);
  const payload = runCtox(args);
  if (!payload || !Array.isArray(payload.results)) return [];
  return payload.results
    .filter((hit) => typeof hit.url === "string" && hit.url.includes("firmenabc.at"))
    .slice(0, MAX_HITS);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) args.push("--country", country);
  return runCtox(args);
}

function recordsFromReadPage(page, fallbackUrl) {
  if (!page || !page.ok) return [];
  const fields = page.extracted_fields && Array.isArray(page.extracted_fields.fields)
    ? page.extracted_fields.fields
    : [];
  return fields
    .filter((item) => item && typeof item.field === "string" && typeof item.value === "string")
    .map((item) => ({
      field: item.field,
      value: item.value,
      confidence: item.confidence || "medium",
      source_url: item.source_url || page.url || fallbackUrl,
      note: item.note || "ctox web read extracted_fields",
    }));
}

(function main() {
  const input = readInput();
  const company = String(input.company || "").trim();
  const country = String(input.country || "AT").trim() || "AT";
  if (!company) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "portal_drift",
      detail: "CTOX_SCRAPE_INPUT_JSON.company missing",
    }));
    return;
  }

  const aggregated = [];
  for (const hit of searchHits(company, country)) {
    const records = recordsFromReadPage(readPage(hit.url, country), hit.url);
    aggregated.push(...records);
    if (aggregated.length > 0) break;
  }

  if (aggregated.length === 0) {
    process.stdout.write(JSON.stringify({
      records: [],
      failure_mode: "temporary_unreachable",
      detail: "no usable firmenabc.at profile records",
    }));
    return;
  }

  process.stdout.write(JSON.stringify({ records: aggregated }));
})();
