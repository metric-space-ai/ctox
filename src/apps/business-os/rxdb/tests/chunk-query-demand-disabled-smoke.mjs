import { replicationWebRtcTestInternals } from '../dist/ctox-rxdb-js.mjs';

const { shouldAttachQueryDemandLoader } = replicationWebRtcTestInternals;

assert(shouldAttachQueryDemandLoader('desktop_files'), 'desktop_files must keep query demand loading');
assert(!shouldAttachQueryDemandLoader('desktop_file_chunks'), 'desktop_file_chunks must not query-fetch');
assert(!shouldAttachQueryDemandLoader('document_blob_chunks'), 'blob chunk collections must not query-fetch');
assert(!shouldAttachQueryDemandLoader('spreadsheet_blob_chunks'), 'spreadsheet chunk collections must not query-fetch');

console.log('ctox-rxdb chunk query demand disabled smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
