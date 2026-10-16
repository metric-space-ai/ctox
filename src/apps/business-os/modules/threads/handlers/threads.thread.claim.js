// threads.thread.claim — Port von assign_thread(…, claim=true) (threads.rs:1111, 2078).
import { preflight } from './_lib.js';
import { assignThread } from './threads.thread.assign.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return assignThread(ctx, command, true);
}
