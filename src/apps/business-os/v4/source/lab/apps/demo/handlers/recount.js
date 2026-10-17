// Handler demo.recount (App-Laufzeit §9.4): zählt Einträge neu.
export default async function recount(ctx, payload) {
  const rows = await ctx.read('items', { selector: {} });
  return { count: rows.length, payload };
}
