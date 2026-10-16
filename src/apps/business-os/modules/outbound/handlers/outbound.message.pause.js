// ref: store_outbound_commands.rs:836 ("outbound.message.pause")
import { pauseOrCancel } from './_lib/pause-cancel.js';

export default async function handle(command, ctx) {
  return pauseOrCancel(command, ctx, 'paused');
}
