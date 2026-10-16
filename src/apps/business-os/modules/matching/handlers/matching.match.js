// matching.match – Port von importer.rs::compute_matching_result :1020-1127
// (Routing store.rs:10188 → importer.rs:61 handle_match_compute). Reine
// Fachlogik: Texte bilden, fünf Dimensionen bewerten, Ergebnis schreiben.
// Nicht übernommen: write_import_artifact (Datei unter runtime/, kein FS im
// Handler; Inhalt steht im Rückgabewert).
import { fail, invalid } from './_lib/errors.js';
import { firstNonempty, nowIso, strPath } from './_lib/text.js';
import { envelope, unwrap } from './_lib/envelope.js';
import { buildMatchItems, matchingObjectText, matchingRequirementText, totalMatchScore } from './_lib/score.js';

export default async function handle(command, ctx) {
  const payload = command.payload || {};
  const clientContext = command.client_context || {};
  const requirementId = firstNonempty([
    strPath(clientContext, ['requirementId']),
    strPath(payload, ['options', 'businessContext', 'requirementId']),
    strPath(payload, ['request', 'requirement_id']),
  ]);
  const objectId = firstNonempty([
    strPath(clientContext, ['objectId']),
    strPath(payload, ['options', 'businessContext', 'objectId']),
    strPath(payload, ['request', 'object_id']),
  ]);
  const sourceId = firstNonempty([
    strPath(clientContext, ['sourceId']),
    strPath(payload, ['options', 'businessContext', 'sourceId']),
  ]);
  if (!requirementId) throw invalid('matching.match requires requirementId');
  if (!objectId) throw invalid('matching.match requires objectId');

  const requirement = unwrap(await ctx.get('matching_requirements', requirementId), 'requirement');
  if (!requirement) throw fail('not_found', `requirement not found: ${requirementId}`);
  const object = unwrap(await ctx.get('matching_objects', objectId), 'object');
  if (!object) throw fail('not_found', `object not found: ${objectId}`);
  const requirementText = matchingRequirementText(requirement);
  const objectText = matchingObjectText(object);
  if (!requirementText.trim()) throw invalid('requirement text is empty');
  if (!objectText.trim()) throw invalid('object text is empty');

  const now = nowIso(ctx.now());
  const recordId = typeof command.record_id === 'string' ? command.record_id : '';
  const matchId = recordId.trim() ? recordId : `${sourceId}|${requirementId}|${objectId}`;
  const items = buildMatchItems(sourceId, requirementId, objectId, requirementText, objectText, now);
  const score = totalMatchScore(items);
  await ctx.put('matching_results', envelope('matches', {
    id: matchId,
    kind: 'match',
    sourceId,
    requirementId,
    objectId,
    requirementTitle: typeof requirement.title === 'string' ? requirement.title : '',
    objectName: typeof object.name === 'string' ? object.name : '',
    active: true,
    removed: false,
    progress: 10,
    status: 'prematch',
    score,
    scoreKey: score,
    notes: '',
    items,
    evidence: items,
    createdAt: now,
    updatedAt: now,
    command_id: command.id,
    // Rust: queue_task.message_key. v4 rechnet direkt, ohne Queue-Aufgabe.
    queue_task_id: null,
  }, ctx.now(), { definition_id: 'matching.matches.v1', schema_version: 'match.v1' }));
  ctx.log('info', 'matching.match computed', { match_id: matchId, requirement_id: requirementId, object_id: objectId, score });
  return {
    record_ids: [matchId],
    records_count: 1,
    collection: 'matching_results',
    definition_id: 'matching.matches.v1',
    score,
  };
}
