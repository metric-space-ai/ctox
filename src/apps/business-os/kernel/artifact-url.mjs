import {CtoxError} from './values.mjs';
export function artifactUrl(appId, version, path) {
  if(typeof path!=='string' || !path || path.startsWith('/') || path.split('/').some(s=>s==='..'||s==='.'||s.includes('\\')) || /[?#\x00]/.test(path))throw new CtoxError('invalid','Invalid artifact path');
  return `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(version)}/${path.split('/').map(encodeURIComponent).join('/')}`;
}
export function utf8Base64(text) {
  const bytes=new TextEncoder().encode(text);let binary='';
  for(let offset=0;offset<bytes.length;offset+=32768)binary+=String.fromCharCode(...bytes.subarray(offset,offset+32768));
  return btoa(binary);
}
