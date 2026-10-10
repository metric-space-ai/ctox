#!/usr/bin/env node
const crypto = require('crypto');
const net = require('net');

const host = process.env.SIGNALING_HOST || '127.0.0.1';
const port = Number(process.env.SIGNALING_PORT || process.argv[2] || 18990);
const debug = process.env.SIGNALING_DEBUG === '1';
const relayRtt = process.argv.find(arg => arg.startsWith('--sync-v3-relay-rtt='));
const relayEvidence = process.argv.find(arg => arg.startsWith('--sync-v3-relay-evidence='));
const relay = relayRtt ? new (require('../../../../scripts/sync-v3/udp-relay.cjs').UdpRelay)(
  Number(relayRtt.split('=')[1]), relayEvidence?.slice('--sync-v3-relay-evidence='.length),
) : null;
if (relay && host !== '127.0.0.1') throw Error('S0 relay signaling must bind loopback');
let relaySignals = Promise.resolve();

function token(len = 12) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  let out = '';
  for (let i = 0; i < len; i += 1) out += alphabet[Math.floor(Math.random() * alphabet.length)];
  return out;
}

function encodeFrame(text) {
  const payload = Buffer.from(text);
  if (payload.length < 126) return Buffer.concat([Buffer.from([0x81, payload.length]), payload]);
  if (payload.length < 65536) {
    const header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
    return Buffer.concat([header, payload]);
  }
  const header = Buffer.alloc(10);
  header[0] = 0x81;
  header[1] = 127;
  header.writeBigUInt64BE(BigInt(payload.length), 2);
  return Buffer.concat([header, payload]);
}

function tryDecodeFrame(buffer) {
  if (buffer.length < 2) return null;
  const opcode = buffer[0] & 0x0f;
  let len = buffer[1] & 0x7f;
  let offset = 2;
  if (len === 126) {
    if (buffer.length < 4) return null;
    len = buffer.readUInt16BE(2);
    offset = 4;
  } else if (len === 127) {
    if (buffer.length < 10) return null;
    const big = buffer.readBigUInt64BE(2);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('websocket frame too large');
    len = Number(big);
    offset = 10;
  }
  const masked = (buffer[1] & 0x80) !== 0;
  const maskOffset = offset;
  if (masked) offset += 4;
  if (buffer.length < offset + len) return null;
  let payload = buffer.subarray(offset, offset + len);
  if (masked) {
    const mask = buffer.subarray(maskOffset, maskOffset + 4);
    const unmasked = Buffer.alloc(len);
    for (let i = 0; i < len; i += 1) unmasked[i] = payload[i] ^ mask[i % 4];
    payload = unmasked;
  }
  return { opcode, text: payload.toString('utf8'), rest: buffer.subarray(offset + len) };
}

const peers = new Map();
const rooms = new Map();
const CONTROL_PLANE_CAPABILITY = 'ctox-control-plane-v1';
const ROLE_BOUND_SIGNALING_CAPABILITY = 'ctox-role-bound-signaling-v1';
const CTOX_RXDB_PROTOCOL = 'ctox-rxdb-protocol-v1';
const ROLE_BOUND_AUTH_VERSION = 'ctox-role-bound-v1';
const MAX_CONTROL_PLANE_TOKEN_TTL_SECONDS = 24 * 60 * 60;
const CONTROL_PLANE_CLOCK_SKEW_SECONDS = 5 * 60;
const KNOWN_ROLES = new Set(['browser', 'ctox_instance', 'desktop_shell', 'desktop_terminal', 'ctox_desktop_app']);

function metadataFromHandshake(header) {
  const requestLine = header.split('\r\n')[0] || '';
  const target = requestLine.split(' ')[1] || '/';
  try {
    const url = new URL(target, 'ws://local');
    const client = (url.searchParams.get('client') || '').trim();
    const role = normalizeRole(url.searchParams.get('role') || url.searchParams.get('peer_role') || '', client);
    const capabilities = new Set(parseCapabilities(url));
    return {
      signalingToken: url.searchParams.get('token') || '',
      client,
      role,
      instanceId: (url.searchParams.get('instance_id') || url.searchParams.get('instance') || '').trim(),
      protocol: (url.searchParams.get('protocol') || '').trim(),
      capabilities,
      tokenIssuedAt: Number(url.searchParams.get('token_iat') || 0),
      tokenExpiresAt: Number(url.searchParams.get('token_exp') || 0),
      authVersion: (url.searchParams.get('auth_version') || '').trim(),
      browserTokenHash: (url.searchParams.get('browser_token_hash') || '').trim(),
      nativeTokenHash: (url.searchParams.get('native_token_hash') || '').trim(),
    };
  } catch {
    return {
      signalingToken: '',
      client: '',
      role: 'unknown',
      instanceId: '',
      protocol: '',
      capabilities: new Set(),
      tokenIssuedAt: 0,
      tokenExpiresAt: 0,
      authVersion: '',
      browserTokenHash: '',
      nativeTokenHash: '',
    };
  }
}

