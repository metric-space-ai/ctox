// outbound.scrape.execute — ein Lauf des aktiven Skripts eines Ziels (Strang OLGSCR).
// Ausführung über das OS-Primitiv process.run (_lib/scrape-runner.js); Laufbeleg in
// outbound_lead_generation_scrape_runs + files. Ersetzt `ctox scrape execute` für Agenten.
import { asU64, fail, has, isObject, requiredString } from './_lib/olg.js';
import { executeScrape } from './_lib/scrape-runner.js';

export const MAX_TIMEOUT_SECONDS = 900;

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const targetKey = requiredString(payload, ['target_key']);
  const timeout = has(payload, 'timeout_seconds') ? asU64(payload.timeout_seconds) : 120;
  if (timeout == null || timeout < 1 || timeout > MAX_TIMEOUT_SECONDS) fail('invalid', `timeout_seconds must be an integer between 1 and ${MAX_TIMEOUT_SECONDS}`);
  if (has(payload, 'input') && !isObject(payload.input)) fail('invalid', 'input must be an object');
  return executeScrape(ctx, {
    target_key: targetKey,
    trigger_kind: typeof payload.trigger_kind === 'string' && payload.trigger_kind !== 'account_reprobe' ? payload.trigger_kind : 'manual',
    timeout_seconds: timeout,
    input: isObject(payload.input) ? payload.input : {},
    command_id: command.id,
  });
}
