// V1.5 chunk envelope decoder. Handles both the plain form (documents array
// inline) and the compressed form (deflate + base64). Browsers use native
// `DecompressionStream("deflate")`; Node falls back to `zlib.inflateRawSync`
// for test parity (deflate-raw, no zlib wrapper — matches flate2 default).

export async function decodeChunk(chunk) {
  if (!chunk || typeof chunk !== 'object') {
    throw new TypeError('chunk must be an object');
  }
  if (!chunk.compressed) {
    return chunk.documents || [];
  }
  if (chunk.compressed !== 'deflate') {
    throw new Error(`unsupported chunk compression: ${chunk.compressed}`);
  }
  if (typeof chunk.compressedBase64 !== 'string') {
    throw new Error('compressed chunk missing compressedBase64');
  }
  const bytes = base64ToBytes(chunk.compressedBase64);
  const json = await deflateInflate(bytes);
  return JSON.parse(json);
}

function base64ToBytes(b64) {
  if (typeof Buffer !== 'undefined' && typeof Buffer.from === 'function') {
    const buf = Buffer.from(b64, 'base64');
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  }
  // Browser fallback via atob.
  const bin = globalThis.atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

async function deflateInflate(bytes) {
  // flate2's DeflateEncoder produces RAW deflate (no zlib header).
  // Browsers expose this as "deflate-raw"; older Node versions don't have
  // DecompressionStream at all and need node:zlib's inflateRawSync.
  if (typeof globalThis.DecompressionStream === 'function') {
    try {
      const stream = new Blob([bytes]).stream().pipeThrough(new globalThis.DecompressionStream('deflate-raw'));
      const buf = await new Response(stream).arrayBuffer();
      return new TextDecoder().decode(buf);
    } catch (err) {
      // Fall through to zlib path if "deflate-raw" isn't supported.
    }
  }
  const zlib = await import('node:zlib');
  const inflated = zlib.inflateRawSync(Buffer.from(bytes));
  return inflated.toString('utf8');
}
