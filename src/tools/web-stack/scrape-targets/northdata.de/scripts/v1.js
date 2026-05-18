// northdata.de — prospect.v1 extractor (Phase B initial revision).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the
// CTOX web stack (`ctox web search` + `ctox web read`) to find a profile
// page, then parses the page HTML for the field set documented in
// `tools/web-stack/src/sources/EXCEL_MATRIX.md`.
//
// Drift contract: if the selectors below stop matching but a profile
// page loads successfully, this script returns an empty records array.
// `ctox scrape execute --allow-heal` then classifies the run as
// `portal_drift` and enqueues a `universal-scraping` repair task that
// will revise this very file.

"use strict";

const { execFileSync } = require("child_process");

const MAX_HITS = 3;

function readInput() {
  const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
  if (!raw) {
    return { company: "", country: "" };
  }
  try {
    return JSON.parse(raw);
  } catch (err) {
    process.stderr.write("invalid CTOX_SCRAPE_INPUT_JSON: " + err.message + "\n");
    return { company: "", country: "" };
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
  } catch (err) {
    // Stay silent on per-hit failures: `classify_outcome` in
    // src/capabilities/scrape.rs runs a substring search for "temporary",
    // "timeout", "429", … on stderr and would misclassify the whole run
    // as temporary_unreachable if one Northdata page returned 429 while
    // others succeeded. Fatal-only stderr stays in main().
    return null;
  }
}

function searchHits(company, country) {
  const args = [
    "web",
    "search",
    "--query",
    company,
    "--source",
    "northdata.de",
    "--include-sources",
  ];
  if (country) {
    args.push("--country", country);
  }
  const payload = runCtox(args);
  if (!payload || !Array.isArray(payload.results)) {
    return [];
  }
  return payload.results
    .filter((hit) => typeof hit.url === "string" && hit.url.includes("northdata.de"))
    .slice(0, MAX_HITS);
}

function readPage(url, country) {
  const args = ["web", "read", "--url", url];
  if (country) {
    args.push("--country", country);
  }
  return runCtox(args);
}

// ---------------------------------------------------------------------------
// Parsing — mirrors src/sources/northdata.rs extract_from_html. Regex is
// intentionally permissive; the unit tests on the Rust side gate the
// selector logic. JS-side drift fixes happen by revising this file.
// ---------------------------------------------------------------------------

