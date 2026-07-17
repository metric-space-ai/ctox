import { replicationWebRtcTestInternals } from '../dist/ctox-rxdb-js.mjs';

const {
  shouldAttachFileDemandLoader,
  shouldAttachQueryDemandLoader,
  shouldPersistFetchedFileChunks,
} = replicationWebRtcTestInternals;

assert(shouldAttachQueryDemandLoader('desktop_files'), 'desktop_files must keep query demand loading');
assert(!shouldAttachQueryDemandLoader('desktop_file_chunks'), 'desktop_file_chunks must not query-fetch');
assert(!shouldAttachQueryDemandLoader('document_blob_chunks'), 'blob chunk collections must not query-fetch');
assert(!shouldAttachQueryDemandLoader('spreadsheet_blob_chunks'), 'spreadsheet chunk collections must not query-fetch');
assert(shouldAttachFileDemandLoader('document_blob_chunks'), 'document blobs must use file demand loading');
assert(shouldAttachFileDemandLoader('spreadsheet_blob_chunks'), 'spreadsheet blobs must use file demand loading');
assert(!shouldPersistFetchedFileChunks('document_blob_chunks'), 'raw document streams must not be written into the structured blob schema');
assert(!shouldPersistFetchedFileChunks('spreadsheet_blob_chunks'), 'raw spreadsheet streams must not be written into the structured blob schema');

console.log('ctox-rxdb chunk query demand disabled smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
