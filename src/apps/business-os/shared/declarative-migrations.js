export function withDeclarativeMigrationStrategies(collections, migrationStrategies = {}) {
  if (!collections || !migrationStrategies || !Object.keys(migrationStrategies).length) {
    return collections;
  }
  const next = {};
  for (const [name, definition] of Object.entries(collections)) {
    const strategies = executableDeclarativeMigrationStrategies(migrationStrategies[name]);
    if (!strategies) {
      next[name] = definition;
    } else if (definition?.schema) {
      next[name] = { ...definition, migrationStrategies: strategies };
    } else {
      next[name] = { schema: definition, migrationStrategies: strategies };
    }
  }
  return next;
}

export function executableDeclarativeMigrationStrategies(strategies) {
  if (!strategies || typeof strategies !== 'object' || Array.isArray(strategies)) return null;
  const executable = {};
  for (const [version, spec] of Object.entries(strategies)) {
    validateDeclarativeMigrationSpec(spec);
    executable[version] = typeof spec === 'function'
      ? spec
      : (oldDoc) => applyDeclarativeMigration(oldDoc, spec);
  }
  return Object.keys(executable).length ? executable : null;
}

export function validateDeclarativeMigrationSpec(spec = {}) {
  if (typeof spec === 'function') return;
  const operations = Array.isArray(spec) ? spec : (spec.operations || []);
  if (!Array.isArray(operations)) {
    throw new Error('declarative migration spec must contain an operations array');
  }
  for (const operation of operations) {
    if (!operation || typeof operation !== 'object') {
      throw new Error('declarative migration operation must be an object');
    }
    if (operation.op === 'set_from_first_truthy') {
      if (!operation.field || !Array.isArray(operation.paths)) {
        throw new Error('set_from_first_truthy migration needs field and paths');
      }
      assertSafePathSegments(operation.field, 'set_from_first_truthy field');
      continue;
    }
    if (operation.op === 'set_boolean') {
      if (!operation.field) throw new Error('set_boolean migration needs field');
      assertSafePathSegments(operation.field, 'set_boolean field');
      continue;
    }
    throw new Error(`unsupported declarative migration operation ${operation.op}`);
  }
}

export function applyDeclarativeMigration(oldDoc, spec = {}) {
  validateDeclarativeMigrationSpec(spec);
  const migrated = { ...(oldDoc || {}) };
  const operations = Array.isArray(spec) ? spec : (spec.operations || []);
  if (!Array.isArray(operations)) {
    throw new Error('declarative migration spec must contain an operations array');
  }
  for (const operation of operations) {
    if (!operation || typeof operation !== 'object') {
      throw new Error('declarative migration operation must be an object');
    }
    if (operation.op === 'set_from_first_truthy') {
      migrated[operation.field] = firstTruthyPathValue(
        oldDoc,
        operation.paths,
        Object.hasOwn(operation, 'default') ? operation.default : undefined,
      );
    } else if (operation.op === 'set_boolean') {
      migrated[operation.field] = Boolean(pathValue(oldDoc, operation.path || operation.field));
    } else {
      throw new Error(`unsupported declarative migration operation ${operation.op}`);
    }
  }
  return migrated;
}

function firstTruthyPathValue(source, paths = [], fallback = undefined) {
  for (const path of paths) {
    const value = pathValue(source, path);
    if (value) return value;
  }
  return fallback;
}

function pathValue(source, path) {
  if (!path || !source || typeof source !== 'object') return undefined;
  let current = source;
  for (const segment of pathSegments(path)) {
    if (isUnsafePathSegment(segment)) return undefined;
    if (!current || typeof current !== 'object') return undefined;
    current = ownValue(current, segment);
  }
  return current;
}

function pathSegments(path) {
  return String(path || '').split('.').filter(Boolean);
}

function isUnsafePathSegment(segment) {
  return segment === '__proto__' || segment === 'prototype' || segment === 'constructor';
}

function assertSafePathSegments(path, label) {
  const parts = pathSegments(path);
  if (parts.some(isUnsafePathSegment)) {
    throw new Error(`${label} contains unsafe prototype segment`);
  }
  return parts;
}

function ownValue(object, key) {
  if (!object || typeof object !== 'object' || !Object.hasOwn(object, key)) return undefined;
  return Object.getOwnPropertyDescriptor(object, key)?.value;
}
