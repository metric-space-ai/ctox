// Policy-Report-Buchführung des Update-Verteilers als App-Logik (Strang OLG3).
//
// Vorher im Kern: mission/channels/outbound_review.rs:2364 record_and_send_policy_report_email
// (Aufruf outbound_update_digest.rs:1442 send_report). Beim Umstieg auf die Kernform mail.send (CAPSIG2) fiel die
// Buchführung weg; sie ist Fachverhalten der App und steht jetzt in der App-Collection
// `outbound_lead_generation_policy_report_reviews` (eine Zeile je Freigabe = communication_founder_reply_reviews
// mit reviewer 'policy-report'):
//   - Prüfung Absender/Empfänger/Betreff/Text (Fehlertexte wörtlich, :2368-2383),
//   - Textprüfung auf Header-Zeilen und interne Begriffe für Text und HTML (:632/:659, update-digest.js founderBodyTextGate),
//   - Freigabe mit exaktem Digest über {thread_key, subject, to, cc, attachments} + sha256(Text) (:1332),
//     review_summary = Richtlinien-Zusammenfassung (outbound_update_digest.rs:1460),
//   - nur eine unverbrauchte Freigabe darf senden (:1795 require_unconsumed_founder_reply_review),
//   - nach dem Versand sent_at + send_result (:1881 mark_founder_reply_review_sent).
// Versand selbst: OS-Primitiv mail.send (Kernform). Nicht übernommen (Kern-Sache): Sendesperre
// acquire_reviewed_founder_send_lock und Kern-Zustandsübergang enforce_reviewed_founder_send_core_transition.
import { fail } from './olg.js';
import { founderBodyTextGate } from './update-digest.js';
import { sha256Hex } from './scrape-registry.js';

export const POLICY_REPORT_REVIEWS = 'outbound_lead_generation_policy_report_reviews';
// ref: outbound_update_digest.rs:1460
export const DIGEST_POLICY_SUMMARY = 'Outbound Update-Verteiler: Empfänger, Zeitplan und Absender vom Admin in der Outbound-App konfiguriert; Inhalt deterministisch aus den App-Datensätzen.';

const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());

/** ref: outbound_review.rs:1332 founder_reply_review_digest (serde_json preserve_order → Einfügereihenfolge). */
export async function founderReplyReviewDigest(action, body) {
  const actionJson = JSON.stringify({
    thread_key: action.thread_key, subject: action.subject, to: action.to, cc: action.cc, attachments: action.attachments,
  });
  const bodySha256 = await sha256Hex(String(body).trim());
  const actionDigest = await sha256Hex(`${actionJson}\u0000${bodySha256}`);
  return { action_digest: actionDigest, action_json: actionJson, body_sha256: bodySha256 };
}

function gate(text) {
  const error = founderBodyTextGate(text);
  if (error != null) fail('invalid', error);
}

/**
 * ref: outbound_review.rs:2364 record_and_send_policy_report_email.
 * report = {sender_email, to[], subject, body, body_html?, report_key, policy_summary}; returns the mail.send result.
 */
export async function recordAndSendPolicyReportEmail(ctx, report) {
  const sender = asciiLower(String(report.sender_email ?? '').trim());
  if (!sender.includes('@')) fail('invalid', 'policy report needs a sender mailbox address');
  const to = (Array.isArray(report.to) ? report.to : []).map((a) => asciiLower(String(a).trim())).filter((a) => a.includes('@'));
  if (to.length === 0) fail('invalid', 'policy report needs at least one recipient');
  if (String(report.subject ?? '').trim() === '' || String(report.body ?? '').trim() === '') fail('invalid', 'policy report needs subject and body');
  const body = report.body.trim();
  const subject = report.subject.trim();
  gate(body);
  const anchorKey = `policy-report:${report.report_key}`;
  const action = { thread_key: anchorKey, subject, to, cc: [], attachments: [] };
  const digest = await founderReplyReviewDigest(action, body);
  const approvalKey = `policy-report:${report.report_key}:${digest.action_digest}`;
  const approvedAt = new Date(ctx.now()).toISOString();
  // INSERT … ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET review_summary, danach die unverbrauchte
  // Freigabe verlangen — in einer Transaktion.
  await ctx.transaction(async (tx) => {
    const existing = await tx.get(POLICY_REPORT_REVIEWS, approvalKey);
    const row = existing != null ? { ...existing, review_summary: report.policy_summary } : {
      id: approvalKey, approval_key: approvalKey, inbound_message_key: anchorKey, action_digest: digest.action_digest,
      action_json: digest.action_json, body_sha256: digest.body_sha256, reviewer: 'policy-report', review_summary: report.policy_summary,
      account_key: `email:${sender}`, report_key: report.report_key, approved_at: approvedAt, sent_at: null, send_result: {},
    };
    await tx.put(POLICY_REPORT_REVIEWS, row);
    if (row.sent_at != null || row.reviewer === 'policy-escalation') {
      fail('conflict', 'reviewed founder reply has no matching unconsumed review approval for the exact body, recipients, cc, subject, and attachments');
    }
  });
  if (typeof report.body_html === 'string') gate(report.body_html);
  const delivery = await ctx.cap.mail.send({
    account_id: sender, to, subject, text: body, html: report.body_html ?? undefined, idempotency_key: report.report_key,
  });
  await ctx.transaction(async (tx) => {
    const row = await tx.get(POLICY_REPORT_REVIEWS, approvalKey);
    if (row != null && row.sent_at == null) {
      await tx.put(POLICY_REPORT_REVIEWS, { ...row, sent_at: new Date(ctx.now()).toISOString(), send_result: delivery ?? {} });
    }
  });
  return delivery;
}
