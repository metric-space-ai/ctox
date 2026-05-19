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
    type: 'matching.source.parse_requirement',
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
    type: 'matching.source.parse_object',
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
    type: options.commandType || 'matching.match',
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
    },
    timeoutMs: options.timeoutMs || 12000
  });

  const text =
    result.text ||
    result.content ||
    result.response ||
    result.output_text ||
    result.payload?.text ||
    result.payload?.content ||
    '';

  if (typeof text === 'string' && text.trim()) return text;

  throw new CtoxQueuedCommandError(
    'CTOX queued matching task; waiting for harness result.',
    result
  );
}

async function queueCommand({ type, recordId, payload, clientContext, timeoutMs = 12000 }) {
  const command = {
    id: recordId,
    module: 'matching',
    type,
    record_id: recordId,
    payload,
    client_context: clientContext
  };

  window.dispatchEvent(new CustomEvent('ctox-business-os:agent-command', { detail: command }));
  const response = await dispatchCtoxCommand(command, { timeoutMs });
  if (response?.ok && response.result) return response.result;
  if (response?.status === 'timeout') {
    throw new Error(`CTOX command bus timeout: ${type}`);
  }
  throw new Error(response?.error || `CTOX command failed: ${type}`);
}

function dispatchCtoxCommand(command, { timeoutMs = 12000 } = {}) {
  const requestId = `ctox_matching_${Date.now()}_${Math.random().toString(16).slice(2)}`;
  return new Promise((resolve) => {
    let done = false;
    const timer = setTimeout(() => {
      if (done) return;
      done = true;
      window.removeEventListener('message', onMessage);
      resolve({ ok: false, status: 'timeout', requestId });
    }, timeoutMs);

    function onMessage(event) {
      if (event.data?.type !== 'ctox-business-os-command-result') return;
      if (event.data.requestId !== requestId) return;
      done = true;
      clearTimeout(timer);
      window.removeEventListener('message', onMessage);
      resolve(event.data);
    }

    window.addEventListener('message', onMessage);
    parent.postMessage({
      type: 'ctox-business-os-command',
      requestId,
      surface: 'matching',
      command
    }, '*');
  });
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
