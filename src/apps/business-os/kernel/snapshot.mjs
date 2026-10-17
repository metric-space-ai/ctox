// Conservative size accounting for structured-clone-compatible snapshots,
// including cycles, maps, binary data and Blob contents (not JSON length).
export function snapshotBytes(value) {
  const seen=new Set(),encoder=new TextEncoder();
  const size=v=>{
    if(v===null||v===undefined)return 4;
    if(typeof v==='string')return encoder.encode(v).byteLength+8;
    if(typeof v!=='object')return 16;
    if(seen.has(v))return 8;seen.add(v);
    if(v instanceof Blob)return v.size+64;
    if(v instanceof ArrayBuffer||ArrayBuffer.isView(v))return v.byteLength+64;
    if(v instanceof Map)return 64+[...v].reduce((n,[k,x])=>n+size(k)+size(x),0);
    if(v instanceof Set)return 64+[...v].reduce((n,x)=>n+size(x),0);
    return 64+Object.entries(v).reduce((n,[k,x])=>n+size(k)+size(x),0);
  };
  return size(value);
}
