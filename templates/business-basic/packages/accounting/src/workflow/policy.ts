import type { AccountingCommand } from "./commands";
import type { AccountingProposal } from "./proposals";

export type PostingPolicy = {
  allowAutoApplyExactBankMatches: boolean;
  requireHumanReviewForLedgerCommands: boolean;
};

export const defaultPostingPolicy: PostingPolicy = {
  allowAutoApplyExactBankMatches: false,
  requireHumanReviewForLedgerCommands: true
};

export function canAutoApplyProposal(proposal: AccountingProposal, policy: PostingPolicy = defaultPostingPolicy) {
  if (proposal.kind === "bank_match" && policy.allowAutoApplyExactBankMatches && proposal.confidence >= 0.99) return true;
  return !commandTouchesLedger(proposal.proposedCommand) || !policy.requireHumanReviewForLedgerCommands;
}

export function commandTouchesLedger(command: AccountingCommand) {
  return command.type === "AcceptBankMatch" || command.type === "PostReceipt" || command.type === "RunDunning" || command.type === "SendInvoice";
}
