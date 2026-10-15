// Optimistic transaction plan; the Core commits this batch in ONE writer job.
// The opaque snapshot token is revalidated there; no SQLite lock spans JS work.
export function bufferedTransaction(snapshot, { query, error }) {
  const rows = structuredClone(snapshot.collections);
  const originals = structuredClone(snapshot.collections);
  const changes = new Map(), acknowledgements = new Map();
  const collection = name => {
    if (!Object.hasOwn(rows,name) || !Array.isArray(rows[name])) error('forbidden','Collection absent from authorized snapshot');
    return rows[name];
  };
  const bindings = {
    async read(name,mango) { return query(collection(name).filter(d => !d.deleted && !d._deleted),mango); },
    async write(name,op) {
      const docs=collection(name);
      if (!op || !['put','patch','remove'].includes(op.type)) error('invalid','Invalid transaction write');
      const id=op.id ?? op.doc?.id;
      if (typeof id !== 'string' || !id) error('invalid','Document id required');
      const index=docs.findIndex(d=>d.id===id), current=docs[index];
      if (current?.deleted) error('conflict','Document is deleted');
      if (Object.hasOwn(op,'base_rev') && op.base_rev !== (current?.rev ?? null)) error('conflict','Snapshot revision differs');
      if (!current && op.type !== 'put') error('not_found','Document unavailable');
      const original=originals[name].find(d=>d.id===id);
      const key=JSON.stringify([name,id]), previous=changes.get(key);
      let next, change;
      if (op.type === 'remove') {
        next={id,deleted:true}; change={type:'remove',id};
      } else {
        if (op.type==='patch' && (!op.patch || typeof op.patch!=='object' || Array.isArray(op.patch))) error('invalid','Patch object required');
        next=op.type==='put' ? structuredClone(op.doc) : {...current,...structuredClone(op.patch)};
        if (!next || next.id!==id) error('invalid','Transaction cannot change identity');
        if (op.type==='put' || previous?.op.type==='put') { const doc={...next};delete doc.rev;delete doc._rev;change={type:'put',doc}; }
        else change={type:'patch',id,patch:{...(previous?.op.patch ?? {}),...structuredClone(op.patch)}};
      }
      // Never mint a revision in JS. Reads retain the snapshot revision until commit.
      if (original?.rev) next.rev=original.rev;
      if (index<0) docs.push(next); else docs[index]=next;
      changes.set(key,{collection:name,op:{...change,base_rev:original?.rev ?? null}});
      const ack={status:'accepted',id};
      if (!acknowledgements.has(key)) acknowledgements.set(key,[]);
      acknowledgements.get(key).push(ack);
      return ack;
    },
  };
  return {
    bindings,
    operations:()=>[...changes.values()],
    settle(results) {
      for(const result of results ?? []) {
        const key=JSON.stringify([result.collection,result.id]);
        for(const ack of acknowledgements.get(key) ?? []) Object.assign(ack,result);
      }
    },
  };
}
