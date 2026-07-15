import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');

const build = app.match(/const APP_BUILD = '([^']+)'/)?.[1];
assert.ok(build, 'shell build id is declared');
assert.match(html, new RegExp(`app\\.js\\?v=${build}`));

assert.match(app, /function ensureModuleStylesheet\(moduleLike\)/);
assert.match(app, /const revision = moduleRevisionQuery\(moduleLike\)/);
assert.match(app, /index\.css\?v=\$\{APP_BUILD\}\$\{revision\}/);
assert.match(app, /existing\.forEach\(\(link\) => link\.remove\(\)\)/);
assert.doesNotMatch(app, /ensureModuleStylesheet\(base\)/);
