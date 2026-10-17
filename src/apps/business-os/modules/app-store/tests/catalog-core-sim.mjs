// TEST-Simulation des Kerns für §16.29 (app_catalog + ctox.app.* mit Vergleich-und-Setzen auf base_app_ver).
// Der gepinnte Kern-Stub 9fa9948 kennt weder app_catalog noch die ctox.app.*-Befehle (V4-PORT.md OFFEN-9); diese
// Datei bildet die Kernantwort exakt nach dem Vertrag nach – NUR im Test, die App sieht ausschließlich api.*:
//   - app_catalog: Plattform-Collection {id, app_id, app_ver, title, layout, source: 'main'|'instance', published_at},
//     lesbar über api.query, schreibt nur der Kern (hier: setCatalog).
//   - ctox.app.publish {app_id, base_app_ver, draft_id} · ctox.app.update {app_id, base_app_ver, release_ver}
//     ctox.app.rollback {app_id, base_app_ver, target_ver} · ctox.app.release {app_id, base_app_ver, channel}
//     ctox.app.remove {app_id, base_app_ver}
//     Basis ≠ aktive Version des Kerns → write_result {status:'rejected', reason:'stale_base', detail}; der Befehl
//     erreicht business_commands nicht (Status failed mit error = reason, wie der Stub abgelehnte Schreibvorgänge
//     meldet). Sonst läuft der Befehl über den echten Stub (business_commands, accepted → running → done), und der
//     Kern ändert business_apps/business_app_versions (sim.push).
//   - update nur für locally_modified=false (außer ausdrücklich discard_local), release_ver muss als main im Katalog
//     stehen. rollback auf ein älteres Schema (manifest.schema_rev kleiner) nur mit Rück-Migration
//     (manifest.down_migration der aktiven Version), sonst reason 'migration_required'.
// Läuft im Browser (page.evaluate → import('/modules/app-store/tests/catalog-core-sim.mjs')).

function delegate(target, overrides) {
  const out = {};
  for (const key of Object.keys(target)) {
    if (key in overrides) continue;
    Object.defineProperty(out, key, { enumerable: true, get: () => { const v = target[key]; return typeof v === 'function' ? v.bind(target) : v; } });
  }
  return Object.freeze(Object.assign(out, overrides));
}

