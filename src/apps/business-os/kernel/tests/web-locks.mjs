// Deterministic Web Locks implementation for Node conformance tests. Browser
// acceptance uses navigator.locks, never this test-only lock manager.
const queues = new Map();
const held = new Set();
export const testLocks = {
  request(name, options, fn) {
    return new Promise((resolve,reject) => {
      if(options.ifAvailable && held.has(name)){ Promise.resolve(fn(null)).then(resolve,reject);return; }
      const item={ fn,resolve,reject,signal:options.signal };
      const abort=()=>{const q=queues.get(name)??[];const i=q.indexOf(item);if(i>=0){q.splice(i,1);reject(new DOMException('Aborted','AbortError'));}};
      options.signal?.addEventListener('abort',abort,{once:true});
      item.abort=abort;
      const q=queues.get(name)??[];q.push(item);queues.set(name,q);drain(name);
    });
  }
};
function drain(name){
  if(held.has(name))return;
  const item=queues.get(name)?.shift();if(!item)return;
  item.signal?.removeEventListener('abort',item.abort);
  if(item.signal?.aborted){item.reject(new DOMException('Aborted','AbortError'));drain(name);return;}
  held.add(name);
  Promise.resolve().then(()=>item.fn({name})).then(item.resolve,item.reject).finally(()=>{held.delete(name);drain(name);});
}
