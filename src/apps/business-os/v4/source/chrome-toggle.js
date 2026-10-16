// Shell v4 – Umschalter „Quelltext“ im Fenster-Chrome (Entwurf §8 erster Punkt).
// Angebot an den Rahmen-Strang; die eigentliche Integration in den Rahmen (v4/frame) macht dieser. Bewusst ohne
// Annahmen über die Chrome-Klassen, nur über drei Elemente des Fensters:
//
//   attachSourceToggle({ controls, content, zone, appRecord, kernel, api, editor? })
//     → { button, open(), close(), isOpen(), handle, destroy() }
//     controls  Element der Fenstersteuerung; der Umschalter wird vorn eingefügt
//     content   Inhaltsbereich der App (bleibt montiert, Live-Remount läuft weiter; wird nur ausgeblendet)
//     zone      Zustandszone des Fensters ({ report, clear })
//     api       STABILER actor-Spiegel (actor.permissions: LiveDoc<string[]>) – nicht die fensterbezogene App-API,
//               deren Abonnements beim Remount entsorgt werden.
// Sichtbar nur mit apps.source.view (Spiegel; geprüft wird im Core). Die Ansicht liegt IM Fenster direkt hinter
// `content`; nichts hängt außerhalb des Fensters.

import { openSourceMode, canViewSource } from './source-mode.js';

export function attachSourceToggle({ controls, content, zone, appRecord, kernel, api, editor }) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'v4-window__source';
  button.dataset.v4SourceToggle = '';
  button.setAttribute('aria-label', 'Quelltext');
  button.setAttribute('aria-pressed', 'false');
  button.title = 'Quelltext';
  button.textContent = '</>';
  button.hidden = true;
  controls.prepend(button);

  let handle = null;
  let host = null;
  let opening = null;
  async function open() {
    if (handle || opening) return opening;
    host = document.createElement('div');
    host.className = 'v4-src-host';
    content.after(host);
    content.hidden = true;
    button.setAttribute('aria-pressed', 'true');
    opening = openSourceMode({ root: host, appRecord, kernel, api, zone, editor })
      .then((hd) => { handle = hd; return hd; })
      .catch((e) => { zone.report({ level: 'error', code: 'source.open', text: `Quelltext nicht ladbar: ${e?.message ?? e}` }); close(); })
      .finally(() => { opening = null; });
    return opening;
  }
  function close() {
    handle?.close();
    handle = null;
    host?.remove();
    host = null;
    content.hidden = false;
    button.setAttribute('aria-pressed', 'false');
  }
  button.addEventListener('click', () => (handle ? close() : open()));
  const off = api.actor.permissions.subscribe((p) => {
    button.hidden = !canViewSource(p);
    if (button.hidden && (handle || host)) close();
  });
  return { button, open, close, isOpen: () => !!handle, get handle() { return handle; }, destroy() { off(); close(); button.remove(); } };
}
