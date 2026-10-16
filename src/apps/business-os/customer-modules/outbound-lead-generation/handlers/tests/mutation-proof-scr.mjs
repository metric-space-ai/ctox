// Mutationsbeleg Strang OLGSCR (Scrape-Registry, Läufer, Feld-Status-Zeugen als App-Logik), OLG3 (INV-MAIL*) und OLG4 (INV-PRA*).
// Mechanik aus tests/mutation-proof.mjs (runProof): je Invariante eine Handler-Zeile in einer TEMP-KOPIE
// brechen; der INV-SCR*-Test der genannten Datei muss rot werden, die Kopie davor/danach grün sein.
// Aufruf: node tests/mutation-proof-scr.mjs   (Exit 0 nur bei vollständigem Beleg)
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runProof } from './mutation-proof.mjs';

const HANDLERS = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export const GROUPS = [
  ['tests/outbound.research_source.test.mjs', [
    { inv: 'INV-SCR1', file: '_lib/research-source-adapter.js',
      from: "const valid = facts.is_file && declaredSha256 !== '' && declaredSha256 === actualSha256 && recordCountMatches;",
      to: 'const valid = facts.is_file && recordCountMatches;' },
  ]],
  ['tests/outbound.lead.research_writeback.test.mjs', [
    { inv: 'INV-SCR2', file: '_lib/field-status-witnesses.js', from: 'if (kept || newlyIssued) witnesses.push(entry);', to: 'witnesses.push(entry);' },
    { inv: 'INV-SCR2', file: '_lib/field-status-witnesses.js', from: 'if (current == null) {\n    const error', to: 'if (false) {\n    const error' },
  ]],
  ['tests/scrape-registry.test.mjs', [
    { inv: 'INV-SCR4', file: '_lib/scrape-registry.js',
      from: "probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms > now,",
      to: "probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms >= now," },
    { inv: 'INV-SCR3', file: '_lib/scrape-runner.js',
      from: 'if (!(authorizedProbe || now >= state.next_probe_at_ms)) return { run: false, state };',
      to: 'if (false) return { run: false, state };' },
    { inv: 'INV-SCR5', file: '_lib/scrape-registry.js',
      from: 'const known = revisions.find((r) => r.script_sha256 === sha);', to: 'const known = undefined;' },
    { inv: 'INV-SCR5', file: '_lib/scrape-registry.js',
      from: '      && (await sha256Hex(body.trim())) === active.script_sha256;', to: '      ;' },
    { inv: 'INV-SCR6', file: '_lib/scrape-runner.js',
      from: "if (exit !== 0) return result('portal_drift', true,", to: "if (false) return result('portal_drift', true," },
  ]],
  ['tests/outbound.research_source.reprobe.test.mjs', [
    { inv: 'INV-SCR3', file: '_lib/scrape-runner.js',
      from: 'const backoffAfter = (failedProbes) => (failedProbes <= 1 ? FIRST_BACKOFF_MS : LATER_BACKOFF_MS);',
      to: 'const backoffAfter = (failedProbes) => FIRST_BACKOFF_MS;' },
  ]],
  // Strang OLG3: Kontakt-E-Mail-Prüfung als App-Logik (vorher Core mail.validate_contacts).
  ['tests/contact-email-validation.test.mjs', [
    { inv: 'INV-MAIL1', file: '_lib/contact-email-validation.js', from: '    if (email !== wanted) continue;\n', to: '' },
    { inv: 'INV-MAIL2', file: '_lib/contact-email-validation.js',
      from: "status: inconclusive ? 'action_required' : 'verified',", to: "status: 'verified'," },
    { inv: 'INV-MAIL3', file: '_lib/contact-email-validation.js', from: '  if (!isValid(verdict)) return;\n  const promote', to: '  const promote' },
    { inv: 'INV-MAIL4', file: '_lib/contact-email-validation.js',
      from: "if (fence?.token == null) fail('conflict', `lead ${recordId} stayed busy", to: "if (false) fail('conflict', `lead ${recordId} stayed busy" },
    { inv: 'INV-MAIL5', file: '_lib/contact-email-validation.js',
      from: "if (requested.includes('person_email_validation') && emailsNeedingValidation(lead, 1).length > 0) return false;",
      to: 'if (false) return false;' },
    { inv: 'INV-MAIL6', file: '_lib/contact-email-validation.js', from: '    if (!receiptRunIdOk(runId)) continue;\n', to: '' },
  ]],
  // Strang OLG3: Policy-Report-Buchführung des Update-Verteilers als App-Logik.
  ['tests/outbound.update_digest.send_now.test.mjs', [
    { inv: 'INV-PR1', file: '_lib/policy-report.js',
      from: "if (row.sent_at != null || row.reviewer === 'policy-escalation') {", to: 'if (false) {' },
    { inv: 'INV-PR2', file: '_lib/policy-report.js', from: '  gate(body);\n', to: '' },
  ]],
  // Strang OLG3/OLG4: Personenrecherche Phase A, App-Prüfungen vor dem Agentenauftrag.
  ['tests/person-research-capture.test.mjs', [
    { inv: 'INV-PRC1', file: '_lib/person-research-capture.js',
      from: "if (autoBrowserCapture && ownerUserId == null) fail(", to: 'if (false) fail(' },
    { inv: 'INV-PRC2', file: '_lib/person-research-capture.js',
      from: "      fail('invalid', 'invalid runtime research source identifier');", to: '      void 0;' },
  ]],
  // Strang OLG4: Personenrecherche Phase A als Agentenauftrag (vorher browser.capture an die Core-Recherchemaschine).
  ['tests/web_stack.person_research.test.mjs', [
    { inv: 'INV-PRA1', file: '_lib/person-research.js', from: 'skill: PERSON_RESEARCH_SKILL,', to: 'skill: GAP_CLOSURE_SKILL,' },
    { inv: 'INV-PRA2', file: '_lib/person-research.js', from: '      handedOver = true;\n', to: '' },
    { inv: 'INV-PRA3', file: '_lib/person-research.js',
      from: "if (payload.person_research_callback_task_id === taskId) return { recordId, skip: 'duplicate' };", to: '' },
    { inv: 'INV-PRA4', file: '_lib/person-research.js', from: ' || payload.person_research_task_id !== taskId', to: '' },
    { inv: 'INV-PRA5', file: '_lib/person-research.js',
      from: '    await projectFailure(ctx, research, reason); // on_failed: Lead failed wie der Worker-Fehlerpfad :2075\n', to: '' },
    { inv: 'INV-PRA6', file: '_lib/person-research.js', from: 'if (block == null) {', to: 'if (false) {' },
    { inv: 'INV-PRA7', file: '_lib/person-research.js', from: 'if (MODE_SKIPPED.includes(request.mode)) {', to: 'if (false) {' },
  ]],
  // Strang SKL-C: Agenten-Starts mit Paket-Skill, Template und Rückruf (agent-task-Simulation).
  ['tests/skl-research.test.mjs', [
    { inv: 'INV-SKL1', file: '_lib/lead-research.js',
      from: "'research_instructions', 'research_instructions_default', 'idempotency_key']", to: "'idempotency_key']" },
    { inv: 'INV-SKL2', file: '_lib/lead-research.js',
      from: 'if (callbackApplied(lead, writebackId)) return', to: 'if (false) return' },
    { inv: 'INV-SKL3', file: '_lib/lead-research.js',
      from: "if (!current) return { ok: true, task_id: taskId, record_id: recordId, ignored: 'superseded' };", to: '' },
    { inv: 'INV-SKL4', file: '_lib/lead-research.js',
      from: 'idempotency_key: `${CAMPAIGN_RESEARCH}:${runId}:${leadId}`,', to: '' },
    { inv: 'INV-SKL5', file: '_lib/sellify-remarks.js',
      from: '    if (!eintrag) return null;\n    personen[personId] = eintrag;', to: '    personen[personId] = eintrag;' },
    { inv: 'INV-SKL6', file: '_lib/lead-research.js',
      from: '  if (gap) next.research_phase = null;', to: '' },
    { inv: 'INV-SKL7', file: '_lib/research-source-adapter.js',
      from: "const registered = status === 'completed' && get(target, 'script_registered') === true;", to: "const registered = status === 'completed';" },
  ]],
];

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let ok = true;
  for (const [file, mutations] of GROUPS) {
    console.log(`\n### ${file}`);
    if (!runProof(HANDLERS, mutations, file)) ok = false;
  }
  process.exitCode = ok ? 0 : 1;
}
