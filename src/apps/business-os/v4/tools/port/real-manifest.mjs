// Origin: CTOX
// License: AGPL-3.0-only
// Prüfstand-Hilfe (Node): Routen- und Rechteteil des echten v4-Manifests einer App für kernel.sim.publishAppVersion.
//
// Seit dem Stub-Stand E26/E27 lehnt der Kern Befehle ohne veröffentlichte Route beim Schreiben ab
// (unknown_command, K-WJ-B2), und Fremdlesen braucht Deklaration + Export. Eine Fixture, die nur business_apps-Zeilen
// schiebt, testet deshalb gegen einen Kern ohne aktive App-Version. Diese Hilfe liefert den Teil des generierten
// Manifests (v4/manifests/<app>.manifest.json), den der Kern zum Routen und Prüfen liest; die Fixture veröffentlicht
// ihn NACH ihrer business_apps-Zeile (sonst überschreibt die Zeile active_ver):
//
//   const routes = realManifestRoutes('customers');                       // Node
//   await page.evaluate((r) => { window.__routes = r; }, routes);
//   await kernel.sim.publishAppVersion('customers', '1', { title, manifestExtra: window.__routes }); // Seite
//
// Muster aus modules/ctox/tests/layout.browser.mjs (E35) und modules/customers/tests/v4-api.test.mjs.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const MANIFESTS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../manifests');
const ROUTE_FIELDS = ['handlers', 'command_rights', 'commands_in', 'commands_out', 'foreign_collections', 'exports', 'collection_rights'];

export function realManifestRoutes(appId) {
  const file = path.join(MANIFESTS, `${appId}.manifest.json`);
  const manifest = JSON.parse(fs.readFileSync(file, 'utf8'));
  const out = {};
  for (const key of ROUTE_FIELDS) if (manifest[key] !== undefined) out[key] = manifest[key];
  return out;
}
