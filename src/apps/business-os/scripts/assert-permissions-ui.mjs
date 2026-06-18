import { readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');

const targets = [
  join(appRoot, 'app.js'),
  join(appRoot, 'shared/react-settings.js'),
  join(appRoot, 'shared/shell-permissions-ui.js'),
  join(appRoot, 'modules/app-store/index.js'),
];

const rules = [
  {
    name: 'local-owner-admin-role-matrix',
    pattern: /\[\s*['"]chef['"]\s*,\s*['"]admin['"]\s*\]|\[\s*['"]admin['"]\s*,\s*['"]chef['"]\s*\]/,
    message: 'owner/admin role arrays belong in shared roles or permissions helpers',
  },
  {
    name: 'local-founder-permission-branch',
    pattern: /\brole\s*(?:={2,3}|!={1,2})\s*['"]founder['"]/,
    message: 'founder-specific permission branches belong in shared permissions.js',
  },
  {
    name: 'raw-founder-permission-check',
    pattern: /governance\??\.\s*founders[\s\S]{0,200}\.some\s*\(/,
    message: 'raw founder assignment checks must be wrapped by shared permissions.js',
  },
  {
    name: 'legacy-settings-copy',
    pattern: /\b(?:User Management|Founder Review|Founder zuweisen|Founder Messages)\b/,
    message: 'Settings UI must use business-facing role and access wording',
  },
  {
    name: 'legacy-role-option-label',
    pattern: />\s*(?:User|Founder|Chef)\s*<\/option>/,
    message: 'role option labels must be Owner, Admin, App-Verantwortliche:r, Teammitglied',
  },
  {
    name: 'legacy-app-modify-copy',
    pattern: /\b(?:App modifizieren|Modul bearbeiten)\b/,
    message: 'app modification affordances must use business-facing App ändern wording',
  },
];

const offenders = [];
for (const file of targets) {
  const content = readFileSync(file, 'utf8');
  for (const rule of rules) {
    if (rule.pattern.test(content)) {
      offenders.push(`${relative(repoRoot, file)}: ${rule.name} — ${rule.message}`);
    }
  }
}

if (offenders.length) {
  console.error(`Business OS permissions UI guard failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('Business OS permissions UI guard OK');
