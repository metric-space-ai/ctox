import { createCtoxOfficeEditor as createEditor } from './capsule.mjs';

export function createCtoxDocumentsEditor(options = {}) {
  return createEditor({ ...options, kind: 'document' });
}

// Compatibility alias for external consumers of the original stable factory.
// CTOX Business OS itself imports the product-specific factory above.
export const createCtoxOfficeEditor = createCtoxDocumentsEditor;
export const CTOX_DOCUMENTS_EDITOR_KIND = 'document';
export const CTOX_DOCUMENTS_PRODUCT_ID = 'ctox-documents';
export const CTOX_OFFICE_EDITOR_KIND = CTOX_DOCUMENTS_EDITOR_KIND;
