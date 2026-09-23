export async function collectUniquePages(loadPage, {
  pageSize = 100,
  idOf = (item) => item?.id,
  onPage,
  shouldContinue = () => true,
} = {}) {
  const records = [];
  const seen = new Set();
  for (let skip = 0; ; skip += pageSize) {
    if (!shouldContinue()) return { records, complete: false };
    const page = await loadPage({ skip, limit: pageSize });
    if (!Array.isArray(page) || page.length > pageSize) {
      throw new Error('Paginierte Abfrage lieferte ein ungültiges Fenster.');
    }
    for (const record of page) {
      const id = String(idOf(record) || '').trim();
      if (!id || seen.has(id)) throw new Error('Paginierte Abfrage lieferte einen fehlenden oder doppelten Datensatz.');
      seen.add(id);
      records.push(record);
    }
    if (onPage) await onPage(page);
    if (page.length < pageSize) return { records, complete: true };
  }
}
