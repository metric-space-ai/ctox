import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = path.dirname(fileURLToPath(import.meta.url));
const businessOsDir = path.resolve(toolDir, '..');
const outputPath = path.join(businessOsDir, 'task_id_inventory.json');
const sourceFiles = ['store.rs', 'rxdb_peer.rs'];
const classifications = new Map(Object.entries({
  accept_rxdb_business_command_with_origin: ['compatibility_mixed', 'return explicit execution_task_id/target fields'],
  complete_business_command_from_app_validation_success: ['execution_link', 'execution_task_id'],
  delete_ctox_task: ['target_task', 'target_task_id'],
  fail_business_command_from_queue_error: ['execution_link', 'execution_task_id'],
  outbound_queue_research_scraper_generation: ['domain_queue_reference', 'target_task_id or a domain-specific queue reference'],
  persist_systematic_research_failure: ['execution_link', 'execution_task_id'],
  persist_terminal_business_chat_command_projection: ['execution_link', 'execution_task_id'],
  process_business_chat_reply: ['execution_link', 'execution_task_id'],
  process_cv_print_parse_command: ['execution_link', 'execution_task_id'],
  process_documents_report_command: ['execution_link', 'execution_task_id'],
  process_source_parse_command: ['execution_link', 'execution_task_id'],
  process_systematic_research_command: ['execution_link', 'execution_task_id'],
  push_repair_action: ['domain_queue_reference', 'target_task_id or a domain-specific repair-task reference'],
  record_command: ['execution_link', 'execution_task_id'],
  record_report: ['execution_link', 'execution_task_id'],
  record_report_command: ['execution_link', 'execution_task_id'],
  write_rxdb_control_command_outcome: ['compatibility_target', 'target_task_id/target_record_id; execution_task_id stays empty'],
  write_rxdb_control_command_state: ['compatibility_target', 'target_task_id/target_record_id; execution_task_id stays empty'],
}));

const sites = [];
const occurrences = new Map();
for (const file of sourceFiles) {
  const source = fs.readFileSync(path.join(businessOsDir, file), 'utf8');
  const production = source.split(/\n#\[cfg\(test\)\]\nmod tests \{/)[0];
  let currentFunction = '<module>';
  for (const [index, line] of production.split('\n').entries()) {
    const functionMatch = line.match(/^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)/);
    if (functionMatch) currentFunction = functionMatch[1];
    if (!/"task_id"\s*:/.test(line)) continue;
    const classification = classifications.get(currentFunction);
    if (!classification) {
      throw new Error(`unclassified task_id writer ${file}:${index + 1} in ${currentFunction}`);
    }
    const occurrenceKey = `${file}:${currentFunction}`;
    const occurrence = (occurrences.get(occurrenceKey) || 0) + 1;
    occurrences.set(occurrenceKey, occurrence);
    sites.push({
      file: `src/core/business_os/${file}`,
      function: currentFunction,
      occurrence,
      semantics: classification[0],
      target_field: classification[1],
    });
  }
}
sites.sort((left, right) => left.file.localeCompare(right.file)
  || left.function.localeCompare(right.function)
  || left.occurrence - right.occurrence);
const inventory = {
  schema: 'ctox.business_os.task_id_inventory.v1',
  compatibility_rule: 'task_id remains read-only compatibility data; new lifecycle code must use execution_task_id, target_task_id or target_record_id',
  sites,
};
const rendered = `${JSON.stringify(inventory, null, 2)}\n`;
if (process.argv.includes('--check')) {
  const existing = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, 'utf8') : '';
  if (existing !== rendered) {
    throw new Error('task_id_inventory.json drifted; regenerate with build_task_id_inventory.mjs');
  }
  console.log('Business OS task_id inventory OK', { sites: sites.length });
} else {
  fs.writeFileSync(outputPath, rendered);
  console.log(`wrote ${outputPath}`);
}
