'use strict';

const fs = require('node:fs');
const path = require('node:path');

// Existing UI cases require these optional catalog apps, not production defaults.
const CATALOG_MODULES = Object.freeze([
  'matching', 'conversations', 'outbound', 'shiftflow', 'buchhaltung',
  'calendar', 'consent', 'customers', 'cv-print-builder', 'esign', 'intake',
  'interviews', 'invoices', 'nachweise', 'placements', 'submissions', 'support',
]);

function prepareUiCatalogFixture(sourceRoot, runtimeRoot) {
  const source = fs.realpathSync(sourceRoot);
  const runtime = fs.realpathSync(runtimeRoot);
  const within = (parent, child) => child === parent || child.startsWith(parent + path.sep);
  if (within(source, runtime)) throw new Error('UI catalog fixture requires a separate smoke root');
  const appRoot = path.join(runtime, 'src/apps/business-os');
  if (!within(runtime, fs.realpathSync(appRoot))) {
    throw new Error('UI catalog fixture app root escapes the smoke root');
  }
  const sourceTemplates = path.join(source, 'src/apps/business-os/template-store');
  const templates = path.join(appRoot, 'template-store');
  // Only replace the symlink made by prepareSmokeSourceRoot. Never write
  // through it, reuse an installed source tree, or rewrite the source catalog.
  if (!fs.lstatSync(templates).isSymbolicLink()
      || fs.realpathSync(templates) !== fs.realpathSync(sourceTemplates)) {
    throw new Error('UI catalog fixture requires the fresh smoke template-store symlink');
  }
  const registry = JSON.parse(fs.readFileSync(path.join(source, 'src/apps/business-os/modules/registry.json'), 'utf8'));
  const installs = CATALOG_MODULES.map((moduleId) => {
    const entries = registry.modules.filter((entry) => entry.id === moduleId);
    if (entries.length !== 1 || entries[0].install_scope !== 'store'
        || entries[0].default_installed !== false || entries[0].core !== false) {
      throw new Error(`UI catalog fixture requires one non-default store entry: ${moduleId}`);
    }
    const manifest = JSON.parse(fs.readFileSync(path.join(source, 'src/apps/business-os/modules', moduleId, 'module.json'), 'utf8'));
    if (manifest.id !== moduleId) throw new Error(`UI catalog source identity mismatch: ${moduleId}`);
    return { moduleId, templateId: `smoke-ui-${moduleId}`, title: manifest.title || moduleId };
  });
  fs.unlinkSync(templates);
  fs.cpSync(sourceTemplates, templates, { recursive: true });
  for (const item of installs) {
    const target = path.join(templates, item.templateId);
    fs.mkdirSync(target);
    fs.writeFileSync(path.join(target, 'template.json'), JSON.stringify({
      id: item.templateId,
      title: item.title,
      default_title: item.title,
      source_module: item.moduleId,
      description: 'Isolated UI regression catalog prerequisite',
    }));
  }
  return { installs };
}

module.exports = { CATALOG_MODULES, prepareUiCatalogFixture };
