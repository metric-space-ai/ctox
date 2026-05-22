export {
    addRxPlugin,
    createRxDatabase,
    removeRxDatabase
} from './index.ts';
export { getRxStorageDexie } from './plugins/storage-dexie/index.ts';
export {
    getConnectionHandlerSimplePeer,
    replicateWebRTC
} from './plugins/replication-webrtc/index.ts';
export { RxDBMigrationSchemaPlugin } from './plugins/migration-schema/index.ts';
export { wrappedValidateAjvStorage } from './plugins/validate-ajv/index.ts';
export { wrappedValidateZSchemaStorage } from './plugins/validate-z-schema/index.ts';
