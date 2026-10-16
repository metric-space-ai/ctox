// Origin: CTOX
// License: Apache-2.0
// ctox.office.settings.save — ref: store.rs:15857-15880 (FAEHIGKEIT).
//
// Shared by documents + spreadsheets. The office-engine runtime settings are a
// core capability; the handler only forwards the validated settings payload.
// Policy: workspace RuntimeManage (enforced by the core before dispatch).
export default async function handle(command, ctx) {
  const outcome = await ctx.cap.office.settings.save(command.payload ?? {});
  return { ok: true, office: outcome };
}
