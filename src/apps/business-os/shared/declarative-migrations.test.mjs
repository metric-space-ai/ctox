import assert from 'node:assert/strict';

import {
  applyDeclarativeMigration,
  validateDeclarativeMigrationSpec,
} from './declarative-migrations.js';

delete Object.prototype.ctoxMigrationPolluted;

assert.throws(
  () => validateDeclarativeMigrationSpec({
    operations: [
      {
        op: 'set_from_first_truthy',
        field: '__proto__.ctoxMigrationPolluted',
        paths: ['title'],
      },
    ],
  }),
  /unsafe prototype segment/,
  'declarative migrations must reject unsafe target fields',
);

assert.throws(
  () => applyDeclarativeMigration(
    { title: 'Safe' },
    {
      operations: [
        {
          op: 'set_boolean',
          field: 'constructor.ctoxMigrationPolluted',
        },
      ],
    },
  ),
  /unsafe prototype segment/,
  'applyDeclarativeMigration must reject unsafe target fields when called directly',
);

const migrated = applyDeclarativeMigration(
  { source: { title: 'Safe' }, __proto__: { ctoxMigrationPolluted: 'ignored' } },
  {
    operations: [
      {
        op: 'set_from_first_truthy',
        field: 'title',
        paths: ['__proto__.ctoxMigrationPolluted', 'source.title'],
      },
    ],
  },
);

assert.equal(migrated.title, 'Safe', 'unsafe source paths are ignored before safe fallbacks');
assert.equal(
  Object.prototype.ctoxMigrationPolluted,
  undefined,
  'declarative migrations must not pollute Object.prototype',
);

console.log('declarative migrations prototype pollution guard OK');
