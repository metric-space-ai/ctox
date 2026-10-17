// Deterministic draft automation shared by outbound.draft.prepare and
// outbound.scheduler.tick. ref: store_outbound_commands.rs:6555, :6579
import { load, str, firstString, asI64 } from './outbound.js';

export const DEFAULT_SKILLBOOK = 'business-os.outbound.message_drafting.v1';

/** ref: :6555 outbound_skillbook_guidance */
export async function skillbookGuidance(store, skillbookId) {
  const skillbook = await load(store, 'outbound_skillbooks', skillbookId);
  if (!skillbook) return null;
  const mission = str(skillbook, ['mission']) ?? '';
  const rules = skillbook.non_negotiable_rules;
  const firstRule = Array.isArray(rules) && typeof rules[0] === 'string' ? rules[0] : '';
  const missionEmpty = mission.trim() === '';
  const ruleEmpty = firstRule.trim() === '';
  if (missionEmpty && ruleEmpty) return null;
  if (!missionEmpty && ruleEmpty) return mission;
  if (missionEmpty && !ruleEmpty) return firstRule;
  return `${mission} Leitplanke: ${firstRule}.`;
}

/** ref: :6579 outbound_generate_automated_draft */
export function generateAutomatedDraft(engagement, latest, request, draftKind, guidance) {
  const contactName = firstString([
    str(engagement, ['payload', 'contact_name']),
    str(request, ['contact_name']),
  ]) ?? 'Hallo';
  const companyName = firstString([
    str(engagement, ['payload', 'company_name']),
    str(request, ['company_name']),
  ]) ?? 'Ihr Unternehmen';
  const previousSubject = latest ? str(latest, ['subject']) : null;
  const isFollowup = draftKind.startsWith('followup');
  let subject;
  if (draftKind === 'initial') subject = `Austausch zu ${companyName}`;
  else if (draftKind === 'reply') subject = previousSubject != null ? `Re: ${previousSubject}` : `Re: Austausch zu ${companyName}`;
  else if (draftKind === 'scheduling') subject = previousSubject != null ? `Re: ${previousSubject}` : 'Terminvorschlag';
  else if (isFollowup) subject = previousSubject != null ? `Re: ${previousSubject}` : `Kurzer Nachtrag zu ${companyName}`;
  else subject = previousSubject ?? `Austausch zu ${companyName}`;
  const strategy = firstString([
    str(request, ['strategy_text']),
    str(request, ['payload', 'strategy_text']),
    str(engagement, ['payload', 'strategy_text']),
    guidance,
  ]) ?? 'Kontextbezogen, knapp und ohne erfundene Aussagen schreiben.';
  let bodyText;
  if (draftKind === 'initial') {
    bodyText = `${contactName},\n\nich habe mir ${companyName} im Kontext der aktuellen Outbound-Campaign angesehen und einen moeglichen Anknuepfungspunkt identifiziert.\n\n${strategy}\n\nWenn das grundsaetzlich relevant ist, schlage ich einen kurzen Austausch vor.\n\nBeste Gruesse`;
  } else if (draftKind === 'reply') {
    const replyText = firstString([
      str(request, ['reply_text']),
      str(engagement, ['payload', 'reply_text']),
      str(engagement, ['payload', 'reply_classification']),
    ]) ?? 'die Rueckmeldung wurde als relevant klassifiziert';
    bodyText = `${contactName},\n\nvielen Dank fuer die Rueckmeldung. Ich habe den Kontext so verstanden: ${replyText}.\n\nGerne konkretisiere ich den naechsten Schritt und halte es knapp: ${strategy}\n\nBeste Gruesse`;
  } else if (draftKind === 'scheduling') {
    const duration = asI64(request?.duration_minutes) ?? 30;
    const slotHint = str(request, ['slot_hint']) ?? 'zwei bis drei passende Zeitfenster';
    bodyText = `${contactName},\n\nsehr gerne. Fuer einen kurzen Austausch wuerde ich ${duration} Minuten einplanen.\n\nIch kann ${slotHint} vorschlagen; bitte geben Sie kurz Bescheid, was bei Ihnen passt.\n\nBeste Gruesse`;
  } else if (isFollowup) {
    bodyText = `${contactName},\n\nich wollte meine vorherige Nachricht kurz nachfassen, weil der Anknuepfungspunkt zu ${companyName} weiterhin relevant sein koennte.\n\n${strategy}\n\nFalls es aktuell nicht passt, reicht eine kurze Rueckmeldung.\n\nBeste Gruesse`;
  } else {
    bodyText = `${contactName},\n\nich bereite den naechsten Outbound-Schritt zu ${companyName} vor.\n\n${strategy}\n\nBeste Gruesse`;
  }
  return { subject, body_text: bodyText, draft_kind: draftKind, requires_approval: true };
}

