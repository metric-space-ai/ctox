// Source of truth for IoT business-os collection schemas consumed by
// src/core/rxdb/tools/build_business_os_schema_contract.mjs to regenerate
// src/core/business_os/business_os_schema_contract.json. House-derived fields
// (index_text, sort_key, status_key, *_at_ms) are part of each schema.

export const collections = {
  iot_agent_status: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "agent_id": {
        "type": "string"
      },
      "link_state": {
        "type": "string"
      },
      "last_event_ms": {
        "type": "number"
      },
      "error": {
        "type": "string"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "agent_id",
      "link_state",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "realm",
      "status_key",
      "sort_key",
      "updated_at_ms",
      [
        "agent_id",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_agents: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "name": {
        "type": "string"
      },
      "kind": {
        "type": "string"
      },
      "enabled": {
        "type": "boolean"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "name",
      "kind",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "realm",
      "status_key",
      "sort_key",
      "updated_at_ms",
      [
        "realm",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_alarms: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "title": {
        "type": "string"
      },
      "severity": {
        "type": "string"
      },
      "status": {
        "type": "string"
      },
      "assignee_id": {
        "type": "string"
      },
      "source": {
        "type": "string"
      },
      "created_ms": {
        "type": "number"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "title",
      "severity",
      "status",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "realm",
      "status_key",
      "severity",
      "assignee_id",
      "sort_key",
      "updated_at_ms",
      [
        "realm",
        "updated_at_ms"
      ],
      [
        "status_key",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_asset_types: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "asset_type": {
        "type": "string"
      },
      "attribute_count": {
        "type": "number"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "asset_type",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "sort_key",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  iot_assets: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "parent_id": {
        "type": "string"
      },
      "asset_type": {
        "type": "string"
      },
      "name": {
        "type": "string"
      },
      "attribute_summary": {
        "type": "object",
        "additionalProperties": true
      },
      "location": {
        "type": "object",
        "additionalProperties": true
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "asset_type",
      "name",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "realm",
      "asset_type",
      "parent_id",
      "sort_key",
      "updated_at_ms",
      [
        "realm",
        "updated_at_ms"
      ],
      [
        "asset_type",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_attributes: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 360
      },
      "realm": {
        "type": "string"
      },
      "asset_id": {
        "type": "string"
      },
      "attribute_name": {
        "type": "string"
      },
      "value_type": {
        "type": "string"
      },
      "timestamp_ms": {
        "type": "number"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "asset_id",
      "attribute_name",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "asset_id",
      "realm",
      "status_key",
      "sort_key",
      "updated_at_ms",
      [
        "asset_id",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_datapoints: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 420
      },
      "realm": {
        "type": "string"
      },
      "asset_id": {
        "type": "string"
      },
      "attribute_name": {
        "type": "string"
      },
      "from_ms": {
        "type": "number"
      },
      "to_ms": {
        "type": "number"
      },
      "shape": {
        "type": "string"
      },
      "point_count": {
        "type": "number"
      },
      "truncated": {
        "type": "boolean"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "asset_id",
      "attribute_name",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "asset_id",
      "status_key",
      "sort_key",
      "updated_at_ms",
      [
        "asset_id",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
  iot_realms: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "name": {
        "type": "string"
      },
      "parent_realm": {
        "type": "string"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "name",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "sort_key",
      "status_key",
      "updated_at_ms"
    ],
    "additionalProperties": true
  },
  iot_rulesets: {
    "version": 0,
    "primaryKey": "id",
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "maxLength": 180
      },
      "realm": {
        "type": "string"
      },
      "name": {
        "type": "string"
      },
      "enabled": {
        "type": "boolean"
      },
      "last_fired_ms": {
        "type": "number"
      },
      "data": {
        "type": "object",
        "additionalProperties": true
      },
      "index_text": {
        "type": "string"
      },
      "sort_key": {
        "type": "string"
      },
      "status_key": {
        "type": "string"
      },
      "created_at_ms": {
        "type": "number"
      },
      "updated_at_ms": {
        "type": "number"
      },
      "_deleted": {
        "type": "boolean"
      }
    },
    "required": [
      "id",
      "realm",
      "name",
      "data",
      "updated_at_ms"
    ],
    "indexes": [
      "realm",
      "status_key",
      "sort_key",
      "updated_at_ms",
      [
        "realm",
        "updated_at_ms"
      ]
    ],
    "additionalProperties": true
  },
};

export const migrationStrategies = {
  iot_agent_status: {},
  iot_agents: {},
  iot_alarms: {},
  iot_asset_types: {},
  iot_assets: {},
  iot_attributes: {},
  iot_datapoints: {},
  iot_realms: {},
  iot_rulesets: {},
};
