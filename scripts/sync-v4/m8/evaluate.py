#!/usr/bin/env python3
"""Fail-closed M8 evaluator. Missing/empty measurements never pass."""
import argparse, json, math, pathlib, sys
def quantile(values, q):
    if not values or any(not isinstance(x,(int,float)) or not math.isfinite(x) or x<0 for x in values):
        return None
    values=sorted(values)
    return values[max(0,math.ceil(len(values)*q)-1)]
def stats(values):
    return {name:quantile(values,q) for name,q in [('p50',.5),('p95',.95),('p99',.99)]}
def evaluate(profiles, command_override=None):
    rows=[];criteria=[]
    for rtt in (20,150,600):
        p=profiles.get(rtt,{})
        cmd=stats(p.get('command_samples_ms',[]));term=stats(p.get('terminal_samples_ms',[]));live=stats(p.get('live_push_samples_ms',[]))
        def record(name, measured, limit, passed):
            criteria.append(dict(profile_rtt_ms=rtt,criterion=name,measured=measured,limit=limit,pass_=bool(passed)))
            criteria[-1]['pass']=criteria[-1].pop('pass_')
        record('native_profile_completed',p.get('completed'),True,p.get('completed') is True and not p.get('errors'))
        record('three_native_peers',p.get('peers'),'>=3',len(set(p.get('peers',[])))>=3 and p.get('associations',0)>=2 and p.get('opened_reliable_channels')==[6,6,6,6])
        command_limit=rtt+100 if command_override is None else command_override
        terminal_limit=100
        live_p95_limit=150
        live_p99_limit=500
        record('command_p99_ms',cmd['p99'],command_limit,cmd['p99'] is not None and cmd['p99']<=command_limit)
        if rtt==20:
            record('terminal_echo_p95_ms',term['p95'],terminal_limit,term['p95'] is not None and term['p95']<=terminal_limit)
        record('live_push_p95_ms',live['p95'],live_p95_limit,live['p95'] is not None and live['p95']<=live_p95_limit)
        record('live_push_p99_ms',live['p99'],live_p99_limit,live['p99'] is not None and live['p99']<=live_p99_limit)
        directions=p.get('directions',[])
        bulk_ok=len(directions)==4 and all(d.get('hash_match') is True and d.get('source',{}).get('bytes',0)>=50*1024*1024 for d in directions)
        record('bidirectional_50MiB_artifacts',directions,'each direction >=50MiB; SHA256 matches',bulk_ok)
        record('sustained_bulk_overlap',p.get('bulk_active_probes'),p.get('logical_sent'),p.get('logical_sent',0)>0 and p.get('bulk_active_probes')==p.get('logical_sent'))
        # Exact workload cardinalities are measurement accounting, not a unit-test score.
        logical=p.get('logical_sent',0)==192 and p.get('logical_received')==192
        command_docs=p.get('accepted_commands')==64 and p.get('terminal_commands')==64 and p.get('projection_reads')==64 and p.get('signed_live_packets')==128 and len(p.get('live_replicas',[]))==2 and all(r.get('documents')==64 and r.get('packets')==64 for r in p.get('live_replicas',[]))
        silent_loss=not (logical and command_docs and bulk_ok and p.get('application_duplicate_replays')==0)
        record('zero_silent_loss',dict(sent=p.get('logical_sent'),received=p.get('logical_received'),live=p.get('signed_live_packets'),duplicates=p.get('application_duplicate_replays')),0,not silent_loss)
        impairment=p.get('impairment',{})
        offered=impairment.get('udp_offered',0);dropped=impairment.get('udp_dropped',0)
        loss_pct=100*dropped/offered if offered else None
        record('udp_loss_injected',loss_pct,'seeded 1% Bernoulli UDP loss; drops observed',offered>0 and dropped>0 and impairment.get('requested_loss_pct')==1 and impairment.get('udp_send_errors')==0)
        rows.append(dict(rtt_ms=rtt,command_ms=cmd,terminal_ms=term,live_push_ms=live,udp_loss_pct=loss_pct,pass_=all(c['pass'] for c in criteria if c['profile_rtt_ms']==rtt)))
        rows[-1]['pass']=rows[-1].pop('pass_')
    component_pass=all(c['pass'] for c in criteria)
    return dict(profile='component profile, pre-cutover',pass_=False,component_profile_pass=component_pass,
        product_acceptance=False,product_blockers=['CUTOVER-3 productive participant/policy/executor bindings not exercised','TURN not measured'],
        criteria=criteria,measured=rows,zero_silent_loss=all(c['pass'] for c in criteria if c['criterion']=='zero_silent_loss'),
        replay={'application_duplicate_replays':sum(p.get('application_duplicate_replays',0) for p in profiles.values()),
                'session_reconnect_replay':'not exercised','sctp_retransmissions':None})
def write(out, result):
    result['pass']=result.pop('pass_',False)
    (out/'m8.json').write_text(json.dumps(result,indent=2)+'\n')
    lines=['| RTT ms | Command p50/p95/p99 ms | Terminal p50/p95/p99 ms | Live p95/p99 ms | Component |',
           '|---:|---|---|---|---|']
    def f(x): return 'unmeasured' if x is None else f'{x:.3f}'
    for row in result['measured']:
        lines.append('| '+str(row['rtt_ms'])+' | '+' / '.join(f(row['command_ms'][q]) for q in ('p50','p95','p99'))+' | '+' / '.join(f(row['terminal_ms'][q]) for q in ('p50','p95','p99'))+' | '+' / '.join(f(row['live_push_ms'][q]) for q in ('p95','p99'))+' | '+('pass' if row['pass'] else 'FAIL')+' |')
    lines+=['','Profile: **component profile, pre-cutover**. Product M8: **not passed**.','TURN not measured. See m8.json for every failed or unmeasured criterion.']
    (out/'m8.md').write_text('\n'.join(lines)+'\n')
def main():
    a=argparse.ArgumentParser();a.add_argument('out',type=pathlib.Path);a.add_argument('--command-threshold-ms',type=float);a.add_argument('--assert-criterion');a.add_argument('--rtt',type=int)
    args=a.parse_args()
    profiles={r:json.loads((args.out/f'profile-{r}.json').read_text()) for r in (20,150,600) if (args.out/f'profile-{r}.json').exists()}
    result=evaluate(profiles,args.command_threshold_ms)
    write(args.out,result)
    selected=[c for c in result['criteria'] if (args.assert_criterion is None or c['criterion']==args.assert_criterion) and (args.rtt is None or c['profile_rtt_ms']==args.rtt)]
    return 0 if selected and all(c['pass'] for c in selected) else 1
if __name__=='__main__':sys.exit(main())