function roomKey(peer, roomId) {
  if (peer.capabilities?.has(ROLE_BOUND_SIGNALING_CAPABILITY)) {
    return `${peer.authVersion}|${peer.browserTokenHash}|${peer.nativeTokenHash}|${roomId}`;
  }
  return `${peer.signalingToken || ''}|${roomId}`;
}

function notifyRoom(roomId) {
  const room = rooms.get(roomId) || new Set();
  const peerIds = Array.from(room);
  const peerDescriptors = peerIds
    .map((peerId) => peerSummary(peers.get(peerId)))
    .filter(Boolean);
  if (debug) console.error(`[signaling] notify room=${redactRoom(roomId)} peers=${peerIds.length}`);
  for (const peerId of room) {
    peers.get(peerId)?.send({ type: 'joined', otherPeerIds: peerIds, peers: peerDescriptors });
  }
}

function redactRoom(roomId) {
  const value = String(roomId || '');
  const separator = value.lastIndexOf('|');
  return separator >= 0 ? `[binding]|${value.slice(separator + 1)}` : value;
}

function normalizeRole(value, client) {
  const role = String(value || '').trim();
  if (KNOWN_ROLES.has(role)) return role;
  const normalizedClient = String(client || '').toLowerCase();
  if (normalizedClient.includes('business') || normalizedClient.includes('browser')) return 'browser';
  if (normalizedClient.includes('ctox')) return 'ctox_instance';
  if (normalizedClient.includes('desktop')) return 'desktop_shell';
  return 'unknown';
}

