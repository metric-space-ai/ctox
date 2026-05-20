const baseRecordFields = {
  id: { type: 'string', maxLength: 160 },
  kind: { type: 'string' },
  created_at_ms: { type: 'number' },
  updated_at_ms: { type: 'number' },
  _deleted: { type: 'boolean' }
};

const employeeSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    name: { type: 'string' },
    email: { type: 'string' },
    role: { type: 'string' },
    weekly_target_hours: { type: 'number' },
    status: { type: 'string' }, // 'active', 'on_leave', 'sick', 'terminated'
    avatar_color: { type: 'string' }, // HSL color or class name
    internal_hourly_rate: { type: 'number' }, // internal cost rate (e.g. hourly wage)
    departments: { type: 'array', items: { type: 'string' } },
    skills: { type: 'array', items: { type: 'string' } },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'name', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const projectSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    name: { type: 'string' },
    client: { type: 'string' },
    location: { type: 'string' },
    hourly_rate: { type: 'number' }, // external billing rate
    color: { type: 'string' }, // oklch color reference for UI assignment
    status: { type: 'string' }, // 'active', 'completed', 'paused'
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'name', 'hourly_rate', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const shiftSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    employee_id: { type: 'string' },
    project_id: { type: 'string' }, // associated project / location
    title: { type: 'string' },
    start_time: { type: 'number' }, // ms timestamp
    end_time: { type: 'number' }, // ms timestamp
    location: { type: 'string' }, // legacy / fallback
    department: { type: 'string' },
    status: { type: 'string' }, // 'draft', 'published', 'confirmed', 'canceled'
    notes: { type: 'string' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'start_time', 'end_time', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const timeRecordSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    employee_id: { type: 'string' },
    shift_id: { type: 'string' }, // optional link to scheduled shift
    project_id: { type: 'string' }, // associated project
    billing_status: { type: 'string' }, // 'uninvoiced', 'invoiced', 'non_billable'
    billing_rate_applied: { type: 'number' }, // external billing rate applied for aggregation
    start_time: { type: 'number' }, // ms timestamp
    end_time: { type: 'number' }, // ms timestamp or null if active
    breaks: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          start: { type: 'number' },
          end: { type: 'number' }
        }
      }
    },
    notes: { type: 'string' },
    approval_status: { type: 'string' }, // 'pending', 'approved', 'rejected'
    approved_by: { type: 'string' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'employee_id', 'start_time', 'approval_status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const absenceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    ...baseRecordFields,
    employee_id: { type: 'string' },
    type: { type: 'string' }, // 'vacation', 'sick', 'parental', 'training', 'compensation'
    start_date: { type: 'string' }, // 'YYYY-MM-DD'
    end_date: { type: 'string' }, // 'YYYY-MM-DD'
    status: { type: 'string' }, // 'pending', 'approved', 'rejected'
    notes: { type: 'string' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'employee_id', 'type', 'start_date', 'end_date', 'status', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  planning_employees: employeeSchema,
  planning_projects: projectSchema,
  planning_shifts: shiftSchema,
  planning_time_records: timeRecordSchema,
  planning_absences: absenceSchema
};

export const migrationStrategies = {
  planning_employees: {},
  planning_projects: {},
  planning_shifts: {},
  planning_time_records: {},
  planning_absences: {}
};

