import { ok } from './sub/inner.js';
import { shared } from 'ctox:shared/util.js';
import { lib } from 'ctox:lib/approved@1/lib.mjs';
import { nope } from 'ctox:lib/unknown@2/lib.mjs';
import { other } from '../beta/schema.js';
import {
  multi,
} from '../../vendor/lib/lib.mjs';
import 'lexical';
import { readFileSync } from 'node:fs';
export * from '../../shared/util.js?v=123';
// import { commented } from '../beta/commented.js';
/* new URL('../../vendor/block-comment.css', import.meta.url) */
const later = () => import('../../vendor/lib/dyn.mjs');
const css = new URL('../../vendor/lib/lib.css', import.meta.url).href;
const own = new URL('./locales/de.json', import.meta.url);
const icon = (id) => new URL(`../${id}/icon.svg`, import.meta.url).pathname;
const inside = (id) => new URL(`./icons/${id}.svg`, import.meta.url).pathname;
const abs = '/business-os/vendor/lib/lib.mjs';
const api = fetch('/api/business/whatever');
const words = 'a ../ b';
const allowedOne = '../../vendor/lib/legacy.mjs'; // isolation-allow: Fixture-Ausnahme
