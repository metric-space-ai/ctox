// ctox.queue.retry – Port von harness_cockpit.rs:152-189 (Arm ×3, 4794af03d).
import { controlQueueTask } from './_lib/cockpit.js';

export default async function handle(command, ctx) {
  return controlQueueTask(command, ctx);
}
