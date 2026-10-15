import {CtoxError} from './values.mjs';
export function artifactUrl(appId, version, path, shellVersion) {
  if(typeof path!=='string' || !path || path.startsWith('/') || path.split('/').some(s=>s==='..'||s==='.'||s.includes('\\')) || /[?#\x00]/.test(path))throw new CtoxError('invalid','Invalid artifact path');
  if(typeof shellVersion!=='string'||!shellVersion)throw new CtoxError('invalid','Shell version required for immutable artifact URL');
  return `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(version)}~shell@${encodeURIComponent(shellVersion)}/${path.split('/').map(encodeURIComponent).join('/')}`;
}
export function utf8Base64(text) {
  const bytes=new TextEncoder().encode(text);let binary='';
  for(let offset=0;offset<bytes.length;offset+=32768)binary+=String.fromCharCode(...bytes.subarray(offset,offset+32768));
  return btoa(binary);
}
