// Browser-side RxDB schemas. Must match collections.schema.json exactly
// (names, versions, primary keys, required fields, property types).

const newsTopicsSchema = {
  version: 0,
  primaryKey: "id",
  type: "object",
  properties: {
    id: {
      type: "string",
      maxLength: 160
    },
    title: {
      type: "string"
    },
    prompt: {
      type: "string"
    },
    cadence: {
      type: "string"
    },
    run_time: {
      type: "string"
    },
    weekday: {
      type: "number"
    },
    deadline_date: {
      type: "string"
    },
    cron_expr: {
      type: "string"
    },
    language: {
      type: "string"
    },
    categories: {
      type: "array",
      items: {
        type: "string"
      }
    },
    max_posts: {
      type: "number"
    },
    enabled: {
      type: "boolean"
    },
    schedule_task_id: {
      type: "string"
    },
    last_issue_at_ms: {
      type: "number"
    },
    sort_order: {
      type: "number"
    },
    payload: {
      type: "object",
      additionalProperties: true
    },
    created_at_ms: {
      type: "number"
    },
    updated_at_ms: {
      type: "number"
    }
  },
  required: [
    "id",
    "title",
    "cadence",
    "enabled",
    "payload",
    "created_at_ms",
    "updated_at_ms"
  ],
  additionalProperties: true
};

const newsSourcesSchema = {
  version: 0,
  primaryKey: "id",
  type: "object",
  properties: {
    id: {
      type: "string",
      maxLength: 180
    },
    topic_id: {
      type: "string"
    },
    kind: {
      type: "string"
    },
    label: {
      type: "string"
    },
    url: {
      type: "string"
    },
    adapter_kind: {
      type: "string"
    },
    target_key: {
      type: "string"
    },
    auth_mode: {
      type: "string"
    },
    credential_secret_name: {
      type: "string"
    },
    account_handle: {
      type: "string"
    },
    items: {
      type: "array",
      items: {
        type: "string"
      }
    },
    enabled: {
      type: "boolean"
    },
    status: {
      type: "string"
    },
    last_error: {
      type: "string"
    },
    last_run_at_ms: {
      type: "number"
    },
    payload: {
      type: "object",
      additionalProperties: true
    },
    created_at_ms: {
      type: "number"
    },
    updated_at_ms: {
      type: "number"
    }
  },
  required: [
    "id",
    "topic_id",
    "kind",
    "label",
    "enabled",
    "payload",
    "created_at_ms",
    "updated_at_ms"
  ],
  additionalProperties: true
};

const newsIssuesSchema = {
  version: 0,
  primaryKey: "id",
  type: "object",
  properties: {
    id: {
      type: "string",
      maxLength: 180
    },
    topic_id: {
      type: "string"
    },
    title: {
      type: "string"
    },
    issue_date: {
      type: "string"
    },
    status: {
      type: "string"
    },
    command_id: {
      type: "string"
    },
    task_id: {
      type: "string"
    },
    post_count: {
      type: "number"
    },
    generated_at_ms: {
      type: "number"
    },
    error: {
      type: "string"
    },
    payload: {
      type: "object",
      additionalProperties: true
    },
    created_at_ms: {
      type: "number"
    },
    updated_at_ms: {
      type: "number"
    }
  },
  required: [
    "id",
    "topic_id",
    "issue_date",
    "status",
    "payload",
    "created_at_ms",
    "updated_at_ms"
  ],
  additionalProperties: true
};

const newsPostsSchema = {
  version: 0,
  primaryKey: "id",
  type: "object",
  properties: {
    id: {
      type: "string",
      maxLength: 220
    },
    issue_id: {
      type: "string"
    },
    topic_id: {
      type: "string"
    },
    rank: {
      type: "number"
    },
    category: {
      type: "string"
    },
    author_name: {
      type: "string"
    },
    author_handle: {
      type: "string"
    },
    author_kind: {
      type: "string"
    },
    intro: {
      type: "string"
    },
    body: {
      type: "string"
    },
    link_card: {
      type: "object",
      additionalProperties: true
    },
    quote: {
      type: "object",
      additionalProperties: true
    },
    sources: {
      type: "array",
      items: {
        type: "object",
        additionalProperties: true
      }
    },
    hype_score: {
      type: "number"
    },
    discussion_count: {
      type: "number"
    },
    published_at_ms: {
      type: "number"
    },
    bookmarked: {
      type: "boolean"
    },
    payload: {
      type: "object",
      additionalProperties: true
    },
    created_at_ms: {
      type: "number"
    },
    updated_at_ms: {
      type: "number"
    }
  },
  required: [
    "id",
    "issue_id",
    "topic_id",
    "intro",
    "payload",
    "created_at_ms",
    "updated_at_ms"
  ],
  additionalProperties: true
};

export const collections = {
  news_topics: newsTopicsSchema,
  news_sources: newsSourcesSchema,
  news_issues: newsIssuesSchema,
  news_posts: newsPostsSchema,
};

export const migrationStrategies = {};
