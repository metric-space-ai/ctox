// The lead list loads one window of a campaign. Command reconciliation used to
// look only at that window, so research leads outside it kept "Läuft" after
// their command had ended: 35 of 494 leads on thesen (08.10.2026), their
// commands long terminal. This sweep finds every queued or running lead in the
// collection, page by page, because one query returns at most 200 rows.
export const IN_FLIGHT_SWEEP_INTERVAL_MS = 60_000;
export const IN_FLIGHT_SWEEP_PAGE = 200;
const MAX_PAGES = 25;

// `find(query)` resolves to plain lead rows. Leads in `known` are skipped.
export async function inFlightLeadsOutsideWindow(find, known = new Set()) {
  const found = [];
  let afterId = '';
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const selector = { research_status: { $in: ['queued', 'running'] } };
    if (afterId) selector.id = { $gt: afterId };
    const rows = (await find({ selector, sort: [{ id: 'asc' }], limit: IN_FLIGHT_SWEEP_PAGE })) || [];
    for (const row of rows) {
      const id = String(row?.id || '');
      if (id && !known.has(id)) found.push(row);
    }
    if (!rows.length) break;
    afterId = String(rows[rows.length - 1]?.id || '');
    if (!afterId) break;
  }
  return found;
}