function parseGeneralInfoItem(html, label) {
  // Northdata renders ribbon sections as
  //   <h3 class="... ribbon ... label">Adresse</h3>
  //   <div class="general-information"><ul><li><div class="content">Grenzacherstrasse 124, 4058 Basel</div></li></ul>
  const labelEscaped = label.replace(/[-/\\^$*+?.()|[\]{}]/g, "\\$&");
  const re = new RegExp(
    "<h3[^>]*ribbon[^>]*>\\s*" +
      labelEscaped +
      "\\s*<\\/h3>([\\s\\S]*?)<h3",
    "i",
  );
  const block = html.match(re);
  if (!block) return null;
  const contentRe = /class=\"[^\"]*content[^\"]*\"[^>]*>([\s\S]*?)<\//i;
  const content = block[1].match(contentRe);
  if (!content) return null;
  return content[1].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

function parseHeading(html) {
  const m = html.match(/<h1[^>]*class=\"[^\"]*qualified[^\"]*\"[^>]*>([\s\S]*?)<\/h1>/i);
  if (!m) return null;
  return m[1].replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

function parseAddressLine(line) {
  // "Grenzacherstrasse 124, 4058 Basel" → {street, plz, ort}
  const parts = line.split(",").map((s) => s.trim()).filter(Boolean);
  if (parts.length < 2) return { street: line, plz: null, ort: null };
  const street = parts[0];
  const last = parts[parts.length - 1];
  const plzMatch = last.match(/\b(\d{4,5})\b\s*(.*)/);
  if (plzMatch) {
    return { street, plz: plzMatch[1], ort: plzMatch[2].trim() || null };
  }
  return { street, plz: null, ort: last };
}

function parseBizqPersons(html) {
  // <figure class="bizq" data-data='[{...}]'> with persons.
  const figures = [
    ...html.matchAll(/<figure[^>]*class=\"[^\"]*bizq[^\"]*\"[^>]*data-data=\"([^\"]+)\"/gi),
  ];
  const out = [];
  for (const fig of figures) {
    let dataStr = fig[1]
      .replace(/&quot;/g, '"')
      .replace(/&amp;/g, "&")
      .replace(/&#39;/g, "'");
    try {
      const data = JSON.parse(dataStr);
      const items = Array.isArray(data) ? data : data.items || [];
      for (const item of items) {
        if (item && !item.old && typeof item.text === "string") {
          out.push(item.text);
        }
      }
    } catch (err) {
      // Selector drifted; let the empty-records path trigger portal_drift.
    }
  }
  return out;
}

function splitPersonClause(text) {
  // "Vorstand Anna Müller" → {position: "Vorstand", first: "Anna", last: "Müller"}
  const trimmed = text.trim();
  const m = trimmed.match(/^([A-Za-zÄÖÜäöü\-\s\.]+?)\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+)\s+([A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+(?:\s+[A-ZÄÖÜ][A-Za-zÄÖÜäöü\-]+)*)$/);
  if (!m) return null;
  return { position: m[1].trim(), first: m[2].trim(), last: m[3].trim() };
}

function extractRecords(url, html) {
  const records = [];
  const push = (field, value, confidence, note) => {
    const v = (value || "").trim();
    if (!v) return;
    records.push({
      field,
      value: v,
      confidence,
      source_url: url,
      note,
    });
  };

  const name = parseGeneralInfoItem(html, "Name");
  if (name) {
    push("firma_name", name, "high", "ribbon section: Name");
  } else {
    const h1 = parseHeading(html);
    if (h1) {
      const cleaned = h1.split(",")[0].trim();
      push("firma_name", cleaned, "medium", "h1 fallback");
    }
  }

  let addressLine = parseGeneralInfoItem(html, "Adresse");
  if (!addressLine) {
    addressLine = parseGeneralInfoItem(html, "Anschrift");
  }
  if (addressLine) {
    const parsed = parseAddressLine(addressLine);
    if (parsed.street) push("firma_anschrift", parsed.street, "high", "ribbon section: Adresse");
    if (parsed.plz) push("firma_plz", parsed.plz, "high", "ribbon section: Adresse");
    if (parsed.ort) push("firma_ort", parsed.ort, "high", "ribbon section: Adresse");
  }

  for (const clause of parseBizqPersons(html)) {
    const parsed = splitPersonClause(clause);
    if (!parsed) continue;
    push("person_position", parsed.position, "medium", "bizq figure: position");
    push("person_vorname", parsed.first, "medium", "bizq figure: first name");
    push("person_nachname", parsed.last, "medium", "bizq figure: last name");
    // First clause is enough for an aggregated record set.
    break;
  }

  return records;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

(async function main() {
  const input = readInput();
  const company = (input.company || "").trim();
  const country = (input.country || "").trim();
  if (!company) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "portal_drift",
        detail: "CTOX_SCRAPE_INPUT_JSON.company missing",
      }),
    );
    return;
  }

  const hits = searchHits(company, country);
  if (hits.length === 0) {
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "temporary_unreachable",
        detail: "ctox web search returned no northdata.de hits",
      }),
    );
    return;
  }

  const aggregated = [];
  for (const hit of hits) {
    const page = readPage(hit.url, country);
    if (!page || !page.ok) {
      continue;
    }
    const html =
      page.raw_html_excerpt || page.raw_html || page.page_text_excerpt || "";
    if (!html) continue;
    aggregated.push(...extractRecords(page.url || hit.url, html));
  }

  process.stdout.write(JSON.stringify({ records: aggregated }));
})();
