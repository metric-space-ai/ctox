import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));
const modules = ['brightdata-core.cjs', 'brightdata-state.cjs', 'brightdata-continuation.cjs', 'run-brightdata.cjs'];

// Native register-script stores one immutable revision, not sibling helper
// files. Package precisely these reviewed CommonJS modules into that one file;
// no package manager, fetched code, eval, or runtime source path is involved.
export function bundleBrightData() {
  const factories = modules.map(name => `${JSON.stringify('./' + name)}: function(module, exports, require) {\n${fs.readFileSync(path.join(directory, name), 'utf8')}\n}`).join(',\n');
  return `"use strict";\n(() => {\nconst nativeRequire = require;\nconst factories = {\n${factories}\n};\nconst cache = new Map();\nfunction load(name) {\n  if (name.startsWith('node:')) return nativeRequire(name);\n  if (!Object.prototype.hasOwnProperty.call(factories, name)) throw new Error('unregistered adapter module');\n  if (cache.has(name)) return cache.get(name).exports;\n  const module = { exports: {} }; cache.set(name, module);\n  factories[name](module, module.exports, load); return module.exports;\n}\nload('./run-brightdata.cjs').main().catch(() => { process.stdout.write(JSON.stringify({schema:'prospect.v1',provider:'linkedin.com',records:[],failure_mode:'blocked',error_code:'runner_failed'})+'\\n'); });\n})();\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 4 || process.argv[2] !== '--output' || !path.isAbsolute(process.argv[3])) {
    throw new Error('Usage: node bundle-brightdata.mjs --output <absolute disposable artifact path>');
  }
  fs.writeFileSync(process.argv[3], bundleBrightData(), { mode: 0o600, flag: 'wx' });
}
