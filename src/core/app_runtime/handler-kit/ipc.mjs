// Same manifest contract as the native Core pre-dispatch check.
import {CtoxError} from './mango.mjs';
export function commandTarget(manifest, type, options = {}) {
  const fail = message => { throw new CtoxError({code:'forbidden',message}); };
  if (options.module !== undefined) {
    if (typeof options.module !== 'string' || !options.module) fail('Invalid command module');
    return options.module;
  }
  if (Object.hasOwn(manifest.handlers ?? {}, type)) return manifest.app_id;
  const targets = [...new Set((manifest.commands_out ?? []).filter(r => r.type === type).map(r => r.app))];
  if (targets.length !== 1) fail('Command target missing or ambiguous');
  return targets[0];
}
export function commandDeclarations(sender, receiver, type, options = {}) {
  const fail = message => { throw new CtoxError({code:'forbidden',message}); };
  if (!receiver || !Object.hasOwn(receiver.handlers ?? {}, type)) fail('Target handler missing');
  if (sender.app_id !== receiver.app_id) {
    if (!(sender.commands_out ?? []).some(r => r.app === receiver.app_id && r.type === type)) fail('Sender commands_out missing');
    if (!(receiver.commands_in ?? []).some(r => r.type === type && (r.from === '*' || Array.isArray(r.from) && r.from.includes(sender.app_id)))) fail('Receiver commands_in missing');
  }
  for (const key of ['on_complete','on_failed']) if (options[key] !== undefined &&
      (typeof options[key] !== 'string' || !Object.hasOwn(sender.handlers ?? {}, options[key]))) fail('Callback must belong to the triggering app');
}
