// Labor-Ersatz für kernel/boot.mjs (E38), bis der Kernel-Strang bootV4 liefert. Gleiche Signatur:
// bootV4({launchContextUrl, resolveShell?}) → loader {kernel}. Zeichnet den Aufruf auf, baut einen Stub-Kern
// (statt WebRTC aus dem Startkontext), veröffentlicht eine Shell-Version und startet den echten Rahmen.
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';

export async function bootV4(options) {
  (globalThis.__bootCalls ??= []).push({ keys: Object.keys(options ?? {}).sort(), launchContextUrl: options?.launchContextUrl });
  const kernel = createStubKernel({ instanceId: 'bootlab', userId: 'owner', storage: 'memory', latencyMs: 5, multiTab: false });
  await kernel.ready;
  await kernel.sim.publishAppVersion('shell', 'lab', { title: 'Shell', styles: [] });
  await kernel.sim.publishAppVersion('notes', '1.0.0', { title: 'Notizen', layout: { category: 'Arbeit' }, styles: [] });
  const frame = await import('/business-os/v4/frame/index.js');
  await kernel.sim.startShell({ startShell: (k, o = {}) => frame.startShell(k, o), stopShell: () => frame.stopShell() });
  return { kernel };
}
