// Legacy forwarder (§12 Nr. 6, §16.16): command types whose Rust logic is not
// ported yet run unchanged in the native Core through `ctx.cap.legacy.<type>`.
// Each forwarder is declared in the manifest (capabilities_required) and counted
// as open on the board until its port lands. The command is passed through
// unchanged; the result is the native result.

export function legacyForwarder(commandType) {
  return async function handle(command, ctx) {
    const legacy = ctx.cap.legacy[commandType];
    return legacy({
      id: command.id,
      type: command.type,
      payload: command.payload,
      actor: command.actor,
      app_id: command.app_id,
      app_ver: command.app_ver,
    });
  };
}
