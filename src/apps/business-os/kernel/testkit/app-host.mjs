import {utf8Base64} from '../artifact-url.mjs';
import {snapshotBytes} from '../snapshot.mjs';
// ============================================================================================================
// TESTKIT – NICHT TEIL DES KERNELS.
// Nachbildung dessen, was die Shell v4 tut (§16.19 Nr. 1: Apps montieren macht ausschließlich die Shell):
// App-Code laden, mount(root, api, ctx), bei Aktivierung remounten (snapshot → kernel.windowState → restored),
// mount-Timeout 9 s, Rückfall auf die vorherige Version ≤ 10 s über kernel.activation.reportFailed (§16.19 Nr. 2, 4).
// Für Tests, Demo und als Referenz für die Shell. Die Shell ersetzt dieses Modul durch ihre eigene Logik
// (Service Worker /~a/… statt Blob-/data:-URL, §16.15).
// ============================================================================================================

const SNAPSHOT_MAX_BYTES = 1024 * 1024;

export function createAppHost(kernel, { mountTimeoutMs = 9000 } = {}) {
  const modules = new Map(); // `${appId}@${ver}` → In-Memory-Modul
  const cache = new Map();
  const mounts = new Set();
  const chains = new Map();
  let disposed = false;
  const cancels=new Set();

  function report(appId, item) {
    try { kernel.status.report(appId, item); } catch (e) { console.error('[testkit] status', e); }
  }

  async function loadModule(appId, ver) {
    const key = `${appId}@${ver}`;
    if (modules.has(key)) return modules.get(key);
    if (cache.has(key)) return cache.get(key);
    const manifest = await kernel.artifacts.manifest(appId, ver);
    if (!manifest) throw new Error(`Manifest ${key} fehlt`);
    const hash = manifest.files?.[manifest.entry];
    if (!hash) throw new Error(`Einstiegsdatei ${manifest.entry} fehlt in ${key}`);
    if(kernel.loadModule){const mod=await kernel.loadModule(appId,ver);cache.set(key,mod);return mod;}
    const text = await (await kernel.artifacts.read(hash)).text();
    let url, revoke = false;
    if (typeof window !== 'undefined' && typeof URL.createObjectURL === 'function') {
      url = URL.createObjectURL(new Blob([text], { type: 'text/javascript' }));
      revoke = true;
    } else {
      url = `data:text/javascript;base64,${utf8Base64(text)}`;
    }
    try {
      const mod = await import(/* @vite-ignore */ url);
      cache.set(key, mod);
      return mod;
    } finally { if (revoke) URL.revokeObjectURL(url); }
  }
  function mountFn(mod) {
    const fn = mod?.mount ?? mod?.default?.mount ?? (typeof mod?.default === 'function' ? mod.default : null);
    if (typeof fn !== 'function') throw new Error('App-Modul exportiert kein mount()');
    return fn;
  }
  async function mountVersion(root, appId, ver, ctx) {
    const mod = await loadModule(appId, ver);
    const api = kernel.appApi(appId, ver);
    const handle = await mountFn(mod)(root, api, ctx);
    if (!handle || typeof handle.unmount !== 'function') throw new Error('mount() muss {unmount} liefern');
    return { handle, api };
  }
  function withTimeout(p,ms,onLate){return new Promise((resolve,reject)=>{
    let ended=false;const clear=()=>{clearTimeout(timer);cancels.delete(cancel);};
    const cancel=()=>{if(!ended){ended=true;clear();reject(new Error('Host mount deadline or disposal'));}};
    const timer=setTimeout(cancel,ms);cancels.add(cancel);
    Promise.resolve(p).then(value=>{if(ended){try{Promise.resolve(onLate?.(value)).catch(()=>{});}catch{}return;}ended=true;clear();resolve(value);},error=>{if(!ended){ended=true;clear();reject(error);}});
  });}
  function checkSnapshot(appId, snap) {
    if (snap === undefined) return undefined;
    try {
      const c = structuredClone(snap);
      const size=snapshotBytes(c);
      if (size > SNAPSHOT_MAX_BYTES) throw new Error('Snapshot > 1 MiB');
      return c;
    } catch (e) {
      report(appId, { level: 'warn', code: 'app.snapshot_invalid', text: `Snapshot verworfen: ${e?.message ?? e}` });
      return undefined;
    }
  }

  async function remountOne(m, newVer) {
    const deadline=Date.now()+10000;
    const oldVer = m.ver;
    let snap;
    try { snap = m.handle.snapshot ? await withTimeout(Promise.resolve().then(()=>m.handle.snapshot()),Math.min(250,mountTimeoutMs)) : undefined; } catch (e) {
      report(m.appId, { level: 'warn', code: 'app.snapshot_failed', text: `snapshot() warf: ${e?.message ?? e}` });
    }
    if(disposed)return;
    snap = checkSnapshot(m.appId, snap);
    // §16.10: Snapshot vor dem Umschalten in window_state
    await kernel.windowState.put(`mount:${m.id}`, { app_id: m.appId, ver: oldVer, next_ver: newVer, snapshot: snap ?? null });
    try { await withTimeout(Promise.resolve().then(()=>m.handle.unmount()),Math.min(250,mountTimeoutMs)); } catch (e) { console.error('[testkit] unmount', e); }
    if(disposed)return;
    try {
      const r = await withTimeout(mountVersion(m.root, m.appId, newVer, { restored: structuredClone(snap) }), Math.max(1,Math.min(mountTimeoutMs,deadline-Date.now()-1000)),
        (lateMount) => { try { lateMount.handle.unmount(); } catch { /* egal */ } });
      Object.assign(m, { ver: newVer, handle: r.handle, api: r.api });
      kernel.status.clear(m.appId, 'app.remount_failed');
    } catch (err) {
      if(disposed)return;
      // Rückfall: alte Version mit demselben Snapshot, dann Kernel informieren (Zeiger zurück, app_activation failed)
      const r = await withTimeout(mountVersion(m.root,m.appId,oldVer,{restored:structuredClone(snap)}),Math.max(1,deadline-Date.now()));
      Object.assign(m, { ver: oldVer, handle: r.handle, api: r.api });
      report(m.appId, { level: 'error', code: 'app.remount_failed', text: `Version ${newVer} startet nicht (${err?.message ?? err}); Version ${oldVer} läuft weiter`, ref: newVer });
      await kernel.activation.reportFailed(m.appId, newVer, `mount_failed: ${err?.message ?? err}`);
    }
  }
  function schedule(appId, ver) {
    const prev = chains.get(appId) ?? Promise.resolve();
    const next = prev.then(async () => {
      for (const m of [...mounts]) if (m.appId === appId && m.ver !== ver && mounts.has(m)) await remountOne(m, ver);
    }).catch((e) => console.error('[testkit] remount', e));
    chains.set(appId, next);
    return next;
  }
  const offActivation = kernel.activation.onChange((ev) => {
    if (!disposed && ev.activated_ver) schedule(ev.app_id, ev.activated_ver);
  });

  return Object.freeze({
    registerModule(appId, ver, module) { modules.set(`${appId}@${ver}`, module); },
    async mount(root, appId) {
      await kernel.ready;
      const ver = kernel.activation.get(appId).get()?.activated_ver;
      if (!ver) throw new Error(`App ${appId} ist nicht aktiviert`);
      const r = await withTimeout(mountVersion(root,appId,ver,{restored:undefined}),mountTimeoutMs,late=>late.handle.unmount());
      const m = { id: Math.random().toString(36).slice(2), appId, ver, root, handle: r.handle, api: r.api };
      mounts.add(m);
      return Object.freeze({
        app_id: appId,
        get ver() { return m.ver; },
        get api() { return m.api; },
        async unmount() { mounts.delete(m); await m.handle.unmount(); },
      });
    },
    /** Wartet, bis laufende Remounts fertig sind. */
    async idle() {
      for (;;) {
        const seen = [...chains.values()];
        await Promise.all(seen);
        const now = [...chains.values()];
        if (now.length === seen.length && now.every((p, i) => p === seen[i])) return;
      }
    },
    async dispose() {
      disposed = true;
      for(const cancel of [...cancels])cancel();
      offActivation();
      for (const m of [...mounts]) { try { await withTimeout(Promise.resolve().then(()=>m.handle.unmount()),Math.min(250,mountTimeoutMs)); } catch { /* egal */ } }
      mounts.clear();
    },
  });
}
