// Host für erzeugte TypeScript-Scrape-Skripte (Strang OLG725, Kern §16.48 Nr. 3).
// Allowlist-Eintrag `olg-scrape-ts` = tsx + runtime/host.ts (untrusted_code:true → nur Sandbox E20).
// Gleiche Prüfung und gleicher Ablauf wie runtime/host.mjs; das Skript wird als .ts abgelegt und über den
// tsx-Lader importiert. Keine eigene Logik hier, damit beide Hosts nicht auseinanderlaufen.
import { runHost } from './host.mjs';

runHost({ extension: '.ts' }).catch((error: unknown) => {
  process.stderr.write(`olg-scrape-host: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
