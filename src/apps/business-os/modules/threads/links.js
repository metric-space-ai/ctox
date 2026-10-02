const INTERNAL_ROUTE = /^#([a-z][a-z0-9-]*)(?:\?([^#]*))?$/i;

export function normalizeInternalDeepLink(value, returnThreadId = '', allowedModules = null) {
  const match = INTERNAL_ROUTE.exec(String(value || '').trim());
  if (!match || match[1] === 'threads') return '';
  if (allowedModules && !allowedModules.has(match[1])) return '';
  const params = new URLSearchParams(match[2] || '');
  if (returnThreadId) params.set('return_thread_id', returnThreadId);
  return `#${match[1]}${params.size ? `?${params.toString()}` : ''}`;
}

export function sourceDeepLinkFor(entry, returnThreadId = '') {
  if (!entry) return '';
  const explicit = INTERNAL_ROUTE.exec(String(entry.source_deep_link || '').trim());
  const module = explicit?.[1] || String(entry.source_module || entry.target_module || '').trim();
  if (!/^[a-z][a-z0-9-]*$/i.test(module) || module === 'threads') return '';
  const params = new URLSearchParams(explicit?.[2] || '');
  const type = String(entry.source_record_type || entry.target_record_type || '').trim();
  const recordId = String(entry.source_record_id || entry.target_record_id || '').trim();
  const legacyId = params.get('record') || params.get('record_id') || params.get('case_id') || '';
  const targetId = recordId || legacyId;

  if (module === 'ctox') {
    if (!params.has('task_id') && !params.has('command_id')) {
      if (entry.task_id || (type === 'task' && targetId)) params.set('task_id', entry.task_id || targetId);
      else if (entry.command_id || (type === 'command' && targetId)) params.set('command_id', entry.command_id || targetId);
    }
    if (params.has('task_id') || params.has('command_id')) {
      params.delete('record');
      params.delete('record_id');
    }
  } else if (module === 'mail') {
    if (!params.has('thread_key') && !params.has('message_id') && targetId) {
      params.set(type === 'conversation' ? 'thread_key' : 'message_id', targetId);
    }
    if (params.has('thread_key') || params.has('message_id')) {
      params.delete('record');
      params.delete('record_id');
    }
  } else if (targetId && !params.has('record')) {
    params.set('record', targetId);
    params.delete('record_id');
  }
  if (type && !params.has('record_type')) params.set('record_type', type);
  if (returnThreadId) params.set('return_thread_id', returnThreadId);
  return `#${module}${params.size ? `?${params.toString()}` : ''}`;
}

export function sourceFocusSupported(entry) {
  const declaredModule = String(entry?.source_module || entry?.target_module || '').trim();
  const linkedModule = INTERNAL_ROUTE.exec(String(entry?.source_deep_link || '').trim())?.[1] || '';
  if (declaredModule && linkedModule && declaredModule !== linkedModule) return false;
  const module = linkedModule || declaredModule;
  const type = String(entry?.source_record_type || entry?.target_record_type || '').trim();
  const id = String(entry?.source_record_id || entry?.target_record_id || '').trim();
  if (module === 'ctox') return Boolean(entry?.task_id || entry?.command_id || (id && ['task', 'command'].includes(type)));
  if (module === 'tickets') return Boolean(id && ['ticket', 'ticket_case'].includes(type));
  if (module === 'outbound') return Boolean(id && ['campaign', 'company', 'pipeline_item', 'engagement', 'outbound_engagement', 'research_run'].includes(type));
  if (module === 'mail') return Boolean(id && ['conversation', 'message'].includes(type));
  if (module === 'documents') return Boolean(id && ['document', 'file'].includes(type));
  return false;
}
