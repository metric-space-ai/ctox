// V4 control-lane contract; no transport, UI, credential store or V3 fallback.
export const PROTOCOL_VERSION = 4;
// hello precedes codec/fragment negotiation and must fit one plain frame.
export const MAX_HELLO_BYTES = 16384;
export const MAX_FRAME_BYTES = 16384;
const MAX_U64 = 18446744073709551615n;
const own = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
const record = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value) => typeof value === 'string' && value.trim().length > 0;
const byteLength = (value) => new TextEncoder().encode(value).byteLength;
const names = (value) => Array.isArray(value) && value.length <= 128
  && value.every((name) => text(name) && byteLength(name) <= 256);
const b64 = (value, length) => typeof value === 'string'
  && value.length === length && /^[A-Za-z0-9_-]+$/.test(value);
const integer = (value) => Number.isInteger(value) && value >= 0 && value <= 65535;

export function decimalSeq(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)
      || value.length > 20 || BigInt(value) > MAX_U64) {
    throw new TypeError('expected canonical decimal u64 string');
  }
  return value;
}
function version(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(value)) return null;
  const [major, minor] = value.split('.').map(Number);
  return integer(major) && integer(minor) ? major * 65536 + minor : null;
}
function bounds(range) {
  if (!record(range)) return null;
  const min = version(range.min), max = version(range.max);
  return min !== null && max !== null && min <= max ? [min, max] : null;
}
const versionText = (value) => `${Math.floor(value / 65536)}.${value % 65536}`;
const REASONS = new Set(['invalid_hello', 'protocol_incompatible', 'bootstrap_incompatible',
  'app_api_incompatible', 'instance_mismatch', 'capability_missing', 'transport_incompatible',
  'device_proof_invalid', 'authentication_failed', 'handshake_timeout', 'unexpected_message']);

export class HelloRejected extends Error {
  constructor(reason, required) {
    super(`hello rejected: ${reason}; required ${required}`);
    this.name = 'HelloRejected';
    this.reason = reason;
    this.required = required;
  }
  toFrame() { return { type: 'hello_reject', reason: this.reason, required: this.required }; }
}
function reject(reason, required) { throw new HelloRejected(reason, required); }
function fields(value, expected) {
  return record(value) && Object.keys(value).every((key) => expected.includes(key))
    && expected.every((key) => own(value, key));
}
const HELLO_FIELDS = ['type', 'protocol_ver', 'bootstrap_ver', 'app_api_ver', 'instance_id',
  'capabilities', 'device_proof', 'scopes_wanted', 'transport'];
const OK_FIELDS = ['type', 'protocol_ver', 'bootstrap_ver', 'app_api_ver', 'instance_id',
  'capabilities', 'transport', 'epoch', 'head_seq', 'scope_tokens', 'rights_ver'];

