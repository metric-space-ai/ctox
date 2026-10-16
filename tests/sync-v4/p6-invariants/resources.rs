//! Real Node child and IPC; uses the existing process harness, not policy mocks as evidence.
use super::*;

#[test]
fn p6_process_cpu_budget_kills_only_the_app_and_allows_replacement() {
    let h = Harness::new(Duration::from_millis(150), IDLE_LIMIT);
    h.register("export default input=>{if(input.loop){while(true){}}return input;}");
    assert_eq!(
        h.invoke(json!({"loop":false})).unwrap(),
        json!({"loop":false})
    );
    let pid = h.pool.process_id("test-app").unwrap();
    let start = Instant::now();
    assert!(
        h.invoke(json!({"loop":true})).is_err(),
        "unbounded CPU handler completed without quota failure"
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "CPU quota was not enforced promptly"
    );
    assert!(
        h.pool.process_id("test-app").is_none(),
        "exhausted app process retained"
    );
    assert_eq!(h.invoke(json!({"value":7})).unwrap(), json!({"value":7}));
    assert_ne!(h.pool.process_id("test-app"), Some(pid));
}

#[test]
fn p6_process_result_budget_rejects_oversized_message() {
    let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
    h.register("export default input=>input.large?'x'.repeat(1024*1024+1):'ok';");
    assert_eq!(h.invoke(json!({"large":false})).unwrap(), json!("ok"));
    assert!(
        h.invoke(json!({"large":true})).is_err(),
        "oversized app result passed IPC budget"
    );
    assert!(h.pool.process_id("test-app").is_none());
}

#[test]
fn p6_process_old_space_heap_limit_is_enforced() {
    let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
    // §16.9 specifies V8 old space, rather than a limit on total process RSS.
    // Retain at most 320 MiB of ordinary heap arrays, never an unbounded loop.
    h.register("export default input=>{if(!input.exhaust)return 1;const chunks=[];for(let n=0;n<160;n++)chunks.push(new Array(262144).fill(n));return chunks.length;};");
    assert_eq!(h.invoke(json!({"exhaust":false})).unwrap(), json!(1));
    let pid = h.pool.process_id("test-app").unwrap();
    assert!(
        h.invoke(json!({"exhaust":true})).is_err(),
        "320 MiB old-space allocation succeeded beyond the configured 256 MiB heap limit"
    );
    assert!(h.pool.process_id("test-app").is_none());
    assert_eq!(h.invoke(json!({"exhaust":false})).unwrap(), json!(1));
    assert_ne!(h.pool.process_id("test-app"), Some(pid));
}
