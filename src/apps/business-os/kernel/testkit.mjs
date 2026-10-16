// Explicit development/test entry. Never imported by the production slot.
export * from './index.mjs';
import {createKernel} from './kernel.mjs';
import {MemoryCoreTransport} from './fake-core.mjs';
export {MemoryCoreTransport};

export function createStubKernel(opts = {}) {
  const namespace = opts.dbName ?? `ctox-${opts.instanceId ?? 'stub'}-${opts.userId ?? opts.actor?.id ?? 'local'}`;
  const actor = {id:opts.userId??opts.actor?.id??'local',role:'admin',permissions:['apps.source.view','apps.source.edit'],...opts.actor};
  // K-1c Punkt 4: coreNamespace = ein Core je Instanz für mehrere Nutzer (eigene DB je Nutzer, §16.7); der Core kennt
  // dann den Nutzer (Rolle, Rechte) und setzt Zeilen-, Collection- und Befehlsrechte durch.
  const shared = typeof opts.coreNamespace === 'string' && opts.coreNamespace;
  const transport = new MemoryCoreTransport({namespace:shared?opts.coreNamespace:namespace,latencyMs:opts.latencyMs,user:shared?{...actor,...(opts.coreUser??{}),id:actor.id}:null,everyMs:opts.everyMs,agentDelayMs:opts.agentDelayMs,deviceLabel:opts.deviceLabel,codingDelayMs:opts.codingDelayMs});
  return createKernel({instanceId:'stub',userId:'local',...opts,actor,transport});
}
