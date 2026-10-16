// kundenpipeline.mail.send – Port aus decision_hub.rs handle_mail_send :787.
// GEMISCHT. Fachlogik hier: Empfaenger (Absender des Vorgangs), Betreff "Re: …",
// Textwahl (payload.body > Run-Zusammenfassung bei art=ergebnis > Antwort-Vorschlag),
// Mail-Historie, Statusuebergang (ergebnis -> abgeschlossen, sonst inArbeit), Audit.
// Faehigkeit (Core): Absender-Hostname des Mailservers und der Versand ueber die
// Mailserver-Ausgangswarteschlange -> ctx.cap.mail.server.config.get / ctx.cap.mail.send.

import { COL_VORGAENGE, actorId, fail, loadAnyRecord, pushAudit, senderAddressOf, upsert } from './_lib/hub.js';

const str = (v) => (typeof v === 'string' ? v : undefined);

export async function sendVorgangMail(ctx, command, payload) {
  if (typeof payload.vorgang_id !== 'string') fail('invalid', 'vorgang_id is required');
  const vorgangId = payload.vorgang_id;
  const art = str(payload.art) ?? 'bestaetigung';
  const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
  if (!vorgang) fail('not_found', `unknown Vorgang \`${vorgangId}\``);

  const to = senderAddressOf(vorgang);
  if (!to) fail('invalid', 'Vorgang has no sender address to reply to');
  const betreff = str(vorgang.quelle_json?.betreff) ?? '';
  const subject = betreff.toLowerCase().startsWith('re:') ? betreff : `Re: ${betreff}`;
  const body =
    str(payload.body) ??
    (art === 'ergebnis' ? str(vorgang.run_json?.zusammenfassung) : str(vorgang.triage_json?.antwort_vorschlag));
  if (body === undefined) fail('invalid', 'no mail body available (payload.body / triage / run summary)');

  // Rust: hostname aus den Mailserver-Laufzeiteinstellungen, leer/Fehler -> "ctox.local".
  let hostname = 'ctox.local';
  try {
    const config = await ctx.cap.mail.server.config.get();
    const h = str(config?.hostname ?? config?.runtime?.hostname);
    if (h && h.trim()) hostname = h;
  } catch {
    // wie Rust: `.ok()` – Fehler fuehrt zum Rueckfall
  }
  const from = `decisions@${hostname}`;
  // Rust baut RFC 822 selbst (text/plain, utf-8, 8bit) und reiht es in die Mailserver-Queue ein. Das ist
  // Transport -> Primitiv mail.send (Kernform caps/mail.rs SendRequest): account_id = die konfigurierte
  // Absenderadresse decisions@<hostname> (Kern prüft: persönliches Konto oder exakt konfigurierte
  // Instanzadresse); text = text/plain utf-8; der Kern versendet höchstens einmal je idempotency_key.
  const sent = await ctx.cap.mail.send({
    account_id: from,
    to: [to],
    subject,
    text: body,
    idempotency_key: `${command.id}:kundenpipeline.mail.send:${vorgangId}:${art}`,
  });
  const msgId = str(sent?.message_id) ?? '';

  const now = ctx.now();
  const actor = actorId(command, 'owner');
  if (Array.isArray(vorgang.mails_json)) {
    vorgang.mails_json.push({
      richtung: 'ausgehend',
      art,
      an: to,
      betreff: subject,
      body,
      gesendet_ms: now,
      message_id: msgId,
    });
  }
  vorgang.status = art === 'ergebnis' ? 'abgeschlossen' : 'inArbeit';
  vorgang.updated_at_ms = now;
  pushAudit(vorgang, now, `mail:${art}:gesendet`, actor, 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);
  return { ok: true, queued: true, to, subject };
}

export default async function handle(command, ctx) {
  return sendVorgangMail(ctx, command, command.payload ?? {});
}
