// Origin: CTOX
// License: AGPL-3.0-only
// Shell v4 – Start (E38). Ruft nur bootV4 des Kerns. bootV4 baut aus dem authentifizierten Startkontext
// (/api/business-os/launch-context: instanceId, userId, actor, Signaling, Sitzungs-Token, verifyPackage-Schlüssel;
// keine Geschäftsdaten) den WebRTC-Transport und ruft createLoader.
// Shell-Modul: ohne resolveShell lädt der Kern manifest.entry (= v4/frame/index.js) der aktivierten Shell-Version
// selbst – mit Service Worker über die Artefakt-Route der Bindung (§16.39), sonst als Blob (kernel.loadModule).

export const LAUNCH_CONTEXT_URL = '/api/business-os/launch-context';
export const KERNEL_BOOT_URL = '/kernel/boot.mjs';

export async function boot({ importModule = (url) => import(/* @vite-ignore */ url), launchContextUrl = LAUNCH_CONTEXT_URL } = {}) {
  const { bootV4 } = await importModule(KERNEL_BOOT_URL);
  return bootV4({ launchContextUrl });
}

if (typeof document !== 'undefined' && !globalThis.__CTOX_V4_BOOT_MANUAL__) {
  // Kein Zustandstext im Fenster: Startfehler meldet der Kern in die Zustandszone; hier nur die Konsole.
  boot().catch((error) => console.error('[ctox v4 boot]', error));
}
