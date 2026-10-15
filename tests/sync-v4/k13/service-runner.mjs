// Runs as #810's unprivileged namespace child, with its real Native lifecycle.
import fs from 'node:fs';
import {Native} from '../../sync-v4-e2e/native.mjs';
import {installServiceEntry} from './service-entry.mjs';
const config=JSON.parse(fs.readFileSync(process.argv[2]));
if(!config.canonicalTmpdir || !fs.statSync(config.canonicalTmpdir).isDirectory())throw Error('Canonical lane TMPDIR required');
process.env.TMPDIR=config.canonicalTmpdir;
installServiceEntry(Native);
await import('../../sync-v4-e2e/runner.mjs');
