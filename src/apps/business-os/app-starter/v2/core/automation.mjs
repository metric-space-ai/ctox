export function buildSignatureCommand(record, archetype) {
  return {
    command_type: archetype.command_type || 'business_os.chat.task',
    title: `${archetype.signature_action}: ${record.title || record.id}`,
    payload: {
      instruction: `${archetype.signature_action} for ${record.title || record.id}`,
      record_snapshot: record,
      archetype: archetype.id
    },
    client_context: {
      source: 'business-os-app-starter-v2',
      record_id: record.id,
      surface: `${archetype.id}.signature-action`
    }
  };
}
