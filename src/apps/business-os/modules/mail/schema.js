// Mail is an operational view over the canonical CTOX communication and
// outbound stores. Re-exporting their schemas prevents a second mail truth and
// keeps every shared collection byte-identical across modules.
import {
  collections as conversationCollections,
  migrationStrategies as conversationMigrationStrategies,
} from '../../modules/conversations/schema.js';
import { collections as ctoxCollections } from '../../modules/ctox/schema.js?v=20260816-browser-sync-guards-v141';
import { collections as appStoreCollections } from '../../modules/app-store/schema.js';
import { collections as documentCollections } from '../../modules/documents/schema.js';

const MAIL_COLLECTIONS = [
  'communication_accounts',
  'communication_threads',
  'communication_messages',
  'outbound_campaigns',
  'outbound_pipeline_items',
  'outbound_engagements',
  'outbound_messages',
  'outbound_approvals',
];

export const collections = Object.fromEntries(
  MAIL_COLLECTIONS.map((name) => [name, conversationCollections[name]]),
);
collections.business_users = ctoxCollections.business_users;
collections.business_commands = appStoreCollections.business_commands;
collections.business_module_catalog = appStoreCollections.business_module_catalog;
collections.documents = documentCollections.documents;
collections.document_versions = documentCollections.document_versions;
collections.document_blob_chunks = documentCollections.document_blob_chunks;

export const migrationStrategies = Object.fromEntries(
  MAIL_COLLECTIONS
    .filter((name) => conversationMigrationStrategies[name])
    .map((name) => [name, conversationMigrationStrategies[name]]),
);
migrationStrategies.business_commands = appStoreCollections.business_commands.version > 0
  ? {
      1: (oldDoc) => ({ ...oldDoc, inbound_channel: oldDoc.inbound_channel || oldDoc.module || '' }),
      2: (oldDoc) => oldDoc,
    }
  : {};