export function createCatalogCore(kernel, inner) {
  const catalog = new Map();
  const listeners = new Set();
  const ahead = new Map(); // Kern ist der Projektion voraus: app_id → aktive Version im Kern
  const actions = {};
  const rejections = [];
  const visible = (doc) => (doc && !doc._deleted ? doc : null);
  const appDoc = (id) => visible(inner.get('business_apps', id).get());
  const versionDoc = (id, ver) => visible(inner.get('business_app_versions', `${id}@${ver}`).get());
  const activeOf = (id) => (ahead.has(id) ? ahead.get(id) : (appDoc(id)?.active_ver ?? null));
  const rows = () => [...catalog.values()];
  const notify = () => { const r = rows(); for (const cb of [...listeners]) cb(r); };
  const catalogLive = { get: rows, subscribe(cb) { listeners.add(cb); cb(rows()); return () => listeners.delete(cb); } };
  const cp = () => kernel.sim.checkpoint;
  const nextVer = (id) => String(Math.max(0, ...inner.query('business_app_versions', {}).get()
    .filter((v) => v.app_id === id).map((v) => Number(v.app_ver) || 0)) + 1);

  function verdict(type, p) {
    if (!p || typeof p.app_id !== 'string' || !('base_app_ver' in p)) return { reason: 'invalid', detail: 'app_id/base_app_ver fehlt' };
    const active = activeOf(p.app_id);
    if ((p.base_app_ver ?? null) !== active) return { reason: 'stale_base', detail: `Basis ${p.base_app_ver ?? '–'} ist nicht aktiv (aktiv: ${active ?? '–'})` };
    if (type === 'ctox.app.update') {
      const release = catalog.get(`${p.app_id}@${p.release_ver}`);
      if (!release || release.source !== 'main') return { reason: 'invalid', detail: `Release ${p.release_ver} nicht im Katalog` };
      const cur = active == null ? null : versionDoc(p.app_id, active);
      if (cur?.manifest?.provenance?.locally_modified === true && p.discard_local !== true) return { reason: 'locally_modified', detail: 'App ist lokal angepasst' };
    }
    if (type === 'ctox.app.rollback') {
      const target = versionDoc(p.app_id, p.target_ver);
      const cur = versionDoc(p.app_id, active);
      if (!target) return { reason: 'invalid', detail: `Version ${p.target_ver} fehlt` };
      if (Number(target.manifest?.schema_rev || 0) < Number(cur?.manifest?.schema_rev || 0) && !cur?.manifest?.down_migration) {
        return { reason: 'migration_required', detail: `Version ${p.target_ver} hat ein älteres Schema; Rück-Migration fehlt` };
      }
    }
    return null;
  }

  function rejectedHandle(v) {
    const id = `rejected-${rejections.length}`;
    const value = Object.freeze({ status: 'failed', error: v.reason });
    return Object.freeze({
      id,
      status: { get: () => value, subscribe(cb) { cb(value); return () => {}; } },
      local: Promise.resolve(),
      result: Promise.resolve({ status: 'rejected', reason: v.reason, detail: v.detail }),
      cancel() {},
    });
  }

  async function activate(id, ver, manifest, extra = {}) {
    await kernel.sim.push('business_app_versions', [{ id: `${id}@${ver}`, app_id: id, app_ver: String(ver), manifest, migrated_at_seq: cp() }]);
    await kernel.sim.push('business_apps', [{ ...(appDoc(id) || {}), id, active_ver: String(ver), migrated_at_seq: cp(), ...extra }]);
  }
  kernel.sim.onCommand('ctox.app.update', async (p) => {
    const release = catalog.get(`${p.app_id}@${p.release_ver}`);
    await activate(p.app_id, p.release_ver, { version: String(p.release_ver), layout: release.layout || {}, provenance: { base_release: String(p.release_ver), base_hash: `h-${p.release_ver}`, locally_modified: false } }, appDoc(p.app_id) ? {} : { title: release.title });
    return { app_id: p.app_id, app_ver: String(p.release_ver) };
  });
  kernel.sim.onCommand('ctox.app.rollback', async (p) => {
    const target = versionDoc(p.app_id, p.target_ver);
    const ver = nextVer(p.app_id);
    await activate(p.app_id, ver, { ...target.manifest });
    return { app_id: p.app_id, app_ver: ver };
  });
  kernel.sim.onCommand('ctox.app.publish', async (p) => {
    const ver = nextVer(p.app_id);
    await activate(p.app_id, ver, { ...(versionDoc(p.app_id, p.base_app_ver)?.manifest || {}), provenance: { locally_modified: true } });
    return { app_id: p.app_id, app_ver: ver };
  });
  kernel.sim.onCommand('ctox.app.release', async (p) => {
    await kernel.sim.push('business_apps', [{ ...appDoc(p.app_id), release_channel: p.channel, migrated_at_seq: cp() }]);
    return { app_id: p.app_id, app_ver: activeOf(p.app_id) };
  });
  kernel.sim.onCommand('ctox.app.remove', async (p) => {
    await kernel.sim.push('business_apps', [{ id: p.app_id, _deleted: true }]);
    return { app_id: p.app_id, archived: true };
  });

  const api = delegate(inner, {
    query: (collection, q) => (collection === 'app_catalog' ? catalogLive : inner.query(collection, q)),
    command(type, payload, opts) {
      if (String(type).startsWith('ctox.app.')) {
        const v = verdict(type, payload);
        if (v) { rejections.push({ type, payload: structuredClone(payload), ...v }); return rejectedHandle(v); }
      }
      return inner.command(type, payload, opts);
    },
    // Die Shell hält Zonen-Aktionen je Code (app-host.js); der Stub-Strom trägt nur Daten.
    status: Object.freeze({
      report(item) {
        const { action, ...plain } = item;
        if (action && typeof action.run === 'function') actions[item.code] = action; else delete actions[item.code];
        inner.status.report(plain);
      },
      clear(code) { delete actions[code]; inner.status.clear(code); },
    }),
  });

  return {
    api,
    actions,
    rejections,
    setCatalog(list) { catalog.clear(); for (const row of list) catalog.set(row.id || `${row.app_id}@${row.app_ver}`, { id: `${row.app_id}@${row.app_ver}`, ...row }); notify(); },
    setAhead(id, ver) { if (ver == null) ahead.delete(id); else ahead.set(id, String(ver)); },
  };
}