function offer(value) {
  return fields(value, ['codecs', 'max_frame_bytes', 'fragmentation', 'reassembly'])
    && Array.isArray(value.codecs) && value.codecs.length <= 128
    && value.codecs.every((codec) => codec === 'zstd' || codec === 'gzip')
    && Number.isInteger(value.max_frame_bytes) && value.max_frame_bytes >= 0
    && value.max_frame_bytes <= 4294967295
    && typeof value.fragmentation === 'boolean' && typeof value.reassembly === 'boolean';
}
function proof(value) {
  return fields(value, ['device_id', 'public_key', 'nonce', 'signature'])
    && text(value.device_id) && byteLength(value.device_id) <= 256
    && b64(value.public_key, 43) && b64(value.nonce, 43) && b64(value.signature, 86);
}
export function decodeHelloFrame(input) {
  const encoded = typeof input === 'string' ? input : null;
  if (encoded === null || new TextEncoder().encode(encoded).byteLength > MAX_HELLO_BYTES) {
    reject('invalid_hello', 'hello <= 16384 bytes');
  }
  let frame;
  try { frame = JSON.parse(encoded); } catch { reject('invalid_hello', 'valid V4 control frame'); }
  if (!record(frame)) reject('invalid_hello', 'valid V4 control frame');
  if (frame.type === 'hello') {
    if (!fields(frame, HELLO_FIELDS) || !integer(frame.protocol_ver) || !integer(frame.bootstrap_ver)
        || !fields(frame.app_api_ver, ['min', 'max']) || !bounds(frame.app_api_ver)
        || !text(frame.instance_id) || !names(frame.capabilities) || !names(frame.scopes_wanted)
        || !proof(frame.device_proof) || !offer(frame.transport)) {
      reject('invalid_hello', 'valid hello fields');
    }
  } else if (frame.type === 'hello_ok') {
    const transport = frame.transport;
    if (!fields(frame, OK_FIELDS) || !integer(frame.protocol_ver) || !integer(frame.bootstrap_ver)
        || version(frame.app_api_ver) === null || !text(frame.instance_id) || !text(frame.epoch)
        || !names(frame.capabilities) || !Array.isArray(frame.scope_tokens)
        || frame.scope_tokens.some((token) => !text(token))
        || !fields(transport, ['codec', 'frame_bytes', 'fragmentation', 'reassembly'])
        || !['zstd', 'gzip'].includes(transport.codec) || !Number.isInteger(transport.frame_bytes)
        || transport.frame_bytes < 1024 || transport.frame_bytes > MAX_FRAME_BYTES
        || transport.fragmentation !== true || transport.reassembly !== true) {
      reject('invalid_hello', 'valid hello_ok fields');
    }
    try { decimalSeq(frame.head_seq); decimalSeq(frame.rights_ver); }
    catch { reject('invalid_hello', 'canonical decimal u64 sequences'); }
  } else if (frame.type === 'hello_reject') {
    if (!fields(frame, ['type', 'reason', 'required']) || !REASONS.has(frame.reason)
        || !text(frame.required) || byteLength(frame.required) > 256) {
      reject('invalid_hello', 'valid hello_reject fields');
    }
  } else reject('invalid_hello', 'hello, hello_ok or hello_reject');
  return frame;
}

// Symmetric compatibility selection; this function grants no authority.
export function negotiateHello(policy, input) {
  const hello = decodeHelloFrame(JSON.stringify(input));
  if (hello.type !== 'hello') reject('unexpected_message', 'hello first');
  if (hello.protocol_ver !== PROTOCOL_VERSION) reject('protocol_incompatible', 'protocol_ver=4');
  if (!text(policy.instance_id) || hello.instance_id !== policy.instance_id)
    reject('instance_mismatch', 'paired instance');
  if (!integer(policy.bootstrap_min) || !integer(policy.bootstrap_max)
      || policy.bootstrap_min > policy.bootstrap_max
      || hello.bootstrap_ver < policy.bootstrap_min || hello.bootstrap_ver > policy.bootstrap_max)
    reject('bootstrap_incompatible', 'supported bootstrap version');
  const local = bounds(policy.app_api_ver), remote = bounds(hello.app_api_ver);
  if (!local || !remote || Math.max(local[0], remote[0]) > Math.min(local[1], remote[1]))
    reject('app_api_incompatible', 'overlapping app API range');
  if (!names(policy.capabilities) || !names(policy.required_capabilities)
      || policy.required_capabilities.some((name) => !policy.capabilities.includes(name)
        || !hello.capabilities.includes(name))) reject('capability_missing', 'required capabilities');
  const ours = policy.transport, theirs = hello.transport;
  const codec = ['zstd', 'gzip'].find((item) => ours?.codecs?.includes(item) && theirs.codecs.includes(item));
  if (!offer(ours) || !codec || !ours.fragmentation || !ours.reassembly
      || !theirs.fragmentation || !theirs.reassembly
      || ours.max_frame_bytes < 1024 || theirs.max_frame_bytes < 1024)
    reject('transport_incompatible', 'common codec; fragmentation/reassembly; frame >= 1024');
  return {
    bootstrap_ver: hello.bootstrap_ver,
    app_api_ver: versionText(Math.min(local[1], remote[1])),
    capabilities: [...new Set(policy.capabilities.filter((name) => hello.capabilities.includes(name)))].sort(),
    transport: { codec, frame_bytes: Math.min(ours.max_frame_bytes, theirs.max_frame_bytes, MAX_FRAME_BYTES),
      fragmentation: true, reassembly: true },
  };
}

