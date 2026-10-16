// threads.notification.preferences.update — Port von update_notification_preferences (threads.rs:2441-2506).
import {
  actorId, arrayStrings, asI64, fail, firstStringField, loadRecord, payloadOf, preflight, slugPart, upsertRecord,
} from './_lib.js';

/** valid_quiet_time (threads.rs:2495): "" oder HH:MM; Rust u8::parse akzeptiert führendes '+'. */
export function validQuietTime(value) {
  if (value === '') return true;
  const i = value.indexOf(':');
  if (i < 0) return false;
  const hour = value.slice(0, i);
  const minute = value.slice(i + 1);
  // Parse gelingt nur für ASCII → Bytelänge (Rust str::len) = String-Länge.
  const u8 = (s) => (/^\+?[0-9]+$/.test(s) ? Number(s) : NaN);
  const h = u8(hour);
  const m = u8(minute);
  return hour.length === 2 && minute.length === 2 && h >= 0 && h < 24 && m >= 0 && m < 60;
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const userId = actorId(command);
  const threshold = asI64(payload.priority_threshold) ?? 20;
  if (!(threshold >= 0 && threshold <= 100)) fail('invalid', 'priority threshold must be 0..100');
  const quietStart = firstStringField(payload, ['quiet_start']) ?? '';
  const quietEnd = firstStringField(payload, ['quiet_end']) ?? '';
  if (!validQuietTime(quietStart)) fail('invalid', 'invalid quiet_start');
  if (!validQuietTime(quietEnd)) fail('invalid', 'invalid quiet_end');
  const notificationTypes = arrayStrings(payload.notification_types);
  const stateId = `thread_state_${slugPart(userId)}_preferences`;
  const existing = (await loadRecord(ctx, 'user_thread_states', stateId)) || {};
  const record = {
    id: stateId,
    thread_id: '__preferences__',
    user_id: userId,
    unread_count: 0,
    last_seen_at_ms: asI64(existing.last_seen_at_ms) ?? 0,
    pinned: false,
    snoozed_until_ms: 0,
    priority: 'normal',
    follow_up_at_ms: 0,
    attention_score: 0,
    attention_reasons: [],
    notification_preferences: {
      priority_threshold: threshold,
      quiet_start: quietStart,
      quiet_end: quietEnd,
      notification_types: notificationTypes,
    },
    created_at_ms: asI64(existing.created_at_ms) ?? now,
    updated_at_ms: now,
  };
  await upsertRecord(ctx, 'user_thread_states', stateId, now, record);
  return { ok: true, user_id: userId, state_id: stateId, projections: [{ collection: 'user_thread_states', record_id: stateId }] };
}
