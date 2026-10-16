// Kernel owns authentication; V3 compatibility is explicit and never a V4 fallback.
import {withTimeout} from './vendor/async-timeout.js';
import {decodeHelloFrame} from './vendor/hello.mjs';
import {CtoxError} from './values.mjs';
const COMMAND_CAPABILITY_TIMEOUT_MS = 120000;
const COMMAND_CAPABILITY_TERMINAL_NEGATIVE_CACHE_MS = 10000;
const COMMAND_CAPABILITY_TRANSIENT_ANTI_STORM_MS = 100;

let capabilityTokenCache = {
  token: null,
  expiresAtMs: 0,
  failureUntilMs: 0,
  failureCode: '',
  failureTransient: false,
};
let capabilityTokenRequestInFlight = null;

export async function getV3CapabilityToken({
  timeoutMs = COMMAND_CAPABILITY_TIMEOUT_MS,
  refresh = false,
} = {}) {
  const result = await acquireBusinessOsCapabilityToken({ timeoutMs, refresh });
  return result.token;
}

export async function acquireBusinessOsCapabilityToken({
  timeoutMs = COMMAND_CAPABILITY_TIMEOUT_MS,
  refresh = false,
} = {}) {
  const now = Date.now();
  if (!refresh && capabilityTokenCache.token && now < capabilityTokenCache.expiresAtMs - 60_000) {
    return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
  }
  if (now < capabilityTokenCache.failureUntilMs) {
    return capabilityAcquisitionResult({
      code: capabilityTokenCache.failureCode,
      transient: capabilityTokenCache.failureTransient,
    });
  }
  const injected = injectedBusinessOsCapabilityToken(now);
  if (injected?.token) {
    capabilityTokenCache = {
      ...injected,
      failureUntilMs: 0,
      failureCode: '',
      failureTransient: false,
    };
    return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
  }
  // A native peer reconfiguration can change the grant epoch without expiring
  // the token. A protocol handshake renews HTTP-session authority; concurrent
  // reads/submits join that request instead of consuming the old positive cache.
  // Device-injected authority above stays host-owned and negative caches remain
  // in force. Renewal never authorizes a locally asserted identity.
  capabilityTokenCache.token = null;
  capabilityTokenCache.expiresAtMs = 0;
  if (!capabilityTokenRequestInFlight) {
    capabilityTokenRequestInFlight = requestBusinessOsCapabilityToken(timeoutMs);
  }
  const inFlight = capabilityTokenRequestInFlight;
  try {
    return await inFlight;
  } finally {
    if (capabilityTokenRequestInFlight === inFlight) {
      capabilityTokenRequestInFlight = null;
    }
  }
}

async function requestBusinessOsCapabilityToken(timeoutMs) {
  const abortController = typeof AbortController === 'function' ? new AbortController() : null;
  try {
    const res = await withTimeout(
      fetch('/api/business-os/auth/capability', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        cache: 'no-store',
        signal: abortController?.signal,
      }),
      timeoutMs,
      {
        code: 'native_unavailable',
        message: 'Business OS capability request timed out.',
        onTimeout: () => abortController?.abort(),
      },
    );
    if (!res.ok) {
      const status = Number(res.status || 0);
      const transient = !isTerminalCapabilityHttpStatus(status);
      const code = `capability_http_${status || 'unknown'}`;
      rememberCapabilityFailure(code, { transient });
      return capabilityAcquisitionResult({ code, transient });
    }
    const data = await res.json();
    if (data && data.capability_token) {
      capabilityTokenCache = {
        token: data.capability_token,
        expiresAtMs: Number(data.expires_at_ms) || Date.now() + 11 * 60 * 60 * 1000,
        failureUntilMs: 0,
        failureCode: '',
        failureTransient: false,
      };
      return capabilityAcquisitionResult({ token: capabilityTokenCache.token });
    }
    // A successful HTTP response without a token is not an authorization
    // rejection signal. Stay fail-closed, but classify the malformed control-
    // plane response as transient rather than pinning all submits for 10 s.
    rememberCapabilityFailure('capability_missing', { transient: true });
    return capabilityAcquisitionResult({ code: 'capability_missing', transient: true });
  } catch (error) {
    // fetch rejects for timeout/abort and network failures. HTTP authorization
    // rejection is handled above from its concrete 4xx status.
    const code = String(error?.code || error?.name || 'capability_unavailable');
    rememberCapabilityFailure(code, { transient: true });
    return capabilityAcquisitionResult({ code, transient: true });
  }
}

