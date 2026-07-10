import {
  CTOX_COMMAND_ALLOWED_EXECUTION_TRANSITIONS,
  CTOX_COMMAND_CONTRACT_VERSION,
  CTOX_COMMAND_ERROR_CODES,
  CTOX_COMMAND_EXECUTION_MODES,
  CTOX_COMMAND_EXECUTION_PHASES,
  CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS,
  CTOX_COMMAND_REPLICATION_PHASES,
  CTOX_COMMAND_TERMINAL_STATUSES,
} from './command-lifecycle.generated.js';

const REPLICATION_PHASES = new Set(CTOX_COMMAND_REPLICATION_PHASES);
const EXECUTION_MODES = new Set(CTOX_COMMAND_EXECUTION_MODES);
const EXECUTION_PHASES = new Set(CTOX_COMMAND_EXECUTION_PHASES);
const TERMINAL_STATUSES = new Set(CTOX_COMMAND_TERMINAL_STATUSES);
const ERROR_CODES = new Set(CTOX_COMMAND_ERROR_CODES);

export function validateCommandLifecycleDocument(document) {
  requireObject(document, 'command lifecycle document');
  if (Number(document.contract_version) !== CTOX_COMMAND_CONTRACT_VERSION) {
    throw lifecycleError('invalid_transition', `contract_version must be ${CTOX_COMMAND_CONTRACT_VERSION}`);
  }
  requireKnown(REPLICATION_PHASES, document.replication_phase, 'replication_phase');
  requireKnown(EXECUTION_MODES, document.execution_mode, 'execution_mode');
  requireKnown(EXECUTION_PHASES, document.execution_phase, 'execution_phase');
  requireKnown(TERMINAL_STATUSES, document.terminal_status, 'terminal_status');
  if (document.error_code != null && document.error_code !== '') {
    requireKnown(ERROR_CODES, document.error_code, 'error_code');
  }
  requireNonNegativeInteger(document.projection_version, 'projection_version');
  requireNonNegativeInteger(document.attempt, 'attempt');
  validateTerminalPair(document.execution_phase, document.terminal_status);
  return document;
}

export function validateCommandLifecycleTransition(previous, next) {
  validateCommandLifecycleDocument(previous);
  validateCommandLifecycleDocument(next);

  for (const field of CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS) {
    if (field in previous && !deepEqual(previous[field], next[field])) {
      throw lifecycleError('idempotency_conflict', `${field} is immutable`);
    }
  }
  if (previous.execution_mode !== next.execution_mode) {
    throw lifecycleError('invalid_transition', 'execution_mode is immutable after native observation');
  }
  if (next.projection_version < previous.projection_version) {
    throw lifecycleError('invalid_transition', 'projection_version cannot decrease');
  }
  const stateChanged = previous.replication_phase !== next.replication_phase
    || previous.execution_phase !== next.execution_phase
    || previous.terminal_status !== next.terminal_status
    || previous.attempt !== next.attempt;
  if (stateChanged && next.projection_version === previous.projection_version) {
    throw lifecycleError('invalid_transition', 'state changes require a newer projection_version');
  }
  if (next.attempt < previous.attempt) {
    throw lifecycleError('invalid_transition', 'attempt cannot decrease');
  }
  const targets = CTOX_COMMAND_ALLOWED_EXECUTION_TRANSITIONS[previous.execution_phase] || [];
  if (!targets.includes(next.execution_phase)) {
    throw lifecycleError(
      'invalid_transition',
      `execution transition ${previous.execution_phase} -> ${next.execution_phase} is not allowed`,
    );
  }
  if (previous.execution_phase === 'terminal'
      && previous.terminal_status !== next.terminal_status) {
    throw lifecycleError('invalid_transition', 'terminal_status cannot change after terminalization');
  }
  return next;
}

function validateTerminalPair(executionPhase, terminalStatus) {
  if (executionPhase === 'terminal' && terminalStatus === 'none') {
    throw lifecycleError('invalid_transition', 'terminal execution requires a terminal_status');
  }
  if (executionPhase !== 'terminal' && terminalStatus !== 'none') {
    throw lifecycleError('invalid_transition', 'nonterminal execution must use terminal_status=none');
  }
}

function requireKnown(values, value, field) {
  if (!values.has(value)) {
    throw lifecycleError('invalid_transition', `${field} contains an unknown value`);
  }
}

function requireObject(value, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw lifecycleError('invalid_transition', `${label} must be an object`);
  }
}

function requireNonNegativeInteger(value, field) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw lifecycleError('invalid_transition', `${field} must be a non-negative integer`);
  }
}

function deepEqual(left, right) {
  return canonicalJson(left) === canonicalJson(right);
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function lifecycleError(code, message) {
  const error = new Error(message);
  error.code = code;
  return error;
}
