// Origin: CTOX
// License: AGPL-3.0-only
import { validateSupervisorRouteDisplayValue } from './workjet-supervisor-route-display-contract.generated.mjs?v=20261010-shell-v2-supervisor-route-display';

const ROUTE_READ = 'project.supervisor.route.read.v1';
const ROUTE_CAPABILITIES = 'project.supervisor.route.capabilities.v1';
const ROUTE_SCHEMA = 'ctox.workjet.supervisor.route-display.v1';
const CAPABILITIES_SCHEMA = 'ctox.workjet.supervisor.route-capabilities.v1';

function routeScopeText(value, label) {
  if (typeof value !== 'string' || !value || value !== value.trim()
    || value.length > 256 || /[\u0000-\u001f]/u.test(value)) {
    throw new TypeError('Invalid Supervisor route ' + label + '.');
  }
  return value;
}

/** Dedicated versioned bridge. No route/model is derived from local thread settings. */
export async function requestSupervisorRoute(dispatch, request, actor, assertCurrent) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || Object.keys(request).some(key => !['action', 'commandId', 'projectId', 'threadId'].includes(key))
    || ![ROUTE_READ, ROUTE_CAPABILITIES].includes(request.action)) {
    throw new TypeError('Invalid Supervisor route request.');
  }
  const commandId = routeScopeText(request.commandId, 'commandId');
  const projectId = routeScopeText(request.projectId, 'projectId');
  const threadId = routeScopeText(request.threadId, 'threadId');
  routeScopeText(actor?.id, 'authenticated Owner');
  const capabilities = request.action === ROUTE_CAPABILITIES;
  const commandType = capabilities
    ? 'ctox.workjet.project.supervisor.route.capabilities.v1'
    : 'ctox.workjet.project.supervisor.route.read.v1';
  const payload = { project_id: projectId, thread_id: threadId };
  assertCurrent();
  const receipt = await dispatch({
    id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
    command_type: commandType, payload,
    client_context: { source: 'workjet-project-control', actor },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: 30_000 });
  assertCurrent();
  const result = receipt?.result;
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId
    || receipt.payload?.project_id !== projectId || receipt.payload?.thread_id !== threadId
    || result?.project_id !== projectId || result?.supervisor_thread_id !== threadId) {
    throw new Error('Supervisor route returned an unmatched native receipt.');
  }
  const type = capabilities ? 'SupervisorRouteCapabilities' : 'SupervisorRouteDisplay';
  if (!validateSupervisorRouteDisplayValue(type, result).ok) {
    throw new Error('Invalid native Supervisor route DTO.');
  }
  if (result.schema !== (capabilities ? CAPABILITIES_SCHEMA : ROUTE_SCHEMA)
    || (capabilities && (result.read_schema !== ROUTE_SCHEMA
      || result.read_command !== 'ctox.workjet.project.supervisor.route.read.v1'))
    || (!capabilities && result.actual !== null)) {
    throw new Error('Supervisor route returned an unsupported or unproved producer contract.');
  }
  return {
    action: request.action, commandId, projectId, threadId, contract: result.schema,
    ...(capabilities ? { capabilities: result } : { route: result }),
  };
}