export function resetV3CapabilityTokenCacheForTests() { capabilityTokenCache={token:null,expiresAtMs:0,failureUntilMs:0,failureCode:'',failureTransient:false}; capabilityTokenRequestInFlight=null; }

function capabilityAcquisitionResult({ token = null, code = '', transient = false } = {}) {
  return {
    token: token || null,
    code: String(code || ''),
    transient: Boolean(transient),
  };
}

function isTerminalCapabilityHttpStatus(status) {
  return status >= 400 && status < 500;
}

function rememberCapabilityFailure(code, { transient }) {
  const cacheMs = transient
    ? COMMAND_CAPABILITY_TRANSIENT_ANTI_STORM_MS
    : COMMAND_CAPABILITY_TERMINAL_NEGATIVE_CACHE_MS;
  capabilityTokenCache = {
    token: null,
    expiresAtMs: 0,
    failureUntilMs: Date.now() + cacheMs,
    failureCode: String(code || 'capability_unavailable'),
    failureTransient: Boolean(transient),
  };
}


function injectedBusinessOsCapabilityToken(now = Date.now()) {
  const candidates = [
    globalThis.CTOX_BUSINESS_OS_SESSION,
    globalThis.ctoxBusinessOsSession,
    globalThis.ctoxBusinessOsLaunch?.session,
    globalThis.CTOX_DESKTOP_SESSION,
    globalThis.ctoxDesktop?.session,
  ].filter((item) => item && typeof item === 'object');
  for (const candidate of candidates) {
    const token = String(candidate.capability_token || candidate.capabilityToken || '').trim();
    if (!token) continue;
    const expiresAtMs = Number(candidate.capability_expires_at_ms || candidate.capabilityExpiresAtMs || 0)
      || now + 11 * 60 * 60 * 1000;
    if (expiresAtMs <= now + 60_000) continue;
    return { token, expiresAtMs };
  }
  return null;
}


/** Authenticated bootstrap supplies the device hello and instance-key verifier.
 * Scope grants stay in memory; a denied V4 session never consumes V3 authority. */
export function createKernelSession({instanceId,userId,hello,verifyScopeToken,resolveScope}) {
  if(!instanceId || !userId || typeof hello!=='function' || typeof verifyScopeToken!=='function')throw new CtoxError('forbidden','Authenticated bootstrap and scope verifier required');
  let selected=null;
  return Object.freeze({
    async hello(identity,challenge) {
      selected=null;
      if(identity.instanceId!==instanceId || identity.userId!==userId)throw new CtoxError('forbidden','Session identity mismatch');
      const offer=decodeHelloFrame(JSON.stringify(await hello(identity,challenge)));
      if(offer.type!=='hello' || offer.instance_id!==instanceId || (challenge && offer.device_proof.nonce!==challenge.nonce))throw new CtoxError('forbidden','Device hello identity mismatch');
      return offer;
    },
    async acceptHello(frame) {
      selected=null;
      const value=decodeHelloFrame(JSON.stringify(frame));
      if(value.type!=='hello_ok' || value.instance_id!==instanceId || !value.scope_tokens.length)throw new CtoxError('forbidden','Authenticated scope grant required');
      for(const token of value.scope_tokens)if(!await verifyScopeToken(token,{instanceId,userId,epoch:value.epoch,rights_ver:value.rights_ver}))throw new CtoxError('forbidden','Scope token verification failed');
      selected=value;
    },
    async scope(collection) {
      if(!selected || typeof resolveScope!=='function')throw new CtoxError('forbidden','Session has no admitted scope');
      const scope=await resolveScope(collection,selected);
      if(!scope?.scope_hash || !Number.isSafeInteger(scope.gen))throw new CtoxError('forbidden','Collection grant missing');
      return {...scope,collection,epoch:selected.epoch};
    },
    tokens(){if(!selected)throw new CtoxError('forbidden','No authenticated session');return [...selected.scope_tokens];},
    close(){selected=null;},
  });
}
