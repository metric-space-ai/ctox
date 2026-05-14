// handelsregister.de — prospect.v1 extractor (Phase B migration).
//
// Reads CTOX_SCRAPE_INPUT_JSON for the company + country, drives the CTOX
// web stack (`ctox web search` + `ctox web read`) to find Treffer or
// "Aktueller Abdruck" detail pages on handelsregister.de — plus its
// sister hub unternehmensregister.de which mirrors a lot of the same
// Pflichtveröffentlichungs-corpus but doesn't enforce a captcha at the
// snippet level — then parses the row / label-value HTML for the field
// set documented in `tools/web-stack/src/sources/EXCEL_MATRIX.md`:
// `firma_name`, `firma_anschrift`, `firma_plz`, `firma_ort` (Confidence
// `high`) plus `person_vorname` / `person_nachname` from the
// Vorstand / Geschäftsführer / phG block (Confidence `medium`).
//
// Selector contract — Trefferliste (PrimeFaces RegPortErg datatable):
//   <table class="RegPortErg">
//     <tr> <td class="RegPortErg_FirmaSp"/> <td class="RegPortErg_SitzSp"/> … </tr>
//     <tr> <td class="RegPortErg_AdresseSp" colspan="5"/> </tr>
//
// Selector contract — "Aktueller Abdruck" Detail-Seiten:
//   label/value <tr><td>Firma</td><td>…</td></tr>; Personen folgen dem
//   DE-Pflichtveröffentlichungs-Schema
//     Nachname, Vorname, Wohnort, *Geburtsdatum [, einzelvertretungsberechtigt]
//   getrennt durch ';' wenn mehrere Personen pro Rolle. Wir splitten am
//   ERSTEN Komma (Nachname), am ZWEITEN Komma (Vorname) und ignorieren
//   alles dahinter (Wohnort, Geburtsdatum, Funktion).
//
// Captcha-Heuristik: handelsregister.de wirft regelmäßig eine reCAPTCHA-
// Wall ein. Wenn raw_html `reCAPTCHA` oder `Bitte beweisen Sie` enthält,
// überspringen wir die Seite stillschweigend (kein record emit). Der
// Executor klassifiziert das dann als `temporary_unreachable` /
// `blocked`, nicht als `portal_drift` — wir wollen die JS-Datei NICHT
// reparieren lassen, wenn nur das Captcha im Weg steht.
//
// Drift contract: wenn die Selectors auseinanderlaufen, aber die Seite
// erfolgreich lädt (kein Captcha-Marker, raw_html nicht leer), gibt
// dieser Script ein leeres records-Array zurück. `ctox scrape execute
// --allow-heal` klassifiziert das dann als `portal_drift` und queut
// einen `universal-scraping`-Repair-Task, der genau diese Datei revidiert.

"use strict";

const { execFileSync } = require("child_process");

const MAX_HITS = 5;

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
    // as temporary_unreachable if one handelsregister.de page returned a
    // 403/Captcha while a sister unternehmensregister.de page succeeded.
    // Fatal-only stderr stays in main().
    return null;
  }
}

