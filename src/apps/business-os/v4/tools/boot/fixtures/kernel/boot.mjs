// Labor-Ersatz für kernel/boot.mjs (E38), bis der Kernel-Strang bootV4 liefert. Gleiche Signatur:
// bootV4({launchContextUrl, resolveShell?}) → loader {kernel}. Zeichnet den Aufruf auf, baut einen Stub-Kern
// (statt WebRTC aus dem Startkontext), veröffentlicht eine Shell-Version und startet den echten Rahmen.
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';

export async function bootV4(options) {
  (globalThis.__bootCalls ??= []).push({ keys: Object.keys(options ?? {}).sort(), launchContextUrl: options?.launchContextUrl });
  const kernel = createStubKernel({ instanceId: 'bootlab', userId: 'owner', storage: 'memory', latencyMs: 5, multiTab: false });
  await kernel.ready;
  // E48: die Shell mit dem Routen-/Rechteteil ihres echten Manifests (boot.test.mjs liefert ihn). Eigentümer-Apps (ctox,
  // desktop) veröffentlicht das Labor bewusst nicht – die Startseite zeigt nur den Rahmen; ctox-platform (shell_preferences)
  // veröffentlicht der Stub selbst.
  const routes = await (await fetch('/kernel/shell-routes.json')).json();
  // Der Rahmen kommt hier vom Rohpfad /business-os/v4/frame/index.js und spricht die App-API daher als shell@dev an
  // (frame/index.js shellVer); die Deklaration muss an dieser Version stehen. Aktiv bleibt shell@lab.
  await kernel.sim.publishAppVersion('shell', 'dev', { title: 'Shell', styles: [], manifestExtra: routes });
  await kernel.sim.publishAppVersion('shell', 'lab', { title: 'Shell', styles: [], manifestExtra: routes });
  await kernel.sim.publishAppVersion('notes', '1.0.0', { title: 'Notizen', layout: { category: 'Arbeit' }, styles: [] });
  const frame = await import('/business-os/v4/frame/index.js');
  await kernel.sim.startShell({ startShell: (k, o = {}) => frame.startShell(k, o), stopShell: () => frame.stopShell() });
  return { kernel };
}
