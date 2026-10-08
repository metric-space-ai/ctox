// Generated from src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json. Do not edit.
export const JOUR_FIXE_SCHEMA = "ctox.workjet.jour_fixe.v1";
export const JOUR_FIXE_VERSION = 1;
export const JOUR_FIXE_TYPES = deepFreeze({
  "MeetingState": {
    "enum": [
      "planned",
      "preparing",
      "ready",
      "live",
      "review",
      "confirmed",
      "cancelled",
      "failed"
    ]
  },
  "Speaker": {
    "enum": [
      "owner",
      "supervisor"
    ]
  },
  "Modality": {
    "enum": [
      "text",
      "speech"
    ]
  },
  "TodoState": {
    "enum": [
      "proposed",
      "confirmed",
      "superseded"
    ]
  },
  "Priority": {
    "enum": [
      "P0",
      "P1",
      "P2"
    ]
  },
  "SupervisorRef": {
    "fields": {
      "workjet_thread_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "ctox_thread_key": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 512
      }
    }
  },
  "GoalRef": {
    "fields": {
      "goal_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64"
      }
    }
  },
  "AudioRef": {
    "fields": {
      "file_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "mime_type": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "duration_ms": {
        "type": "u64"
      },
      "narration_text_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "source_run_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "model": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "format": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 32
      },
      "synthesis_duration_ms": {
        "type": "u64"
      },
      "provenance": {
        "type": "AudioProvenance",
        "optional": true
      },
      "generation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128,
        "optional": true
      }
    }
  },
  "Slide": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "position": {
        "type": "u64"
      },
      "title": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "body_markdown": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 16384
      },
      "audio": {
        "type": "AudioRef",
        "optional": true
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "Comment": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "slide_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64"
      },
      "x": {
        "type": "f64",
        "minimum": 0,
        "maximum": 1
      },
      "y": {
        "type": "f64",
        "minimum": 0,
        "maximum": 1
      },
      "text": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 4096
      },
      "author_user_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "created_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "supervisor_event_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "TranscriptTurn": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "sequence": {
        "type": "u64"
      },
      "speaker": {
        "type": "Speaker"
      },
      "modality": {
        "type": "Modality"
      },
      "text": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 16384
      },
      "started_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "ended_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "source_run_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 128
      },
      "audio": {
        "type": "AudioRef",
        "optional": true
      },
      "stream_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 128
      },
      "sentence_end_latency_ms": {
        "type": "u64",
        "optional": true
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "Todo": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "title": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 512
      },
      "acceptance": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 4096
      },
      "priority": {
        "type": "Priority"
      },
      "evidence_ids": {
        "type": "Vec<String>",
        "max_items": 128
      },
      "due_at_ms": {
        "type": "i64",
        "optional": true,
        "minimum": 0
      },
      "owner": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 256
      }
    }
  },
  "TodoList": {
    "fields": {
      "revision": {
        "type": "u64"
      },
      "status": {
        "type": "TodoState"
      },
      "items": {
        "type": "Vec<Todo>",
        "max_items": 100
      },
      "confirmed_by_user_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 256
      },
      "confirmed_at_ms": {
        "type": "i64",
        "optional": true,
        "minimum": 0
      },
      "goal": {
        "type": "GoalRef",
        "optional": true
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "Meeting": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "owner_user_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "supervisor": {
        "type": "SupervisorRef"
      },
      "scheduled_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "prepare_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "timezone": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "state": {
        "type": "MeetingState"
      },
      "revision": {
        "type": "u64"
      },
      "deck_revision": {
        "type": "u64"
      },
      "previous_goal": {
        "type": "GoalRef",
        "optional": true
      },
      "slides": {
        "type": "Vec<Slide>",
        "max_items": 100
      },
      "comments": {
        "type": "Vec<Comment>",
        "max_items": 1000
      },
      "transcript": {
        "type": "Vec<TranscriptTurn>",
        "max_items": 10000
      },
      "todos": {
        "type": "TodoList",
        "optional": true
      },
      "error": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 4096
      }
    }
  },
  "PrepareRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      }
    }
  },
  "PublishDeckRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      },
      "deck_revision": {
        "type": "u64"
      },
      "slides": {
        "type": "Vec<Slide>",
        "min_items": 1,
        "max_items": 100
      }
    }
  },
  "MeetingTransitionRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      }
    }
  },
  "AddCommentRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      },
      "comment_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "slide_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64"
      },
      "x": {
        "type": "f64",
        "minimum": 0,
        "maximum": 1
      },
      "y": {
        "type": "f64",
        "minimum": 0,
        "maximum": 1
      },
      "text": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 4096
      }
    }
  },
  "AppendTranscriptRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      },
      "turn": {
        "type": "TranscriptTurn"
      }
    }
  },
  "ProposeTodosRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      },
      "proposal_revision": {
        "type": "u64"
      },
      "items": {
        "type": "Vec<Todo>",
        "max_items": 100
      }
    }
  },
  "ConfirmTodosRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "expected_revision": {
        "type": "u64"
      },
      "proposal_revision": {
        "type": "u64"
      },
      "expected_goal_revision": {
        "type": "u64"
      }
    }
  },
  "TranscriptEvent": {
    "fields": {
      "stream_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "sequence": {
        "type": "u64"
      },
      "is_final": {
        "type": "bool"
      },
      "text": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 16384
      },
      "started_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "ended_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "sentence_end_latency_ms": {
        "type": "u64",
        "optional": true
      }
    }
  },
  "ReadMeetingRequest": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 128
      }
    }
  },
  "ListMeetingsRequest": {
    "fields": {
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "limit": {
        "type": "u64",
        "minimum": 1,
        "maximum": 20
      }
    }
  },
  "MeetingSummary": {
    "fields": {
      "id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "scheduled_at_ms": {
        "type": "u64"
      },
      "state": {
        "type": "MeetingState"
      },
      "revision": {
        "type": "u64"
      },
      "todo_count": {
        "type": "u64"
      }
    }
  },
  "MeetingMutationReceipt": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "revision": {
        "type": "u64"
      },
      "state": {
        "type": "MeetingState"
      },
      "changed_id": {
        "type": "String",
        "optional": true,
        "min_chars": 1,
        "max_chars": 128
      },
      "todos_revision": {
        "type": "u64",
        "optional": true
      }
    }
  },
  "AudioProvenance": {
    "enum": [
      "native_gateway",
      "authenticated_owner_local_audio"
    ]
  },
  "LocalNarrationRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "instance_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "slide_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "file_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "generation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64",
        "minimum": 1
      },
      "expected_revision": {
        "type": "u64"
      },
      "audio_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "narration_text_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      }
    }
  },
  "LocalNarrationReceipt": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "instance_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "slide_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64",
        "minimum": 1
      },
      "owner_user_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "revision": {
        "type": "u64"
      },
      "audio": {
        "type": "AudioRef"
      },
      "persisted_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "provenance": {
        "type": "AudioProvenance"
      },
      "provider_verified": {
        "type": "bool"
      }
    }
  },
  "LocalTranscriptProvenance": {
    "enum": [
      "authenticated_owner_local_candidate"
    ]
  },
  "LocalTranscriptCandidateRequest": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "request_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "instance_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64",
        "minimum": 1
      },
      "expected_revision": {
        "type": "u64"
      },
      "text": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 4096
      }
    }
  },
  "LocalTranscriptCandidateReceipt": {
    "fields": {
      "operation_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "request_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "instance_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "project_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "meeting_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "deck_revision": {
        "type": "u64",
        "minimum": 1
      },
      "owner_user_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 256
      },
      "turn_id": {
        "type": "String",
        "min_chars": 1,
        "max_chars": 128
      },
      "sequence": {
        "type": "u64",
        "minimum": 1
      },
      "revision": {
        "type": "u64",
        "minimum": 1
      },
      "text_sha256": {
        "type": "String",
        "min_chars": 64,
        "max_chars": 64
      },
      "persisted_at_ms": {
        "type": "i64",
        "minimum": 0
      },
      "provenance": {
        "type": "LocalTranscriptProvenance"
      },
      "provider_verified": {
        "type": "bool"
      }
    }
  }
});
export const JOUR_FIXE_COMMANDS = deepFreeze({
  "ctox.workjet.jour_fixe.prepare": {
    "request_type": "PrepareRequest",
    "authorization": "bound_supervisor"
  },
  "ctox.workjet.jour_fixe.deck.publish": {
    "request_type": "PublishDeckRequest",
    "authorization": "bound_supervisor"
  },
  "ctox.workjet.jour_fixe.meeting.start": {
    "request_type": "MeetingTransitionRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.meeting.end": {
    "request_type": "MeetingTransitionRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.comment.add": {
    "request_type": "AddCommentRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.transcript.local_candidate": {
    "request_type": "LocalTranscriptCandidateRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.transcript.append": {
    "request_type": "AppendTranscriptRequest",
    "authorization": "owner_or_bound_supervisor"
  },
  "ctox.workjet.jour_fixe.todos.propose": {
    "request_type": "ProposeTodosRequest",
    "authorization": "bound_supervisor"
  },
  "ctox.workjet.jour_fixe.todos.confirm": {
    "request_type": "ConfirmTodosRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.todos.revise": {
    "request_type": "ProposeTodosRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.meeting.read": {
    "request_type": "ReadMeetingRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.meetings.list": {
    "request_type": "ListMeetingsRequest",
    "authorization": "owner"
  },
  "ctox.workjet.jour_fixe.narration.local_publish": {
    "request_type": "LocalNarrationRequest",
    "authorization": "owner"
  }
});

function deepFreeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(deepFreeze); Object.freeze(value); }
  return value;
}
export function validateJourFixeValue(typeName, value) {
  function validate(type, value, field) {
    if (type.startsWith('Vec<')) {
      if (!Array.isArray(value)) throw new Error(field + ': array required');
      value.forEach(v => validate(type.slice(4,-1), v, field)); return;
    }
    if (type === 'String') { if (typeof value !== 'string') throw new Error(field + ': string required'); return; }
    if (type === 'bool') { if (typeof value !== 'boolean') throw new Error(field + ': boolean required'); return; }

    if (['u64','i64','f64'].includes(type)) {
      if (typeof value !== 'number' || !Number.isFinite(value)
          || (type !== 'f64' && (!Number.isSafeInteger(value) || (type === 'u64' && value < 0)))) throw new Error(field + ': invalid number');
      return;
    }
    const shape = JOUR_FIXE_TYPES[type];
    if (!shape) throw new Error(field + ': unknown type');
    if (shape.enum) { if (!shape.enum.includes(value)) throw new Error(field + ': invalid enum'); return; }
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(field + ': object required');
    for (const key of Object.keys(value)) if (!Object.hasOwn(shape.fields, key)) throw new Error(field + ': unexpected ' + key);
    for (const [key, definition] of Object.entries(shape.fields)) {
      const v = value[key], at = field + '.' + key;
      if (v === undefined || v === null) { if (definition.optional) continue; throw new Error(at + ': required'); }
      validate(definition.type, v, at);
      for (const [constraint,bound] of Object.entries(definition)) {
        const metric = constraint.endsWith('_chars') ? [...v].length : constraint.endsWith('_items') ? v.length : v;
        if ((['minimum','min_chars','min_items'].includes(constraint) && metric < bound)
            || (['maximum','max_chars','max_items'].includes(constraint) && metric > bound)) throw new Error(at + ': ' + constraint);
      }
    }
  }
  try { validate(typeName, value, typeName); return {ok:true}; }
  catch (error) { return {ok:false, error:error.message}; }
}