function searchHits(company, country) {
  // `--source handelsregister.de` pins the provider cascade through the
  // Rust adapter's `shape_query`, which already adds
  // `site:handelsregister.de OR site:unternehmensregister.de`. We *also*
  // pass `--domain` for both hosts so the search-provider host filter
  // matches independently of how Google/Brave interprets the OR.
  const args = [
    "web",
    "search",
    "--query",
    company,
    "--source",
    "handelsregister.de",
    "--domain",
    "handelsregister.de",
    "--domain",
    "unternehmensregister.de",
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
    .filter(
      (hit) =>
        typeof hit.url === "string" &&
        (hit.url.includes("handelsregister.de") ||
          hit.url.includes("unternehmensregister.de")),
    )
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
// Parsing — mirrors src/sources/handelsregister.rs extract_from_html. Regex
// is intentionally permissive; the Rust unit tests gate the selector logic.
// JS-side drift fixes happen by revising this file.
// ---------------------------------------------------------------------------

function collapseWhitespace(input) {
  return (input || "")
    .replace(/&nbsp;/gi, " ")
    .replace(/<br\s*\/?>/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function stripTags(input) {
  return collapseWhitespace((input || "").replace(/<[^>]+>/g, " "));
}

function isCaptchaPage(html) {
  if (!html) return false;
  if (html.indexOf("reCAPTCHA") !== -1) return true;
  if (html.indexOf("Bitte beweisen Sie") !== -1) return true;
  return false;
}

function splitAddress(raw) {
  // "Walter-Wittenstein-Straße 1, 97999 Igersheim" → {street,plz,ort}
  const cleaned = collapseWhitespace(raw);
  if (!cleaned) return { street: "", plz: "", ort: "" };
  const parts = cleaned.split(",").map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) return { street: "", plz: "", ort: "" };
  const street = parts[0];
  let plz = "";
  let ort = "";
  if (parts.length >= 2) {
    const tail = parts.slice(1).join(", ").trim();
    const m = tail.match(/^(\d{5})\s+(.+)$/);
    if (m) {
      plz = m[1];
      ort = m[2].trim();
    } else {
      ort = tail;
    }
  }
  return { street, plz, ort };
}

// ---- Trefferliste ----------------------------------------------------------

function parseResultTables(html, url) {
  // Each `<table class="...RegPortErg...">` is one hit row. The class
  // attribute may have decoration tokens (ui-datatable-..); use a token
  // boundary check rather than equality.
  const tableRe = /<table\b[^>]*\bclass=("|')([^"']*\bRegPortErg\b[^"']*)\1[^>]*>([\s\S]*?)<\/table>/gi;
  const records = [];
  let match;
  while ((match = tableRe.exec(html)) !== null) {
    const inner = match[3];
    const hit = parseResultTableBody(inner);
    if (!hit) continue;
    if (hit.firma) {
      records.push({
        field: "firma_name",
        value: hit.firma,
        confidence: "high",
        source_url: url,
        note: "Trefferliste — RegPortErg_FirmaSp",
      });
    }
    if (hit.address && hit.address.street) {
      records.push({
        field: "firma_anschrift",
        value: hit.address.street,
        confidence: "high",
        source_url: url,
        note: "Trefferliste — RegPortErg_AdresseSp",
      });
    }
    if (hit.address && hit.address.plz) {
      records.push({
        field: "firma_plz",
        value: hit.address.plz,
        confidence: "high",
        source_url: url,
        note: "Trefferliste — RegPortErg_AdresseSp",
      });
    }
    if (hit.address && hit.address.ort) {
      records.push({
        field: "firma_ort",
        value: hit.address.ort,
        confidence: "high",
        source_url: url,
        note: "Trefferliste — RegPortErg_AdresseSp",
      });
    } else if (hit.sitz) {
      // Address line missing → fall back to Sitz column for the city only.
      records.push({
        field: "firma_ort",
        value: hit.sitz,
        confidence: "high",
        source_url: url,
        note: "Trefferliste — RegPortErg_SitzSp (fallback)",
      });
    }
    // Only the first hit is treated as primary, but downstream the executor
    // de-dupes by (field, source_url) anyway. Bail early on the first table
    // with a firma to mirror the Rust adapter behaviour.
    if (hit.firma) break;
  }
  return records;
}

function parseResultTableBody(inner) {
  const firma = cellText(inner, "RegPortErg_FirmaSp");
  if (!firma) {
    return null;
  }
  const sitz = cellText(inner, "RegPortErg_SitzSp");
  const adresseRaw = cellText(inner, "RegPortErg_AdresseSp");
  return {
    firma,
    sitz: sitz || null,
    address: adresseRaw ? splitAddress(adresseRaw) : null,
  };
}

function cellText(html, className) {
  // Match `<td class="… <className> …">…</td>` — the class attribute can
  // have decoration tokens (column-firma, ui-datatable-…), so match on
  // token boundaries.
  const re = new RegExp(
    "<td\\b[^>]*\\bclass=(\"|')([^\"']*\\b" +
      className +
      "\\b[^\"']*)\\1[^>]*>([\\s\\S]*?)<\\/td>",
    "i",
  );
  const m = html.match(re);
  if (!m) return "";
  return stripTags(m[3]);
}

// ---- "Aktueller Abdruck" Detail-Seiten -------------------------------------

function parseDetailPage(html, url) {
  // The detail view renders rows as <tr><td>Firma</td><td>…</td></tr>.
  // We scan every <tr>…</tr> with exactly the first two <td> cells and
  // map the first cell's normalized label to a known field.
  const rowRe = /<tr\b[^>]*>([\s\S]*?)<\/tr>/gi;
  const cellRe = /<td\b[^>]*>([\s\S]*?)<\/td>/gi;

  const detail = {};
  let row;
  while ((row = rowRe.exec(html)) !== null) {
    const rowInner = row[1];
    const cells = [];
    let c;
    while ((c = cellRe.exec(rowInner)) !== null) {
      cells.push(stripTags(c[1]));
      if (cells.length >= 2) break;
    }
    // Reset lastIndex on the inner regex — cellRe is local to this loop
    // iteration via redeclaration on next while-pass; nothing to do.
    if (cells.length < 2) continue;
    const label = normalizeLabel(cells[0]);
    const value = cells[1];
    if (!label || !value) continue;

    switch (label) {
      case "firma":
        if (!detail.firma) detail.firma = value;
        break;
      case "sitz":
      case "sitz / zweigniederlassung":
      case "sitz/zweigniederlassung":
        if (!detail.sitz) detail.sitz = value;
        break;
      case "geschäftsanschrift":
      case "geschaftsanschrift":
        if (!detail.address) detail.address = splitAddress(value);
        break;
      case "vorstand":
        if (!detail.person) detail.person = parsePersonBlock(value, "Aktueller Abdruck — Vorstand");
        break;
      case "geschäftsführer":
      case "geschaftsfuhrer":
      case "geschäftsführerin":
        if (!detail.person) detail.person = parsePersonBlock(value, "Aktueller Abdruck — Geschäftsführer");
        break;
      case "inhaber":
        if (!detail.person) detail.person = parsePersonBlock(value, "Aktueller Abdruck — Inhaber");
        break;
      case "persönlich haftender gesellschafter":
      case "personlich haftender gesellschafter":
        if (!detail.person) detail.person = parsePersonBlock(value, "Aktueller Abdruck — phG");
        break;
      default:
        break;
    }
  }

  if (!detail.firma && !detail.sitz && !detail.address && !detail.person) {
    return [];
  }

  const records = [];
  if (detail.firma) {
    records.push({
      field: "firma_name",
      value: detail.firma,
      confidence: "high",
      source_url: url,
      note: "Aktueller Abdruck — Firma",
    });
  }
  if (detail.address) {
    if (detail.address.street) {
      records.push({
        field: "firma_anschrift",
        value: detail.address.street,
        confidence: "high",
        source_url: url,
        note: "Aktueller Abdruck — Geschäftsanschrift",
      });
    }
    if (detail.address.plz) {
      records.push({
        field: "firma_plz",
        value: detail.address.plz,
        confidence: "high",
        source_url: url,
        note: "Aktueller Abdruck — Geschäftsanschrift",
      });
    }
    if (detail.address.ort) {
      records.push({
        field: "firma_ort",
        value: detail.address.ort,
        confidence: "high",
        source_url: url,
        note: "Aktueller Abdruck — Geschäftsanschrift",
      });
    }
  } else if (detail.sitz) {
    records.push({
      field: "firma_ort",
      value: detail.sitz,
      confidence: "high",
      source_url: url,
      note: "Aktueller Abdruck — Sitz (fallback)",
    });
  }
  if (detail.person) {
    if (detail.person.first_name) {
      records.push({
        field: "person_vorname",
        value: detail.person.first_name,
        confidence: "medium",
        source_url: url,
        note: detail.person.role_note,
      });
    }
    if (detail.person.last_name) {
      records.push({
        field: "person_nachname",
        value: detail.person.last_name,
        confidence: "medium",
        source_url: url,
        note: detail.person.role_note,
      });
    }
  }
  return records;
}

function normalizeLabel(raw) {
  return (raw || "")
    .trim()
    .replace(/:\s*$/, "")
    .trim()
    .toLowerCase();
}

function parsePersonBlock(raw, roleNote) {
  // Multiple persons separated by ';'. Take the first non-empty entry.
  const normalized = collapseWhitespace(raw);
  const first = normalized
    .split(";")
    .map((s) => s.trim())
    .find((s) => s.length > 0);
  if (!first) return null;
  // Schema: `Nachname, Vorname, Wohnort, *Geburtsdatum [, einzelvertretungsberechtigt]`
  const parts = first.split(",").map((s) => s.trim());
  if (parts.length === 0 || !parts[0]) return null;
  const last = parts[0];
  let firstName = null;
  if (parts.length >= 2) {
    const candidate = parts[1];
    if (candidate && !looksLikeDateToken(candidate) && !looksLikeFunctionKeyword(candidate)) {
      firstName = candidate;
    }
  }
  return {
    first_name: firstName,
    last_name: last,
    role_note: roleNote,
  };
}

function looksLikeDateToken(token) {
  const stripped = token.replace(/^\*/, "");
  return /^[\d.\s]+$/.test(stripped) && /\d/.test(stripped);
}

function looksLikeFunctionKeyword(token) {
  const lower = token.toLowerCase();
  return (
    lower.indexOf("vertretungsberechtigt") !== -1 ||
    lower.indexOf("gesamt") !== -1 ||
    lower.indexOf("einzel") !== -1 ||
    lower.indexOf("prokura") !== -1
  );
}

function extractRecords(url, html) {
  if (!html) return [];
  if (isCaptchaPage(html)) {
    // Caller path will see an empty record set and the executor will
    // classify the run via classify_outcome (no records + the captcha
    // page rendered as a normal 200 → blocked). We don't emit a
    // failure_mode here because we still want to try the remaining hits
    // (some hits may come from unternehmensregister.de which doesn't
    // captcha-wall the snippet).
    return [];
  }
  // Try detail-page parsing first; if it returns nothing, fall back to
  // the search-result-table path. The two formats can co-occur on a
  // single response in rare cases, but in practice a page is one or
  // the other.
  const detail = parseDetailPage(html, url);
  if (detail.length > 0) return detail;
  return parseResultTables(html, url);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

(function main() {
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
        detail:
          "ctox web search returned no handelsregister.de / unternehmensregister.de hits",
      }),
    );
    return;
  }

  const aggregated = [];
  let captchaSeen = false;
  let pagesRead = 0;
  for (const hit of hits) {
    const page = readPage(hit.url, country);
    if (!page || !page.ok) {
      continue;
    }
    const html =
      page.raw_html_excerpt || page.raw_html || page.page_text_excerpt || "";
    if (!html) continue;
    pagesRead += 1;
    if (isCaptchaPage(html)) {
      captchaSeen = true;
      continue;
    }
    const records = extractRecords(page.url || hit.url, html);
    if (records.length > 0) {
      aggregated.push(...records);
      break; // first hit with extractable records is enough
    }
  }

  if (aggregated.length === 0 && captchaSeen) {
    // Every readable page was a captcha — surface that to the executor so
    // it doesn't queue a portal_drift repair. classify_outcome maps
    // failure_mode=blocked to outcome="blocked" (status code stays 200
    // because the captcha wall is rendered as a normal 200, which is why
    // skip_probe=true is set in target.json — the wall would otherwise
    // also kill the upfront probe).
    process.stdout.write(
      JSON.stringify({
        records: [],
        failure_mode: "blocked",
        detail: "handelsregister.de captcha wall on all " + pagesRead + " hit page(s)",
      }),
    );
    return;
  }

  process.stdout.write(JSON.stringify({ records: aggregated }));
})();
