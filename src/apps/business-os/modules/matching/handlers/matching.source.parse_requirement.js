// matching.source.parse_requirement – Port von importer.rs::import_matching_requirement
// :676-845 + parse_job_html :2164 (Routing store.rs:10177 → importer.rs:26).
// Fachlogik (Felder, Abschnitte, IDs, drei Datensätze, Abbildung der Abfragemodi) hier.
// Native Teile in Kernform (Strang CAPSIG, §16.32):
//   ctx.cap.web.fetch({url})  – SSRF-geschützter GET; Ergebnis Text-Blob {ref} + title/url
//   ctx.cap.doc.html({blob_id:{ref}, selector, attribute?, limit?}) – DOM-Abfragen (scraper)
//   Inline-HTML/Fragmente legt der Handler über ctx.files.put als files-Datensatz an.
// write_import_artifact entfällt (kein FS).
import { invalid } from './_lib/errors.js';
import { firstNonempty, hexSha256, nowIso, shortHash, strPath } from './_lib/text.js';
import { parseJobHtml, queryJobDom } from './_lib/job.js';
import { putFileRef } from './_lib/files.js';
import { envelope } from './_lib/envelope.js';

export default async function handle(command, ctx) {
  const payload = command.payload || {};
  const url = firstNonempty([strPath(payload, ['input', 'url']), strPath(payload, ['source', 'url'])]);
  let html = strPath(payload, ['input', 'html']);
  if (html.trim() === '') {
    html = firstNonempty([strPath(payload, ['input', 'text']), strPath(payload, ['source', 'text'])]);
  }
  if (url.trim() === '' && html.trim() === '') {
    throw invalid('matching requirement parse requires input.url, input.html, or input.text');
  }
  let httpStatus = 0;
  let htmlRef = '';
  let htmlSha256 = '';
  let fetchedTitle = null;
  if (html.trim() === '') {
    // web.fetch (Kernform): SSRF-Schutz, Zeitlimit und Agent setzt der Kern; Ergebnis ist ein Text-Blob
    // (files-Datensatz, {ref}) plus title/url. Fehler kommen als {code} – kein HTTP-Status mehr.
    const response = await ctx.cap.web.fetch({ url });
    htmlRef = typeof response?.blob_id?.ref === 'string' ? response.blob_id.ref : '';
    if (!htmlRef) throw invalid(`failed to read response body from ${url}`);
    httpStatus = 200;
    htmlSha256 = typeof response.sha256 === 'string' && response.sha256 ? response.sha256 : htmlRef;
    fetchedTitle = typeof response.title === 'string' && response.title.trim() ? response.title : null;
  } else {
    // UNKLAR: Rust hasht die Bytes von `html`; strPath trimmt Payload-Werte (wie Rust str_path).
    htmlSha256 = hexSha256(html);
    htmlRef = await putFileRef(ctx, html, 'requirement.html', 'text/html');
  }
  const dom = await queryJobDom(ctx, htmlRef);
  // Abgerufene Seiten liefert der Kern als Text: fehlt <title>, gilt der vom Kern gemeldete Seitentitel.
  if (fetchedTitle !== null && (dom.title === null || dom.title === undefined)) dom.title = fetchedTitle;
  // Ohne Roh-HTML (abgerufene Seite = Text) sucht die idFS-Regel im Seitentext.
  const parsed = await parseJobHtml(url, html || (typeof dom.body === 'string' ? dom.body : ''), dom, async (fragment) => {
    const fragmentRef = await putFileRef(ctx, `<html><body>${fragment}</body></html>`, 'fragment.html', 'text/html');
    const r = await ctx.cap.doc.html({ blob_id: { ref: fragmentRef }, selector: 'body', limit: 1 });
    return Array.isArray(r?.matches) && typeof r.matches[0] === 'string' ? r.matches[0] : '';
  });
  const now = nowIso(ctx.now());
  const sourceId = `source_${shortHash(`${parsed.company_name}:${parsed.location}`)}`;
  const recordId = typeof command.record_id === 'string' ? command.record_id : '';
  const requirementId = recordId.trim() ? recordId : command.id;
  const requirementSourceId = `${requirementId}::source`;
  const locationId = `loc_${shortHash(parsed.location)}`;
  const sourceDoc = {
    id: sourceId,
    kind: 'source',
    name: parsed.company_name,
    legalName: parsed.company_name,
    logoUrl: parsed.company_logo,
    sourceUrl: url,
    active: true,
    locations: [{
      id: locationId,
      city: parsed.city.trim() === '' ? parsed.location : parsed.city,
      country: parsed.country_name,
      postalCode: parsed.zip,
      address: parsed.street,
      countryKey: parsed.country,
      postalCodeKey: parsed.zip,
    }],
    createdAt: now,
    updatedAt: now,
  };
  const requirementDoc = {
    id: requirementId,
    kind: 'requirement',
    sourceId,
    sourceName: parsed.company_name,
    title: parsed.title,
    internalReferenceId: parsed.external_ref,
    status: 'open',
    location: parsed.location,
    locationIds: [locationId],
    fachlevelClass: parsed.fachlevel_class,
    workModel: parsed.work_model,
    type: parsed.work_model ?? 'Vollzeit',
    remote: parsed.remote,
    aboutSource: parsed.about_company,
    aboutRole: parsed.about_role,
    responsibilities: parsed.responsibilities,
    objectRequirements: parsed.candidate_requirements,
    requirements: parsed.requirements,
    benefits: parsed.benefits,
    closingNotes: parsed.closing_notes,
    language: 'de',
    sourceUrl: url,
    rawText: parsed.raw_text,
    active: true,
    createdAt: now,
    updatedAt: now,
  };
  const requirementSourceDoc = {
    id: requirementSourceId,
    kind: 'requirementSource',
    requirementId,
    sourceId,
    source: parsed.source_name,
    sourceUrl: url,
    externalRef: parsed.external_ref,
    rawText: parsed.raw_text,
    rawHtmlSha256: htmlSha256,
    parsed: {
      aboutSource: parsed.about_company,
      aboutRole: parsed.about_role,
      responsibilities: parsed.responsibilities,
      objectRequirements: parsed.candidate_requirements,
      benefits: parsed.benefits,
      closingNotes: parsed.closing_notes,
      workModel: parsed.work_model,
      remote: parsed.remote,
      fachlevelClass: parsed.fachlevel_class,
    },
    parsingMeta: {
      schemaVersion: 'matching.requirement.v1',
      confidence: parsed.confidence,
      httpStatus,
      command_id: command.id,
      queue_task_id: null,
    },
    createdAt: now,
    updatedAt: now,
  };
  await ctx.transaction(async (tx) => {
    const ms = ctx.now();
    const meta = { definition_id: 'matching.requirements.v1', schema_version: 'matching.requirement.v1' };
    await tx.put('matching_requirements', envelope('sources', sourceDoc, ms, meta));
    await tx.put('matching_requirements', envelope('requirements', requirementDoc, ms, meta));
    await tx.put('matching_requirements', envelope('requirementSources', requirementSourceDoc, ms, meta));
  });
  return {
    record_ids: [sourceId, requirementId, requirementSourceId],
    records_count: 3,
    collection: 'matching_requirements',
    definition_id: 'matching.requirements.v1',
    title: parsed.title,
  };
}
