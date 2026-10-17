// Origin: CTOX
// License: Apache-2.0
// support.agent.reject_suggestion – ref: support.rs:1279-1320
import { decideSuggestion } from './_suggestion-decision.js';

export default async function handle(command, ctx) {
  return decideSuggestion(command, ctx, 'rejected');
}