function parseCapabilities(url) {
  return [
    ...url.searchParams.getAll('cap'),
    ...url.searchParams.getAll('capability'),
    ...url.searchParams.getAll('capabilities'),
  ]
    .join(',')
    .split(/[,\s]+/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function instanceIdFromBusinessOsRoom(roomId) {
  const parts = String(roomId || '').split(':');
  return parts[0] === 'ctox-business-os' && parts[1] ? parts[1] : '';
}

function controlPlaneErrorCode(reason) {
  const normalized = String(reason || '').toLowerCase();
  if (normalized.includes('protocol missing')) return 'protocol_missing';
  if (normalized.includes('protocol')) return 'protocol_mismatch';
  if (normalized.includes('expired')) return 'token_expired';
  if (normalized.includes('not yet valid')) return 'token_not_yet_valid';
  if (normalized.includes('ttl too long')) return 'token_ttl_too_long';
  if (normalized.includes('token window')) return 'token_window_invalid';
  if (normalized.includes('missing control plane token')) return 'token_missing';
  if (normalized.includes('instance mismatch')) return 'instance_mismatch';
  if (normalized.includes('missing control plane instance')) return 'instance_missing';
  if (normalized.includes('role auth missing')) return 'role_auth_missing';
  if (normalized.includes('role auth binding')) return 'role_auth_binding_invalid';
  if (normalized.includes('role credential')) return 'role_credential_invalid';
  if (normalized.includes('role')) return 'role_invalid';
  return 'control_plane_rejected';
}

function sha256Hex(value) {
  return crypto.createHash('sha256').update(String(value || '')).digest('hex');
}

function validateRoleBoundJoin(peer) {
  if (!peer.capabilities?.has(ROLE_BOUND_SIGNALING_CAPABILITY)) return '';
  if (!peer.authVersion || !peer.browserTokenHash || !peer.nativeTokenHash) {
    return 'role auth missing';
  }
  if (peer.authVersion !== ROLE_BOUND_AUTH_VERSION
      || !/^[a-f0-9]{64}$/.test(peer.browserTokenHash)
      || !/^[a-f0-9]{64}$/.test(peer.nativeTokenHash)) {
    return 'role auth binding invalid';
  }
  const expectedHash = peer.role === 'browser'
    ? peer.browserTokenHash
    : (peer.role === 'ctox_instance' ? peer.nativeTokenHash : '');
  if (!expectedHash || sha256Hex(peer.signalingToken) !== expectedHash) {
    return 'role credential invalid';
  }
  return '';
}

function validateControlPlaneJoin(peer, roomId) {
  if (!peer.capabilities?.has(CONTROL_PLANE_CAPABILITY)) return '';
  if (!peer.signalingToken) return 'missing control plane token';
  if (!peer.protocol) return 'control plane protocol missing';
  if (peer.protocol !== CTOX_RXDB_PROTOCOL) return 'control plane protocol mismatch';
  if (!Number.isFinite(peer.tokenIssuedAt) || !Number.isFinite(peer.tokenExpiresAt)) return 'invalid control plane token window';
  if (peer.tokenIssuedAt <= 0 || peer.tokenExpiresAt <= 0) return 'missing control plane token window';
  const now = Math.floor(Date.now() / 1000);
  if (peer.tokenIssuedAt > now + CONTROL_PLANE_CLOCK_SKEW_SECONDS) return 'control plane token not yet valid';
  if (peer.tokenExpiresAt < now - CONTROL_PLANE_CLOCK_SKEW_SECONDS) return 'control plane token expired';
  if (peer.tokenExpiresAt - peer.tokenIssuedAt > MAX_CONTROL_PLANE_TOKEN_TTL_SECONDS) return 'control plane token ttl too long';
  if (!KNOWN_ROLES.has(peer.role)) return 'invalid control plane role';
  const roomInstanceId = instanceIdFromBusinessOsRoom(roomId);
  if (roomInstanceId && !peer.instanceId) return 'missing control plane instance';
  if (roomInstanceId && peer.instanceId !== roomInstanceId) return 'control plane instance mismatch';
  const roleBoundError = validateRoleBoundJoin(peer);
  if (roleBoundError) return roleBoundError;
  return '';
}

function peerSummary(peer) {
  if (!peer) return null;
  return {
    peerId: peer.id,
    role: peer.role || 'unknown',
    protocol: peer.protocol || '',
    instanceId: peer.instanceId || '',
    client: peer.client || '',
    capabilities: Array.from(peer.capabilities || []),
  };
}

const server = net.createServer((socket) => {
  let handshake = false;
  let buffer = Buffer.alloc(0);
  let peer = null;

  function send(message) {
    if (!socket.destroyed) socket.write(encodeFrame(JSON.stringify(message)));
  }

  function disconnect() {
    if (!peer) return;
    for (const roomId of peer.rooms) {
      const room = rooms.get(roomId);
      room?.delete(peer.id);
      if (room && room.size === 0) rooms.delete(roomId);
      else notifyRoom(roomId);
    }
    peers.delete(peer.id);
    peer = null;
  }

  socket.on('data', (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    if (!handshake) {
      const headerEnd = buffer.indexOf('\r\n\r\n');
      if (headerEnd === -1) return;
      const header = buffer.subarray(0, headerEnd).toString('utf8');
      const key = /^Sec-WebSocket-Key: (.+)$/im.exec(header)?.[1]?.trim();
      if (!key) {
        socket.destroy();
        return;
      }
      const accept = crypto
        .createHash('sha1')
        .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
        .digest('base64');
      socket.write([
        'HTTP/1.1 101 Switching Protocols',
        'Upgrade: websocket',
        'Connection: Upgrade',
        `Sec-WebSocket-Accept: ${accept}`,
        '\r\n',
      ].join('\r\n'));
      handshake = true;
      buffer = buffer.subarray(headerEnd + 4);
      peer = { id: token(), ...metadataFromHandshake(header), rooms: new Set(), send };
      peers.set(peer.id, peer);
      if (debug) {
        console.error(`[signaling] open peer=${peer.id} role=${peer.role} instance=${peer.instanceId || '-'} protocol=${peer.protocol || '-'} token=${peer.signalingToken ? 'yes' : 'no'} caps=${Array.from(peer.capabilities || []).join(',') || '-'}`);
      }
      send({ type: 'init', yourPeerId: peer.id, peer: peerSummary(peer) });
    }

    while (true) {
      const decoded = tryDecodeFrame(buffer);
      if (!decoded) break;
      buffer = decoded.rest;
      if (decoded.opcode === 8) {
        socket.end();
        break;
      }
      if (decoded.opcode !== 1) continue;
      let message;
      try {
        message = JSON.parse(decoded.text);
      } catch {
        socket.destroy();
        break;
      }
      if (message.type === 'join') {
        if (typeof message.room !== 'string' || message.room.length <= 5 || message.room.length >= 512) {
          socket.destroy();
          break;
        }
        const controlPlaneError = validateControlPlaneJoin(peer, message.room);
        if (controlPlaneError) {
          send({
            type: 'ctoxError',
            scope: 'control-plane',
            code: controlPlaneErrorCode(controlPlaneError),
            reason: controlPlaneError,
          });
          socket.end();
          break;
        }
        const key = roomKey(peer, message.room);
        peer.rooms.add(key);
        if (!rooms.has(key)) rooms.set(key, new Set());
        rooms.get(key).add(peer.id);
        if (debug) console.error(`[signaling] join peer=${peer.id} role=${peer.role} room=${redactRoom(key)} size=${rooms.get(key).size}`);
        notifyRoom(key);
      } else if (message.type === 'signal') {
        if (message.senderPeerId !== peer.id) {
          socket.destroy();
          break;
        }
        const receiver = peers.get(message.receiverPeerId);
        if (debug) {
          const data = message.data || message.signal || {};
          const signalType = data?.type || (data?.sdp ? 'sdp' : (data?.candidate ? 'candidate' : 'unknown'));
          const candidateLine = typeof data?.candidate?.candidate === 'string'
            ? data.candidate.candidate
            : (typeof data?.candidate === 'string' ? data.candidate : '');
          const candidateType = /\styp\s+([a-z0-9-]+)/i.exec(candidateLine)?.[1] || '';
          const candidateAddress = candidateLine
            .replace(/^candidate:\S+\s+\S+\s+\S+\s+\S+\s+/i, '')
            .split(/\s+/)
            .slice(0, 2)
            .join(':')
            .slice(0, 80);
          const sdpBytes = typeof data?.sdp === 'string' ? Buffer.byteLength(data.sdp) : 0;
          console.error(`[signaling] signal from=${peer.id} to=${message.receiverPeerId} room=${redactRoom(roomKey(peer, message.room))} receiver=${receiver ? 'yes' : 'no'} sameRoom=${receiver?.rooms.has(roomKey(peer, message.room)) ? 'yes' : 'no'} type=${signalType} candidateType=${candidateType || '-'} candidate=${candidateAddress || '-'} sdpBytes=${sdpBytes}`);
        }
        if (receiver && receiver.rooms.has(roomKey(peer, message.room))) {
          if (!relay) receiver.send(message);
          else {
            const sender = peer;
            relaySignals = relaySignals.then(async () => {
              const rewritten = await relay.rewrite(message, sender, receiver);
              if (rewritten) receiver.send(rewritten);
            }).catch(error => { console.error(`S0 relay failed: ${error.message}`); process.exitCode = 1; });
          }
        }
      } else if (message.type !== 'ping') {
        socket.destroy();
        break;
      }
    }
  });
  socket.on('close', disconnect);
  socket.on('error', disconnect);
});

function runSelfTest() {
  const now = Math.floor(Date.now() / 1000);
  const browserToken = 'browser-role-token';
  const nativeToken = 'native-role-token';
  const shared = {
    client: 'ctox-role-bound-self-test',
    instanceId: 'inst_self_test',
    protocol: CTOX_RXDB_PROTOCOL,
    capabilities: new Set([CONTROL_PLANE_CAPABILITY, ROLE_BOUND_SIGNALING_CAPABILITY]),
    tokenIssuedAt: now,
    tokenExpiresAt: now + 60,
    authVersion: ROLE_BOUND_AUTH_VERSION,
    browserTokenHash: sha256Hex(browserToken),
    nativeTokenHash: sha256Hex(nativeToken),
  };
  const browser = { ...shared, role: 'browser', signalingToken: browserToken };
  const native = { ...shared, role: 'ctox_instance', signalingToken: nativeToken };
  const room = 'ctox-business-os:inst_self_test:room';
  if (validateControlPlaneJoin(browser, room) || validateControlPlaneJoin(native, room)) {
    throw new Error('valid role-bound peers were rejected');
  }
  if (roomKey(browser, room) !== roomKey(native, room)) {
    throw new Error('role-bound browser and native peers did not resolve to one room');
  }
  const wrongNative = { ...native, signalingToken: browserToken };
  const wrongNativeError = validateControlPlaneJoin(wrongNative, room);
  if (controlPlaneErrorCode(wrongNativeError) !== 'role_credential_invalid') {
    throw new Error('native peer authenticated with a browser credential');
  }
  const missingBinding = { ...browser, browserTokenHash: '' };
  const missingBindingError = validateControlPlaneJoin(missingBinding, room);
  if (controlPlaneErrorCode(missingBindingError) !== 'role_auth_missing') {
    throw new Error('missing role binding did not fail closed');
  }
  const splitBinding = { ...native, browserTokenHash: sha256Hex('different-browser-token') };
  if (validateControlPlaneJoin(splitBinding, room)) {
    throw new Error('self-consistent native binding should remain valid');
  }
  if (roomKey(browser, room) === roomKey(splitBinding, room)) {
    throw new Error('different role bindings resolved to the same room');
  }
  console.log('local_signaling_role_binding_self_test=1');
}

if (process.env.SIGNALING_SELF_TEST === '1') {
  runSelfTest();
  process.exit(0);
}

server.listen(port, host, () => {
  console.log(`CTOX RxDB signaling listening on ws://${host}:${port}`);
});
if (relay) {
  const stop = async () => {
    await relaySignals;
    await relay.close();
    process.exit(process.exitCode || 0);
  };
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
}
