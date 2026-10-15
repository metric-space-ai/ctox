//! Opt-in E12 profile. Uses the real writer with 8 admitted producers.
use crate::types::WriteJob;
use crate::{
    config::ConnectionConfig, registry::Registry, writer::WriterOptions, DbId, JobOptions,
};
use rusqlite::{ffi, params};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Default)]
struct Samples {
    first: Option<Instant>,
    commit: Option<Instant>,
    commits: Vec<f64>,
    holds: Vec<f64>,
    jobs: usize,
    batch_jobs: Vec<usize>,
}
unsafe extern "C" fn committed(
    ctx: *mut std::ffi::c_void,
    _db: *mut ffi::sqlite3,
    _name: *const std::ffi::c_char,
    _pages: i32,
) -> i32 {
    // The benchmark holds this Arc until the writer has closed its connection.
    let state = unsafe { &*(ctx as *const Mutex<Samples>) };
    let now = Instant::now();
    let mut s = state.lock().unwrap();
    if let (Some(first), Some(commit)) = (s.first.take(), s.commit.take()) {
        s.commits
            .push(now.duration_since(commit).as_secs_f64() * 1e6);
        s.holds.push(now.duration_since(first).as_secs_f64() * 1e6);
        let jobs = std::mem::take(&mut s.jobs);
        s.batch_jobs.push(jobs);
    }
    0
}
fn percentile(v: &[f64], p: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).ceil() as usize]
}

fn profile(bytes: usize, triggers: bool) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m2.sqlite");
    let samples = Arc::new(Mutex::new(Samples::default()));
    let hooks = samples.clone();
    let options = WriterOptions {
        configure: Some(Arc::new(move |conn| {
            ConnectionConfig::default().apply_writer(conn)?;
            conn.execute_batch("CREATE TABLE c_profile(id TEXT PRIMARY KEY NOT NULL,rev TEXT NOT NULL,schema_ver INTEGER NOT NULL,
            data TEXT CHECK(json_valid(data)),deleted INTEGER NOT NULL)")?;
            let tx = conn.unchecked_transaction()?;
            if triggers {
                super::install_triggers(
                    &tx,
                    &super::TableBinding::generic("profile", 1, "c_profile", 1, "1"),
                )?;
            }
            tx.commit()?;
            let begin = hooks.clone();
            conn.commit_hook(Some(move || {
                begin.lock().unwrap().commit = Some(Instant::now());
                false
            }));
            // A WAL callback occurs after SQLite's real commit, before writer acknowledgement.
            // No Connection is opened or used on any producer.
            unsafe {
                ffi::sqlite3_wal_hook(
                    conn.handle(),
                    Some(committed),
                    Arc::as_ptr(&hooks) as *mut std::ffi::c_void,
                );
            }
            Ok(())
        })),
        head_seq: None,
    };
    let writer = Registry.open(DbId::new(&path).unwrap(), options).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(writer.ready()).unwrap();
    let payload = Arc::new(json!({"content":"x".repeat(bytes-14)}).to_string());
    let wal_start = std::fs::metadata(path.with_extension("sqlite-wal"))
        .unwrap()
        .len();
    let barrier = Arc::new(std::sync::Barrier::new(8));
    std::thread::scope(|scope| {
        for producer in 0..8 {
            let writer = writer.clone();
            let payload = payload.clone();
            let stats = samples.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                // At most 8x4 inflight: 64KiB batches stay below the writer execution budget.
                for round in 0..32 {
                    let mut pending = Vec::new();
                    for item in 0..4 {
                        let body = payload.clone();
                        let stats = stats.clone();
                        let id = format!("{producer}-{round}-{item}");
                        pending.push(
                            writer
                                .submit(WriteJob::new(
                                    JobOptions {
                                        batch_key: Some("m2-profile".into()),
                                        ..Default::default()
                                    },
                                    move |tx| {
                                        {
                                            let mut s = stats.lock().unwrap();
                                            s.first.get_or_insert_with(Instant::now);
                                            s.jobs += 1;
                                        }
                                        tx.execute(
                                            "INSERT INTO c_profile VALUES(?1,'1',1,?2,0)",
                                            params![id, body.as_str()],
                                        )?;
                                        Ok(())
                                    },
                                ))
                                .unwrap(),
                        );
                    }
                    for job in pending {
                        job.blocking_wait().unwrap();
                    }
                }
            });
        }
    });
    // Keep the owner alive to measure the WAL before shutdown's final checkpoint.
    let wal_bytes = std::fs::metadata(path.with_extension("sqlite-wal"))
        .unwrap()
        .len();
    runtime.block_on(writer.shutdown()).unwrap();
    let s = samples.lock().unwrap();
    assert_eq!(s.batch_jobs.iter().sum::<usize>(), 1024);
    assert!(
        s.batch_jobs.iter().any(|n| *n > 1),
        "real batching must occur"
    );
    json!({"payload_bytes":payload.len(),"triggers":triggers,"producers":8,"mutations":1024,"commits":s.commits.len(),
        "commit_p50_us":percentile(&s.commits,0.50),"commit_p99_us":percentile(&s.commits,0.99),
        "writer_hold_p50_us":percentile(&s.holds,0.50),"writer_hold_p99_us":percentile(&s.holds,0.99),
        "wal_growth_bytes":wal_bytes-wal_start,
        "commit_per_mutation_p50_us":percentile(&s.commits.iter().zip(&s.batch_jobs).map(|(t,n)|t / *n as f64).collect::<Vec<_>>(),0.50),
        "commit_per_mutation_p99_us":percentile(&s.commits.iter().zip(&s.batch_jobs).map(|(t,n)|t / *n as f64).collect::<Vec<_>>(),0.99),"batch_jobs_p50":percentile(&s.batch_jobs.iter().map(|x|*x as f64).collect::<Vec<_>>(),0.50)})
}

#[test]
#[ignore = "explicit E12 M2 WAL/writer measurement"]
fn measure_m2_profile() {
    for bytes in [512, 4096, 65536] {
        // Alternate run order to expose ordering/cache sensitivity, retain every raw sample.
        let mut rows = Vec::new();
        for trial in 0..4 {
            for triggered in [trial % 2 == 0, trial % 2 != 0] {
                let mut row = profile(bytes, triggered);
                row["trial"] = json!(trial);
                rows.push(row);
            }
        }
        let med = |on: bool, key: &str| {
            let v: Vec<_> = rows
                .iter()
                .filter(|r| r["triggers"] == json!(on))
                .map(|r| r[key].as_f64().unwrap())
                .collect();
            percentile(&v, 0.5)
        };
        let p50 = (med(true, "commit_p50_us") / med(false, "commit_p50_us") - 1.0) * 100.0;
        let p99 = (med(true, "commit_p99_us") / med(false, "commit_p99_us") - 1.0) * 100.0;
        println!(
            "M2_PROFILE {}",
            json!({"document_bytes":bytes,"samples":rows,"commit_p50_overhead_percent":p50,
            "commit_p99_overhead_percent":p99,"target_p50_percent":15,"target_p99_percent":25,"matched_batch_medians":med(true,"batch_jobs_p50")==med(false,"batch_jobs_p50"),
            "target_met":p50<=15.0&&p99<=25.0&&med(true,"batch_jobs_p50")==med(false,"batch_jobs_p50"),
            "commit_timing":"SQLite commit_hook to postcommit WAL hook","writer_hold_timing":"first writer job to postcommit WAL hook",
            "wal":"NORMAL; canonical ConnectionConfig; autocheckpoint disabled; no reader/installed acceptance claim"})
        );
    }
}
