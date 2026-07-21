const noteRecordSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 180 },
    title: { type: 'string' },
    content: { type: 'string' },
    folder: { type: 'string' },
    updated_at_ms: { type: 'number' },
    notebook: { type: 'string' },
    tags: { type: 'string' },
    is_favorite: { type: 'boolean' },
    is_trashed: { type: 'boolean' },
    is_locked: { type: 'boolean' },
    lock_passcode: { type: 'string' }
  },
  required: ['id', 'title', 'updated_at_ms'],
  additionalProperties: true
};

// `notes` opts into the field-merge conflict strategy (docs/ctox-rxdb.md
// §8.2): concurrent edits to different fields (title/notebook/tags vs.
// content) both survive. The wrapper is a sibling of `schema` and is
// hash-neutral (all consumers read `definition.schema || definition`).
// business_commands is shell-registered — a module schema must not redefine it
// (module.json still declares it for ACCESS; the shell owns the schema).
export const collections = {
  notes: { schema: noteRecordSchema, conflictStrategy: 'field-merge' }
};

export const migrationStrategies = {
  notes: {
    1: (oldDoc) => ({
      ...oldDoc,
      notebook: oldDoc.notebook || '',
      tags: oldDoc.tags || '',
      is_favorite: !!oldDoc.is_favorite,
      is_trashed: !!oldDoc.is_trashed,
      is_locked: !!oldDoc.is_locked,
      lock_passcode: oldDoc.lock_passcode || ''
    })
  }
};
