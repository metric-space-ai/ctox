const projectionSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 512 },
    updated_at_ms: { type: 'number' },
    is_deleted: { type: 'boolean' },
  },
  required: ['id', 'updated_at_ms'],
  indexes: ['updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  ctox_ticket_items: projectionSchema,
  ctox_ticket_events: projectionSchema,
  ctox_ticket_event_routing_state: projectionSchema,
  ctox_ticket_cases: projectionSchema,
  ctox_ticket_self_work_items: projectionSchema,
  ctox_ticket_self_work_notes: projectionSchema,
  ctox_ticket_label_assignments: projectionSchema,
  ctox_ticket_control_bundles: projectionSchema,
  ctox_ticket_approvals: projectionSchema,
  ctox_ticket_verifications: projectionSchema,
  ctox_ticket_writebacks: projectionSchema,
  ctox_ticket_clarification_requests: projectionSchema,
};

export const migrationStrategies = {
  ctox_ticket_items: {},
  ctox_ticket_events: {},
  ctox_ticket_event_routing_state: {},
  ctox_ticket_cases: {},
  ctox_ticket_self_work_items: {},
  ctox_ticket_self_work_notes: {},
  ctox_ticket_label_assignments: {},
  ctox_ticket_control_bundles: {},
  ctox_ticket_approvals: {},
  ctox_ticket_verifications: {},
  ctox_ticket_writebacks: {},
  ctox_ticket_clarification_requests: {},
};
