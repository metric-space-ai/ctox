// threads.notification.mark_read — Port von update_notification_status(…, "read") (threads.rs:2395-2439).
import {
  actorId, clone, fail, idFromPayloadOrRecord, isAdmin, loadRecord, preflight, projectionValues,
  refreshThreadStates, threadStateId, upsertRecord, valueString,
} from './_lib.js';

/** Gemeinsamer Kern für mark_read ("read") und dismiss ("dismissed"). */
export async function updateNotificationStatus(ctx, command, status) {
  const now = ctx.now();
  const notificationId = idFromPayloadOrRecord(command, ['notification_id', 'id'], 'notification_id is required');
  const notification = await loadRecord(ctx, 'user_notifications', notificationId);
  if (!notification) fail('not_found', `notification ${notificationId} not found`);
  const notificationUser = valueString(notification, 'user_id');
  if (!(isAdmin(command) || notificationUser === actorId(command))) {
    fail('forbidden', 'only notification owner or admin can update notification status');
  }
  const threadId = valueString(notification, 'thread_id');
  const stateId = threadStateId(notificationUser, threadId);
  const projections = [{ collection: 'user_notifications', record_id: notificationId }];
  await ctx.transaction(async (tx) => {
    const next = clone(notification);
    next.status = status;
    await upsertRecord(tx, 'user_notifications', notificationId, now, next);
    const userState = await loadRecord(tx, 'user_thread_states', stateId);
    if (userState) {
      const state = clone(userState);
      state.last_seen_at_ms = now;
      state.updated_at_ms = now;
      await upsertRecord(tx, 'user_thread_states', stateId, now, state);
      projections.push({ collection: 'user_thread_states', record_id: stateId });
    }
    // U-6: Rust schreibt vorher ohne Transaktion; fehlt der Thread, scheitert der Befehl
    // nach den Schreibvorgängen. Hier rollt die Transaktion zurück.
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, notification_id: notificationId, status, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return updateNotificationStatus(ctx, command, 'read');
}
