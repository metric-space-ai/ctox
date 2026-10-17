// threads.ctox_approval.cancel — Port von cancel_approval (threads.rs:1762-1777).
import { actorId, approvalIdFromCommand, fail, isAdmin, loadApproval, preflight, valueString } from './_lib.js';
import { decideWithoutQueue } from './threads.ctox_approval.reject.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const approval = await loadApproval(ctx, approvalIdFromCommand(command));
  if (!(actorId(command) === valueString(approval, 'requester_user_id') || isAdmin(command))) {
    fail('forbidden', 'only requester or admin can cancel this approval request');
  }
  return decideWithoutQueue(ctx, command, 'cancelled', 'approval_cancelled');
}
