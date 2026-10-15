export {artifactUrl} from './artifact-url.mjs';
import { createKernel } from './kernel.mjs';
import { CtoxError } from './values.mjs';

/** Bootstrap only: identity comes from the authenticated bootstrap, never from
 * cookies/protected files. Shell owns /sw.js and artifact URL rewriting.
 * The loader contains no UI and never fetches business records over HTTP.
 */
export async function createLoader(options) {
  const { serviceWorker = '/sw.js', resolveShell, ...kernelOptions } = options;
  const kernel = createKernel(kernelOptions);
  await kernel.ready;
  let registration = null;
  try {
    if(serviceWorker && globalThis.navigator?.serviceWorker) {
      const url = new URL(serviceWorker, globalThis.location.href);
      if(url.origin !== globalThis.location.origin || url.pathname !== '/sw.js') throw new CtoxError('invalid','Only the same-origin /sw.js bootstrap is allowed');
      registration = await navigator.serviceWorker.register(url.href,{scope:'/'});
    }
  } catch(error) {
    kernel.statusZone.clear('shell','shell.service_worker');
    kernel.appApi('shell','0').status.report({level:'error',code:'shell.service_worker',text:String(error.message)});
  }
  let current = null, closed = false, chain = Promise.resolve();
  const start = async (ver) => {
    if(closed || current === ver)return;
    const module = resolveShell ? await resolveShell(ver,kernel.shell.artifacts) : await kernel.loadModule('shell',ver);
    const outcome = current === null ? await kernel.startShell(module) : await kernel.swapShell(module);
    if(outcome === true || outcome.swapped)current=ver;
  };
  const enqueue = (ver) => {
    chain=chain.then(()=>start(ver)).catch(error=>{
      kernel.appApi('shell',String(ver)).status.report({level:'error',code:'shell.load_failed',text:String(error.message)});
    });
    return chain;
  };
  const off = kernel.shell.activation.onChange(event=>{
    if(event.app_id==='shell' && event.state==='activated')void enqueue(event.activated_ver);
  });
  const initial=kernel.shell.activation.get('shell').get()?.activated_ver;
  if(initial)await enqueue(initial);
  return Object.freeze({kernel,registration, get shellVersion(){return current;},
    async settle(){await chain;},
    async close(){if(closed)return;closed=true;off();await chain;await kernel.close();}
  });
}
