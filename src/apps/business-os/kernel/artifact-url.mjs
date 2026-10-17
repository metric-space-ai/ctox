import {CtoxError} from './values.mjs';
export function artifactUrl(appId, version, path, shellVersion) {
  if(typeof path!=='string' || !path || path.startsWith('/') || path.split('/').some(s=>s==='..'||s==='.'||s.includes('\\')) || /[?#\x00]/.test(path))throw new CtoxError('invalid','Invalid artifact path');
  if(typeof shellVersion!=='string'||!shellVersion)throw new CtoxError('invalid','Shell version required for immutable artifact URL');
  return `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(version)}~shell@${encodeURIComponent(shellVersion)}/${path.split('/').map(encodeURIComponent).join('/')}`;
}
export function utf8Base64(text) { return bytesBase64(new TextEncoder().encode(text)); }
export function bytesBase64(bytes) {
  let binary='';
  for(let offset=0;offset<bytes.length;offset+=32768)binary+=String.fromCharCode(...bytes.subarray(offset,offset+32768));
  return btoa(binary);
}
