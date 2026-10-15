import fs from 'node:fs';
const evidence=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const failures=[];
if (!evidence.terminal || !evidence.measured) failures.push('measurement incomplete: '+(evidence.error||''));
if (!evidence.relayProof?.length || !evidence.measurements?.relayAlwaysObserved) failures.push('relay not verified');
if (evidence.config.peers !== 3) failures.push('requires 3 peers');
if (evidence.config.durationSeconds === 600 && evidence.injectedDisconnectsPerBrowser < 19)
  failures.push('missing 30s disconnect injections');
const totals=evidence.nativeEvents?.filter(e=>e.kind==='status').at(-1)?.sendCounters;
const errorEvents=evidence.nativeEvents?.filter(e=>e.kind==='send_failure_event') || [];
const failed=totals ? Object.values(totals).reduce((n,c)=>n+c.too_large+c.stream_missing+c.not_ready+c.sctp_rejected,0) : null;
if(failed === null || failed !== errorEvents.length || errorEvents.some(e=>e.failure.kind==='event_subscriber_lagged'))
  failures.push('send error/event counter mismatch');
if (evidence.config.durationSeconds !== 600) failures.push('smoke only; 600s required for M3');
for(const direction of ['browserToNative','nativeToBrowser']) {
 const classes = evidence.measurements?.[direction] || {};
 if(Object.keys(classes).length !== 6) failures.push(direction+':missing workload classes');
 for (const [cls, counts] of Object.entries(classes)) {
   if(counts.reliableCountersEqual !== true || counts.sendResultMissing !== 0
     || counts.receivedWithoutAcceptedSend?.length || counts.sendAccepted <= 0)
     failures.push(direction+':'+cls+':counter mismatch');
 }
}
evidence.m3Acceptance={passed:failures.length===0,failures,
 scope:'reliable workload classes; no audio/video or netem assertion'};
fs.writeFileSync(process.argv[2],JSON.stringify(evidence,null,2));
console.log(JSON.stringify(evidence.m3Acceptance));
if(failures.length) process.exitCode=1;
