import { CtoxError } from './values.mjs';

/** The only core-facing seam. Implementations must bind a connection to exactly
 * one (instanceId,userId). page is the single initial/live delivery message.
 * No app query calls this interface; subscriptions are planned by the kernel.
 */
export class KernelTransport {
  async connect(_identity, _events) {}
  async subscribe(_subscription) {} // offline: retain subscription intent without manufacturing pages
  async write(_entry) { throw new CtoxError('unavailable', 'No transport configured'); }
  async appActivation(_report) {}
  priority(_hint) {}
  async artifact(_hash) { throw new CtoxError('unavailable', 'No artifact transport'); }
  async blob(_ref) { throw new CtoxError('unavailable', 'No file transport'); }
  async upload(_ref, _blob) { throw new CtoxError('unavailable', 'No file transport'); }
  close() {}
}

export const decimalSeq = (value) => {
  if (typeof value === 'number' && (!Number.isSafeInteger(value) || value < 0)) throw new CtoxError('invalid', 'Unsafe sequence');
  const text = String(value ?? '0');
  if (!/^(0|[1-9][0-9]*)$/.test(text)) throw new CtoxError('invalid', 'Sequence must be an unsigned decimal string');
  return text;
};
export const seqGreater = (a, b) => BigInt(decimalSeq(a)) > BigInt(decimalSeq(b));
export const maxSeq = (a,b) => seqGreater(a,b) ? decimalSeq(a) : decimalSeq(b);
export const checkpointKey = ({ epoch = '1', collection, scope_hash = 'local', gen = 1 }) =>
  `cursor:${JSON.stringify([String(epoch), collection, scope_hash, gen])}`;
