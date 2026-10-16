// threads.thread.unwatch — Port von update_thread_watch(…, false) (threads.rs:1106, 1890).
import { preflight } from './_lib.js';
import { updateThreadWatch } from './threads.thread.watch.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return updateThreadWatch(ctx, command, false);
}
