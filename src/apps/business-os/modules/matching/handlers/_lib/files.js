// Origin: CTOX
// License: Apache-2.0
// files-Datensätze für Primitiv-Eingaben (Kernform §16.32, Strang CAPSIG): BlobRef = files.ref (sha256 des
// Inhalts). Rohdaten (Base64, Inline-HTML, Fragmente) legt der Handler über ctx.files.put an und übergibt
// dessen ref als {ref}; vom Primitiv erzeugte Blobs erscheinen ebenso als files-Datensatz (owner_app = matching).
import { invalid } from './errors.js';

/** Bytes bzw. Text als files-Datensatz anlegen → ref. */
export async function putFileRef(ctx, data, name, mime) {
  if (typeof ctx.files?.put !== 'function') {
    throw Object.assign(new Error('ctx.files.put is not available'), { code: 'unavailable' });
  }
  const bytes = typeof data === 'string' ? new TextEncoder().encode(data) : data;
  const result = await ctx.files.put(bytes, { name, mime });
  const ref = typeof result?.ref === 'string' ? result.ref : (typeof result === 'string' ? result : '');
  if (!ref) throw invalid(`files.put returned no ref for ${name}`);
  return ref;
}
