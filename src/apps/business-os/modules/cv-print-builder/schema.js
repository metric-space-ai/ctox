import { collections as ctoxCollections } from '../ctox/schema.js';
import { collections as desktopCollections } from '../desktop/schema.js';
import { collections as documentCollections } from '../documents/schema.js';

export const collections = {
  business_chats: ctoxCollections.business_chats,
  desktop_files: desktopCollections.desktop_files,
  desktop_file_chunks: desktopCollections.desktop_file_chunks,
  documents: documentCollections.documents,
  document_versions: documentCollections.document_versions,
};

export const migrationStrategies = {};
