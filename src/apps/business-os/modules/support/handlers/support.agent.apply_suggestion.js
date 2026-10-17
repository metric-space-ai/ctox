// Origin: CTOX
// License: Apache-2.0
// support.agent.apply_suggestion – ref: support.rs:1279-1320
// Rust-Treue: setzt nur den Status "applied"; der Vorschlagsinhalt wird NICHT ausgeführt (§3 U9).
import { decideSuggestion } from './_suggestion-decision.js';

export default async function handle(command, ctx) {
  return decideSuggestion(command, ctx, 'applied');
}