/** Browser initiator, per peer. Host owns the WebRTC channel and signer. State
 * callbacks contain only state/reason; scope tokens remain in the return value.
 * The host calls connected() on open, sends its result through control lane 0,
 * then passes hello_ok/reject to receive(). No UI or persistence in this module.
 */
export class HelloConnection {
  #hello;
  #state = 'connecting';
  #report;
  #rejection = null;
  #timer = null;
  #timeout;
  #requiredCapabilities;
  constructor(hello, report, { handshakeTimeoutMs = 10000, requiredCapabilities = [] } = {}) {
    this.#hello = decodeHelloFrame(JSON.stringify(hello));
    if (this.#hello.type !== 'hello') reject('unexpected_message', 'hello first');
    if (typeof report !== 'function') throw new TypeError('state reporter is required');
    if (!Number.isInteger(handshakeTimeoutMs) || handshakeTimeoutMs <= 0 || handshakeTimeoutMs > 60000)
      throw new TypeError('bounded handshake deadline is required');
    if (!names(requiredCapabilities)
        || requiredCapabilities.some((name) => !this.#hello.capabilities.includes(name)))
      throw new TypeError('required capabilities must be offered');
    this.#requiredCapabilities = [...requiredCapabilities];
    this.#report = report;
    this.#timeout = handshakeTimeoutMs;
    // A real deadline, never a polling/retry timer. It includes connection setup.
    this.#timer = setTimeout(() => {
      this.#rejection = Object.freeze({ reason: 'handshake_timeout', required: 'hello before deadline' });
      this.close();
    }, this.#timeout);
    this.#emit();
  }
  get snapshot() { return Object.freeze({ state: this.#state, rejection: this.#rejection }); }
  #emit() { this.#report(this.snapshot); }
  #set(state) { this.#state = state; this.#emit(); }
  connected() {
    if (this.#state !== 'connecting') reject('unexpected_message', 'connecting state');
    this.#set('authenticating');
    // Fresh clone: caller cannot mutate the retained compatibility offer.
    return JSON.parse(JSON.stringify(this.#hello));
  }
  receive(input) {
    if (this.#state !== 'authenticating') reject('unexpected_message', 'authenticating state');
    try {
      const frame = decodeHelloFrame(input);
      if (frame.type === 'hello_reject') reject(frame.reason, frame.required);
      if (frame.type !== 'hello_ok') reject('unexpected_message', 'hello_ok or hello_reject');
      const hello = this.#hello;
      if (frame.protocol_ver !== hello.protocol_ver) reject('protocol_incompatible', 'offered protocol');
      if (frame.bootstrap_ver !== hello.bootstrap_ver) reject('bootstrap_incompatible', 'offered bootstrap');
      if (frame.instance_id !== hello.instance_id) reject('instance_mismatch', 'paired instance');
      const range = bounds(hello.app_api_ver), selected = version(frame.app_api_ver);
      if (selected < range[0] || selected > range[1]) reject('app_api_incompatible', 'offered API range');
      if (frame.capabilities.some((name) => !hello.capabilities.includes(name))
          || this.#requiredCapabilities.some((name) => !frame.capabilities.includes(name)))
        reject('capability_missing', 'offered capabilities');
      if (!hello.transport.codecs.includes(frame.transport.codec)
          || frame.transport.frame_bytes > hello.transport.max_frame_bytes
          || !hello.transport.fragmentation || !hello.transport.reassembly)
        reject('transport_incompatible', 'offered transport limits');
      this.#set('negotiated');
      return frame;
    } catch (error) {
      const failure = error instanceof HelloRejected ? error : new HelloRejected('invalid_hello', 'valid V4 control frame');
      this.#rejection = Object.freeze({ reason: failure.reason, required: failure.required });
      this.close();
      throw failure;
    }
  }
  activate() {
    if (this.#state !== 'negotiated') reject('unexpected_message', 'negotiated state');
    clearTimeout(this.#timer);
    this.#timer = null;
    this.#set('live');
  }
  close() {
    if (this.#state === 'closed') return;
    clearTimeout(this.#timer);
    this.#timer = null;
    this.#set('draining');
    this.#set('closed');
  }
}
