import { createCtoxOfficeEditor as createEditor } from './capsule.mjs';

export function createCtoxSpreadsheetsEditor(options = {}) {
  return createEditor({ ...options, kind: 'spreadsheet' });
}

// Compatibility alias for external consumers of the original stable factory.
// CTOX Business OS itself imports the product-specific factory above.
export const createCtoxOfficeEditor = createCtoxSpreadsheetsEditor;
export const CTOX_SPREADSHEETS_EDITOR_KIND = 'spreadsheet';
export const CTOX_SPREADSHEETS_PRODUCT_ID = 'ctox-spreadsheets';
export const CTOX_OFFICE_EDITOR_KIND = CTOX_SPREADSHEETS_EDITOR_KIND;
