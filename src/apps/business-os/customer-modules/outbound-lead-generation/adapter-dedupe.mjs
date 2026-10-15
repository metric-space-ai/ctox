// Altlast aus der Vorgaenger-App: Adapter mit anderem ID-Schema neben dem
// aktuellen `adapter_leadgen_<quelle>`. Die App nahm den ersten, den sie fand,
// und zeigte teils den Stand des alten Eintrags (15.10.2026: 12 Quellen doppelt).
// Hat eine Quelle einen aktuellen Adapter, sind die anderen zu entfernen.
export function doppelteAdapterIds(adapters = []) {
  const aktuell = new Set(adapters
    .filter((doc) => doc && !doc._deleted && String(doc.id || '').startsWith('adapter_leadgen_'))
    .map((doc) => doc.source_id));
  return adapters
    .filter((doc) => doc && !doc._deleted && doc.source_id && aktuell.has(doc.source_id)
      && !String(doc.id || '').startsWith('adapter_leadgen_'))
    .map((doc) => doc.id);
}
