import { sendMatchingCommand } from './v4Runtime.js';

export function tool(definition = {}) {
  return definition;
}

export const z = createZodLikeStub();

export class CtoxQueuedCommandError extends Error {
  constructor(message, result = {}) {
    super(message);
    this.name = 'CtoxQueuedCommandError';
    this.commandQueued = true;
    this.commandId = result.command_id || result.commandId || result.id || '';
    this.status = result.status || 'queued';
    this.result = result;
  }
}

const PARSE_REQUIREMENT_PROMPT = [
  'Parse the supplied requirement source into one structured matching requirement JSON object.',
  'Return only JSON with title, source, summary, requirements[], constraints[], nice_to_have[], and raw_text.',
  'Persist the result in matching_requirements.'
].join('\n');

const PARSE_OBJECT_PROMPT = [
  'Parse the supplied object source into one structured matching object JSON object.',
  'Return only JSON with title, source, summary, attributes[], evidence[], and raw_text.',
  'Persist the result in matching_objects.'
].join('\n');

const MATCH_PROMPT = [
  'Match one structured requirement object against one structured pool object.',
  'Return only JSON with requirement_id, object_id, score, evidence[], gaps[], and decision.',
  'Persist the result in matching_results.'
].join('\n');

export async function queueRequirementParseTask({ html = '', url = '' } = {}) {
  return queueCommand({
    commandType: 'matching.source.parse_requirement',
    recordId: stableRecordId('requirement'),
    payload: {
      instruction: PARSE_REQUIREMENT_PROMPT,
      prompt_key: 'parse_requirement',
      input: {
        kind: 'html',
        url: String(url || ''),
        html: String(html || '')
      },
      output_collection: 'matching_requirements'
    },
    clientContext: {
      column: 'left',
      action: 'parse-requirement'
    }
  });
}

export async function queueObjectParseTask({ files = [], sourceLabel = '', filenames = [] } = {}) {
  const filePayloads = await Promise.all((Array.isArray(files) ? files : []).map(fileToPayload));
  return queueCommand({
    commandType: 'matching.source.parse_object',
    recordId: stableRecordId('object'),
    payload: {
      instruction: PARSE_OBJECT_PROMPT,
      prompt_key: 'parse_object',
      input: {
        kind: 'files',
        source_label: sourceLabel,
        filenames,
        files: filePayloads
      },
      output_collection: 'matching_objects'
    },
    clientContext: {
      column: 'right',
      action: 'parse-object'
    }
  });
}

export async function llmChat(request = {}, options = {}) {
  const result = await queueCommand({
    commandType: options.commandType || 'matching.match',
    recordId:
      options.recordId ||
      options.matchId ||
      options.businessContext?.matchId ||
      stableRecordId('match'),
    payload: {
      instruction: MATCH_PROMPT,
      prompt_key: 'match',
      request,
      options,
      output_collection: 'matching_results'
    },
    clientContext: {
      column: 'center',
      action: 'match',
      agent: request?.agent || options.agent || 'matcher',
      ...(options.businessContext || {})
    }
  });

  // App-API 4.0: nie auf das Ergebnis warten. Der Handler schreibt matching_results; die UI
  // übernimmt es über das Live-Abo (wie vorher der Queue-Weg).
  throw new CtoxQueuedCommandError(
    'CTOX queued matching task; result arrives in matching_results.',
    result
  );
}

// Befehl über api.command (PORTING R5): liefert sofort {command_id, status:'queued'}.
// Fehlschlag meldet v4Runtime in die Zustandszone; lokal ungültig → Ausnahme an die UI.
async function queueCommand({ commandType, recordId, payload, clientContext }) {
  const handle = sendMatchingCommand(commandType, {
    ...payload,
    record_id: recordId,
    client_context: clientContext
  });
  if (!handle) throw new Error(`CTOX command failed: ${commandType}`);
  await handle.local;
  return { ok: true, command_id: handle.id, id: handle.id, record_id: recordId, status: 'queued' };
}

function stableRecordId(prefix) {
  if (globalThis.crypto?.randomUUID) return `${prefix}_${crypto.randomUUID()}`;
  return `${prefix}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

async function fileToPayload(file) {
  const dataUrl = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.'));
    reader.onload = () => resolve(String(reader.result || ''));
    reader.readAsDataURL(file);
  });
  return {
    name: file?.name || 'document.pdf',
    type: file?.type || 'application/pdf',
    size: Number(file?.size || 0),
    data_url: dataUrl
  };
}

function createZodLikeStub() {
  const fn = () => proxy;
  const proxy = new Proxy(fn, {
    get(_target, prop) {
      if (prop === 'parse') return (value) => value;
      if (prop === 'safeParse') return (value) => ({ success: true, data: value });
      if (prop === Symbol.toStringTag) return 'CtoxRequirementMatchingZodStub';
      return proxy;
    },
    apply() {
      return proxy;
    }
  });
  return proxy;
}
