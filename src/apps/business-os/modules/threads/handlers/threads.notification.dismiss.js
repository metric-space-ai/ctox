// threads.notification.dismiss — Port von update_notification_status(…, "dismissed") (threads.rs:1124, 2395).
import { preflight } from './_lib.js';
import { updateNotificationStatus } from './threads.notification.mark_read.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return updateNotificationStatus(ctx, command, 'dismissed');
}
