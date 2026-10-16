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
// E48: das Kern-Manifest der Plattform-App (business_commands, ctox_queue_tasks, files, … mit Exporten).
const PLATFORM_MANIFEST = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../../core/business_os/v4/ctox-platform.manifest.json');
// FIXT-R (E48): auch die eigenen collections – ohne sie sperrt der Stub seit 696d9b9 das Lesen eigener Collections, und die
// Fixture-Daten werden gegen die echten Schemas geprüft.
const ROUTE_FIELDS = ['handlers', 'command_rights', 'commands_in', 'commands_out', 'foreign_collections', 'exports', 'collection_rights', 'collections'];

export function realManifestRoutes(appId) {
  const manifest = readManifest(appId);
  const out = {};
  for (const key of ROUTE_FIELDS) if (manifest[key] !== undefined) out[key] = manifest[key];
  // FIXT-R: Fremdlesen braucht ein aktives Eigentümer-Manifest mit collections + exports (Stub seit 696d9b9). Die Fixture
  // veröffentlicht diese Eigentümer vor der eigenen Version (Zeile „for (const o of …_fixture_owners …)“); das Feld ist für
  // den Kern ohne Bedeutung (wie _format).
  out._fixture_owners = fixtureOwners(appId, manifest);
  return out;
}

function readManifest(appId) {
  return JSON.parse(fs.readFileSync(path.join(MANIFESTS, `${appId}.manifest.json`), 'utf8'));
}

function fixtureOwners(appId, manifest) {
  const owners = new Set();
  for (const decl of Object.values(manifest.foreign_collections ?? {})) {
    const owner = decl?.owner_app ?? decl?.app ?? decl?.owner;
    if (owner && owner !== appId) owners.add(owner);
  }
  const out = [];
  for (const owner of [...owners].sort()) {
    if (owner === 'ctox-platform') { const p = realPlatformManifest(); out.push({ app: owner, ver: p.app_ver, publish: p.publish }); continue; }
    if (!fs.existsSync(path.join(MANIFESTS, `${owner}.manifest.json`))) continue;
    const m = readManifest(owner);
    out.push({ app: owner, ver: String(m.app_ver ?? '1'), publish: { title: m.title ?? owner, collections: m.collections ?? {}, manifestExtra: { exports: m.exports ?? {} } } });
  }
  return out;
}

// E48: Fremdlesen von Plattform-Collections (business_commands, ctox_queue_tasks, …) braucht ein aktives ctox-platform-Manifest
// mit collections + exports. Solange der Stub es nicht selbst veröffentlicht, veröffentlicht die Fixture es aus dem echten
// Kern-Manifest – nur, wenn noch keine aktive ctox-platform-Version existiert:
//
//   const platform = realPlatformManifest();                                // Node
//   // Seite: if (!(await kernel.sim.inspect('business_apps','ctox-platform'))?.data?.active_ver)
//   //   await kernel.sim.publishAppVersion('ctox-platform', platform.app_ver, platform.publish);
export function realPlatformManifest() {
  const manifest = JSON.parse(fs.readFileSync(PLATFORM_MANIFEST, 'utf8'));
  return {
    app_ver: String(manifest.app_ver ?? '1'),
    publish: {
      title: manifest.title ?? 'CTOX Plattform',
      collections: manifest.collections ?? {},
      manifestExtra: { exports: manifest.exports ?? {}, command_rights: manifest.command_rights ?? {} },
    },
  };
}
