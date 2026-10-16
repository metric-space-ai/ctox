// ctox.crew.learning.confirm – Port von crew_commands.rs:202-241 (Arm ×3).
import { runLearningControl } from './_lib/learning.js';

export default async function handle(command, ctx) {
  return runLearningControl(command, ctx, 'confirm');
}
