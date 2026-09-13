mod provider_continuation_execution {
    use super::*;

    const SCRIPT: &str = r#"
const {createHash}=require('node:crypto');
const path=require('node:path');
const raw=process.env.CTOX_SCRAPE_INPUT_JSON;
const input=JSON.parse(raw);
if(input.mode==='prior') {
  console.log(JSON.stringify({records:[{id:'prior',company_name:'Previous Company'}]}));
} else {
  const receipt={schema:'ctox.scrape.provider_continuation.v1',
    run_id:path.basename(process.env.CTOX_SCRAPE_RUN_DIR),
    target_key:process.env.CTOX_SCRAPE_TARGET_KEY,
    input_sha256:createHash('sha256').update(raw).digest('hex'),
    operation_id:input.research_operation_id,source_id:input.source_id,
    provider:'brightdata',company:input.company,country:input.country,
    dataset_id:'gd_l1viktl72bvl7bjuj0',snapshot_id:'sd_fixture',
    query_hash:'b'.repeat(64),phase:'pending',submission_attempt:1,retry_after_seconds:15};
  if(input.mode==='foreign') receipt.operation_id='research-v1-'+'c'.repeat(64);
  console.log(JSON.stringify({records:[],failure_mode:'awaiting_provider',continuation:receipt}));
}
"#;

    fn fixture(root: &Path) -> ScrapeTargetView {
        let target = upsert_target(
            root,
            DEFAULT_RUNTIME_ROOT,
            json!({
                "target_key":"linkedin-com", "display_name":"LinkedIn fixture",
                "start_url":"https://www.linkedin.com/", "target_kind":"prospect-research",
                "config":{"skip_probe":true,"expected_min_records":1,
                    "async_provider":"brightdata","expected_provider":"linkedin.com",
                    "llm_enrichment":{"enabled":false}},
                "output_schema":{"schema_key":"prospect.v1"}
            }),
        )
        .unwrap();
        let script = root.join("provider-wait.js");
        fs::write(&script, SCRIPT).unwrap();
        register_script(
            root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            script.to_str().unwrap(),
            "javascript",
            None,
            None,
        )
        .unwrap();
        target
    }

    #[test]
    fn configured_research_binding_rejects_provider_and_script_drift_before_execution() {
        let root = temp_root("configured-research-binding");
        fixture(&root);
        assert!(configured_research_target_binding(&root, "absent", "linkedin.com").is_err());
        assert!(configured_research_target_binding(&root, "linkedin-com", "xing.com").is_err());
        let binding =
            configured_research_target_binding(&root, "linkedin-com", "linkedin.com").unwrap();
        let script = root.join("changed-provider.js");
        fs::write(&script, format!("{SCRIPT}\n// changed revision\n")).unwrap();
        register_script(
            &root,
            DEFAULT_RUNTIME_ROOT,
            "linkedin-com",
            script.to_str().unwrap(),
            "javascript",
            None,
            None,
        )
        .unwrap();
        let changed =
            configured_research_target_binding(&root, "linkedin-com", "linkedin.com").unwrap();
        assert_ne!(binding, changed);
        let args = vec![
            "--target-key".into(),
            "linkedin-com".into(),
            "--input-json".into(),
            json!({"mode":"prior"}).to_string(),
        ];
        let error = execute_scrape_with_research_binding(&root, &args, &binding).unwrap_err();
        assert!(error.to_string().contains("changed before dispatch"));
        let conn = open_db(&root).unwrap();
        let runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM scrape_run", [], |row| row.get(0))
            .unwrap();
        assert_eq!(runs, 0, "stale binding must not start a provider run");
        drop(conn);
        cleanup_test_root(&root);
    }

    #[test]
    fn configured_research_binding_tracks_registered_config_changes() {
        let root = temp_root("configured-research-config");
        fixture(&root);
        let before =
            configured_research_target_binding(&root, "linkedin-com", "linkedin.com").unwrap();
        upsert_target(
            &root,
            DEFAULT_RUNTIME_ROOT,
            json!({
                "target_key":"linkedin-com", "display_name":"LinkedIn fixture",
                "start_url":"https://www.linkedin.com/", "target_kind":"prospect-research",
                "config":{"skip_probe":true,"expected_min_records":2,
                    "async_provider":"brightdata","expected_provider":"linkedin.com",
                    "llm_enrichment":{"enabled":false}},
                "output_schema":{"schema_key":"prospect.v1"}
            }),
        )
        .unwrap();
        let after =
            configured_research_target_binding(&root, "linkedin-com", "linkedin.com").unwrap();
        assert_ne!(before, after);
        cleanup_test_root(&root);
    }

    fn execute(root: &Path, mode: &str) -> ScrapeExecutionOutcome {
        execute_scrape_with_outcome(root, &[
            "--target-key".into(), "linkedin-com".into(), "--allow-heal".into(),
            "--input-json".into(), json!({"company":"Fixture GmbH","country":"DE",
                "source_id":"linkedin.com", "research_operation_id":format!("research-v1-{}", "a".repeat(64)),
                "mode":mode}).to_string(), "--timeout-seconds".into(), "10".into(),
        ]).unwrap()
    }

    #[test]
    fn provider_continuation_persists_wait_without_materialization_or_repair() {
        let root = temp_root("provider-continuation");
        let target = fixture(&root);
        let prior = execute(&root, "prior");
        assert_eq!(prior.status, ScrapeRunStatus::Succeeded, "{prior:?}");
        let conn = open_db(&root).unwrap();
        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id=?1",
                params![target.target_id],
                |row| row.get(0),
            )
            .unwrap();
        let pending = execute(&root, "pending");
        assert_eq!(
            pending.status,
            ScrapeRunStatus::AwaitingProvider,
            "{pending:?}"
        );
        assert!(!pending.ok);
        assert!(pending.error.is_none());
        assert!(!pending.should_queue_repair);
        assert!(pending.repair_queue_task.is_none());
        assert!(pending.repair_request_path.is_none());
        assert!(pending.materialization.is_none());
        assert!(pending.query_completion.is_none());
        assert_eq!(pending.records_found, 0);
        assert!(pending.fields_extracted.is_empty());
        let receipt = pending.continuation.as_ref().unwrap();
        assert_eq!(receipt["run_id"], pending.run_id);
        let operation = format!("research-v1-{}", "a".repeat(64));
        assert_eq!(
            load_provider_wait_receipt(
                &root,
                &pending.run_id,
                "linkedin-com",
                &operation,
                "Fixture GmbH",
                "DE"
            )
            .unwrap(),
            *receipt
        );
        for (run_id, target, operation, company, country) in [
            (
                prior.run_id.as_str(),
                "linkedin-com",
                operation.as_str(),
                "Fixture GmbH",
                "DE",
            ),
            (
                pending.run_id.as_str(),
                "other-target",
                operation.as_str(),
                "Fixture GmbH",
                "DE",
            ),
            (
                pending.run_id.as_str(),
                "linkedin-com",
                "foreign-operation",
                "Fixture GmbH",
                "DE",
            ),
            (
                pending.run_id.as_str(),
                "linkedin-com",
                operation.as_str(),
                "Other GmbH",
                "DE",
            ),
            (
                pending.run_id.as_str(),
                "linkedin-com",
                operation.as_str(),
                "Fixture GmbH",
                "CH",
            ),
        ] {
            assert!(
                load_provider_wait_receipt(&root, run_id, target, operation, company, country)
                    .is_err()
            );
        }
        let (status, result): (String, String) = conn
            .query_row(
                "SELECT status,result_json FROM scrape_run WHERE run_id=?1",
                params![pending.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "awaiting_provider");
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["continuation"],
            *receipt
        );
        let manifest: Value =
            serde_json::from_slice(&fs::read(&pending.run_manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["result"]["continuation"], *receipt);
        assert_eq!(
            load_last_successful_run(&conn, &target.target_id)
                .unwrap()
                .unwrap()["run_id"],
            prior.run_id
        );
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id=?1",
                params![target.target_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
        drop(conn);
        cleanup_test_root(&root);
    }

    #[test]
    fn provider_continuation_rejects_foreign_operation_before_any_repair() {
        let root = temp_root("provider-continuation-foreign");
        fixture(&root);
        let outcome = execute(&root, "foreign");
        assert!(!outcome.ok);
        assert_eq!(outcome.reason, "invalid_provider_continuation");
        assert_eq!(outcome.status, ScrapeRunStatus::PortalDrift);
        assert!(outcome.continuation.is_none());
        assert!(outcome.materialization.is_none());
        assert!(outcome.query_completion.is_none());
        assert!(!outcome.should_queue_repair);
        assert!(outcome.repair_queue_task.is_none());
        cleanup_test_root(&root);
    }
}

mod completed_empty_query_contract {
    use super::*;

    const SCRIPT: &str = r#"
python3 - <<'CTOX_QUERY_FIXTURE'
import hashlib,json,os,pathlib,sys
raw=os.environ['CTOX_SCRAPE_INPUT_JSON']; inp=json.loads(raw)
mode=inp.get('mode','valid'); run=pathlib.Path(os.environ['CTOX_SCRAPE_RUN_DIR'])
if mode == 'records':
 print(json.dumps({'records':[{'id':'prior','company_name':'Previous Company'}]})); sys.exit(0)
r={'schema':'ctox.scrape.query_completion.v1','run_id':run.name,
 'target_key':os.environ['CTOX_SCRAPE_TARGET_KEY'],'input_sha256':hashlib.sha256(raw.encode()).hexdigest(),
 'query':inp['company'],'provider_url':'https://example.com/search','http_status':200,
 'completed':True,'results_page':True,'blocked':False,'matched_records':0,'observed_records':8,
 'observation':{'publishers':['Other Company'],'exact_matches':[]}}
if mode == 'wrong_run': r['run_id']='old-run'
if mode == 'wrong_target': r['target_key']='other-target'
if mode == 'wrong_input': r['input_sha256']='0'*64
if mode == 'wrong_query': r['query']='Different Company'
if mode == 'wrong_provider': r['provider_url']='https://other.example/search'
if mode == 'blocked': r['blocked']=True
if mode == 'http_error': r['http_status']=503
if mode == 'not_completed': r['completed']=False
if mode == 'wrong_count': r['matched_records']=1
if mode == 'no_observation': r['observation']={}
b=json.dumps(r).encode(); path=run/'outputs/query-evidence.json'; path.write_bytes(b)
p={'records':[],'query_completion':{'evidence_path':'outputs/query-evidence.json','evidence_sha256':hashlib.sha256(b).hexdigest()}}
if mode == 'wrong_hash': p['query_completion']['evidence_sha256']='0'*64
if mode == 'missing_evidence': path.unlink()
if mode == 'absent_receipt': del p['query_completion']
if mode == 'blocked': p['failure_mode']='blocked'
print(json.dumps(p)); sys.exit(2 if mode == 'exit_error' else 0)
CTOX_QUERY_FIXTURE
"#;

    fn fixture(root: &Path) -> ScrapeTargetView {
        let target = upsert_target(root, DEFAULT_RUNTIME_ROOT, json!({
            "target_key":"query-provider", "display_name":"Query Provider",
            "start_url":"https://example.com/search", "target_kind":"company",
            "config":{"skip_probe":true,"expected_min_records":1,"llm_enrichment":{"enabled":false}},
            "output_schema":{"schema_key":"company.v1"}
        })).unwrap();
        let path = root.join("query.sh");
        fs::write(&path, SCRIPT).unwrap();
        register_script(
            root,
            DEFAULT_RUNTIME_ROOT,
            &target.target_key,
            path.to_str().unwrap(),
            "shell",
            None,
            None,
        )
        .unwrap();
        target
    }

    fn execute(root: &Path, mode: &str) -> ScrapeExecutionOutcome {
        execute_scrape_with_outcome(
            root,
            &[
                "--target-key".into(),
                "query-provider".into(),
                "--input-json".into(),
                json!({"company":"Current Company","mode":mode}).to_string(),
                "--timeout-seconds".into(),
                "10".into(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn completed_empty_query_persists_current_receipt_without_fabricated_or_old_records() {
        let root = temp_root("completed-empty-query");
        let target = fixture(&root);
        let previous = execute(&root, "records");
        assert_eq!(previous.status, ScrapeRunStatus::Succeeded, "{previous:?}");
        let before: i64 = open_db(&root)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id=?1",
                params![target.target_id],
                |row| row.get(0),
            )
            .unwrap();
        let outcome = execute(&root, "valid");
        assert_eq!(outcome.status, ScrapeRunStatus::CompletedEmpty);
        assert!(outcome.ok);
        assert!(!outcome.should_queue_repair);
        assert!(outcome.repair_request_path.is_none());
        assert!(outcome.repair_queue_task.is_none());
        assert!(outcome.error.is_none());
        assert_eq!(outcome.records_found, 0);
        assert!(outcome.fields_extracted.is_empty());
        assert!(outcome.materialization.is_none());
        assert_ne!(outcome.run_id, previous.run_id);
        let receipt = outcome.query_completion.as_ref().unwrap();
        assert_eq!(receipt["run_id"], outcome.run_id);
        assert_eq!(receipt["query"], "Current Company");
        let conn = open_db(&root).unwrap();
        let (status, result): (String, String) = conn
            .query_row(
                "SELECT status,result_json FROM scrape_run WHERE run_id=?1",
                params![outcome.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "completed_empty");
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["query_completion"],
            *receipt
        );
        assert_eq!(
            load_last_successful_run(&conn, &target.target_id)
                .unwrap()
                .unwrap()["run_id"],
            previous.run_id
        );
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scrape_record_latest WHERE target_id=?1",
                params![target.target_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            after, before,
            "empty query must not materialize or erase records"
        );
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(&outcome.run_manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["result"]["query_completion"], *receipt);
        let evidence = manifest["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["artifact_kind"] == "query_completion_evidence")
            .unwrap();
        let bytes = fs::read(evidence["path"].as_str().unwrap()).unwrap();
        assert_eq!(evidence["content_sha256"], compute_sha256_bytes(&bytes));
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *receipt);
        drop(conn);
        cleanup_test_root(&root);
    }

    #[test]
    fn invalid_empty_query_receipts_persist_failure_and_never_become_success() {
        let root = temp_root("invalid-empty-query");
        fixture(&root);
        for mode in [
            "wrong_run",
            "wrong_target",
            "wrong_input",
            "wrong_query",
            "wrong_provider",
            "blocked",
            "http_error",
            "not_completed",
            "wrong_count",
            "no_observation",
            "wrong_hash",
            "missing_evidence",
            "absent_receipt",
            "exit_error",
        ] {
            let outcome = execute(&root, mode);
            assert!(!outcome.ok, "{mode}");
            assert_ne!(outcome.status, ScrapeRunStatus::CompletedEmpty, "{mode}");
            assert!(outcome.query_completion.is_none(), "{mode}");
            assert!(outcome.materialization.is_none(), "{mode}");
            if mode == "blocked" {
                assert_eq!(outcome.status, ScrapeRunStatus::Blocked, "{outcome:?}");
            }
            let persisted: String = open_db(&root)
                .unwrap()
                .query_row(
                    "SELECT status FROM scrape_run WHERE run_id=?1",
                    params![outcome.run_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(persisted, outcome.status.as_str(), "{mode}");
        }
        cleanup_test_root(&root);
    }
}

// In-tree behavioral tests for the scrape capability (extracted from
// mod.rs; `use super::*` keeps access to crate-private internals).
use super::*;
use std::io::BufReader;
use std::net::TcpListener;

#[test]
fn scrape_runner_env_allowlist_keeps_runtime_drops_secrets() {
    // Things node/Playwright need to start are preserved...
    assert!(is_preserved_runner_env_key("PATH"));
    assert!(is_preserved_runner_env_key("home")); // case-insensitive
    assert!(is_preserved_runner_env_key("LANG"));
    // ...but daemon secrets and arbitrary env are not handed to the
    // untrusted, auto-heal-rewritable runner.
    assert!(!is_preserved_runner_env_key("OPENAI_API_KEY"));
    assert!(!is_preserved_runner_env_key("DNB_DIRECT_API_KEY"));
    assert!(!is_preserved_runner_env_key("AWS_SECRET_ACCESS_KEY"));
    assert!(!is_preserved_runner_env_key("CTOX_SECRET_TOKEN"));
    assert!(!is_preserved_runner_env_key("GITHUB_TOKEN"));
}

use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

static SCRAPE_EXEC_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn scrape_record_provenance_binds_executor_run_and_evidence() {
    let mut records = vec![json!({
        "source_id": "northdata",
        "source_url": "https://www.northdata.de/Example+Industrial+GmbH",
        "field": "company_name",
        "value": "Example Industrial GmbH"
    })];

    bind_scrape_record_provenance(
        &mut records,
        "run-123",
        Some(r#"{"company":" Example Industrial GmbH ","source_id":"northdata.de"}"#),
        "2026-07-21T19:00:00Z",
    );

    let record = records[0].as_object().unwrap();
    assert_eq!(record.get("run_id"), Some(&json!("run-123")));
    assert_eq!(
        record.get("company_name"),
        Some(&json!("Example Industrial GmbH"))
    );
    assert_eq!(record.get("source_id"), Some(&json!("northdata.de")));
    let evidence = record.get("evidence_gate").unwrap();
    assert_eq!(evidence["evidence_eligible"], json!(true));
    assert_eq!(evidence["verification_status"], json!("verified"));
    assert_eq!(evidence["checked_at"], json!("2026-07-21T19:00:00Z"));
    assert_eq!(evidence["receipt_kind"], json!("registered_scrape_output"));
    assert!(evidence["snapshot_hash"]
        .as_str()
        .is_some_and(|value| value.starts_with("sha256:") && value.len() == 71));
}

#[test]
fn scrape_record_provenance_preserves_adapter_supplied_receipts() {
    let mut records = vec![json!({
        "run_id": "adapter-run",
        "company_name": "Adapter Company",
        "evidence_gate": {
            "evidence_eligible": true,
            "verification_status": "verified",
            "snapshot_hash": "sha256:adapter"
        }
    })];

    bind_scrape_record_provenance(
        &mut records,
        "executor-run",
        Some(r#"{"company":"Input Company"}"#),
        "2026-07-21T19:00:00Z",
    );

    assert_eq!(records[0]["run_id"], json!("adapter-run"));
    assert_eq!(records[0]["company_name"], json!("Adapter Company"));
    assert_eq!(
        records[0]["evidence_gate"]["snapshot_hash"],
        json!("sha256:adapter")
    );
    assert!(records[0]["evidence_gate"].get("receipt_kind").is_none());
}

struct TestFeedServer {
    addr: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for TestFeedServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(&self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn start_test_feed_server() -> TestFeedServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_text = format!("127.0.0.1:{}", addr.port());
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request_line = String::new();
                    let _ = std::io::BufRead::read_line(
                        &mut BufReader::new(stream.try_clone().unwrap()),
                        &mut request_line,
                    );
                    let path = request_line
                        .split_terminator(['\r', '\n'])
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let (status, body) = match path {
                        "/rss.xml" => (
                            "200 OK",
                            r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
<title>Fixture RSS</title>
<item>
  <title>RSS Alpha</title>
  <link>https://example.test/rss-alpha</link>
  <description>Alpha summary</description>
  <pubDate>Mon, 01 Jan 2026 10:00:00 +0000</pubDate>
</item>
<item>
  <title>RSS Beta</title>
  <link>https://example.test/rss-beta</link>
  <description>Beta summary</description>
  <pubDate>Tue, 02 Jan 2026 10:00:00 +0000</pubDate>
</item>
  </channel>
</rss>"#,
                        ),
                        "/atom.xml" => (
                            "200 OK",
                            r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Fixture Atom</title>
  <entry>
<title>Atom Alpha</title>
<link href="https://example.test/atom-alpha" />
<summary>Atom alpha summary</summary>
<updated>2026-01-03T10:00:00+00:00</updated>
  </entry>
  <entry>
<title>Atom Beta</title>
<link href="https://example.test/atom-beta" />
<summary>Atom beta summary</summary>
<updated>2026-01-04T10:00:00+00:00</updated>
  </entry>
</feed>"#,
                        ),
                        _ => ("404 Not Found", "not found"),
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });
    TestFeedServer {
        addr: addr_text,
        stop,
        handle: Some(handle),
    }
}

fn temp_root(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ctox-scrape-{prefix}-{}",
        stable_digest(&now_iso_string())
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[cfg(unix)]
#[test]
fn embed_texts_via_local_socket_uses_internal_embedding_contract() {
    let root = std::env::temp_dir().join(format!("ce-{}", &stable_digest(&now_iso_string())[..8]));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let socket_path = root.join("e.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn(move || -> Result<()> {
        let (stream, _) = listener.accept()?;
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut request_line)?;
        assert!(request_line.contains("\"kind\":\"embeddings_create\""));
        assert!(request_line.contains("\"model\":\"Qwen/Qwen3-Embedding-0.6B\""));
        assert!(request_line.contains("\"truncate_sequence\":false"));
        let response = concat!(
            "{\"kind\":\"embeddings\",\"model\":\"Qwen/Qwen3-Embedding-0.6B\",",
            "\"data\":[[1.0,2.5],[3.0]],\"prompt_tokens\":4,\"total_tokens\":4}\n"
        );
        std::io::Write::write_all(reader.get_mut(), response.as_bytes())?;
        std::io::Write::flush(reader.get_mut())?;
        Ok(())
    });
    let inputs = vec!["alpha".to_string(), "beta".to_string()];
    let transport = LocalTransport::UnixSocket {
        path: socket_path.clone(),
    };
    let vectors =
        embed_texts_via_local_socket(&transport, &inputs, "Qwen/Qwen3-Embedding-0.6B").unwrap();
    assert_eq!(vectors, vec![vec![1.0_f64, 2.5_f64], vec![3.0_f64]]);
    server.join().unwrap().unwrap();
    cleanup_test_root(&root);
}

#[cfg(unix)]
#[test]
fn invoke_responses_text_via_local_socket_streams_internal_response_contract() {
    let root = std::env::temp_dir().join(format!("cr-{}", &stable_digest(&now_iso_string())[..8]));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let socket_path = root.join("r.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn(move || -> Result<()> {
        let (stream, _) = listener.accept()?;
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut request_line)?;
        assert!(request_line.contains("\"kind\":\"responses_create\""));
        assert!(request_line.contains("\"stream\":true"));
        assert!(request_line.contains("\"model\":\"gpt-oss-120b\""));
        assert!(request_line.contains("\"input\":\"Say hello\""));
        for line in [
            "{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n",
            "{\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n",
            "{\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output_text\":\"Hello world\"}}\n",
        ] {
            std::io::Write::write_all(reader.get_mut(), line.as_bytes())?;
        }
        std::io::Write::flush(reader.get_mut())?;
        Ok(())
    });
    let transport = LocalTransport::UnixSocket {
        path: socket_path.clone(),
    };
    let text =
        invoke_responses_text_via_local_socket(&transport, "gpt-oss-120b", "Say hello", 5).unwrap();
    assert_eq!(text, "Hello world");
    server.join().unwrap().unwrap();
    cleanup_test_root(&root);
}

fn cleanup_test_root(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

#[test]
fn upsert_target_creates_workspace_and_manifest() {
    let root = temp_root("upsert");
    let payload = json!({
        "target_key": "Acme Jobs",
        "display_name": "Acme Jobs",
        "start_url": "https://example.com/jobs",
        "target_kind": "jobs",
        "output_schema": {"schema_key": "jobs.v1"},
        "config": {"skip_probe": true}
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
    assert_eq!(target.target_key, "acme-jobs");
    assert_eq!(
        PathBuf::from(&target.workspace_dir),
        PathBuf::from(DEFAULT_RUNTIME_ROOT)
            .join("targets")
            .join("acme-jobs")
    );
    assert!(!Path::new(&target.workspace_dir).is_absolute());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("manifest.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("api/api_contract.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("api/semantic_template.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("api/llm_enrichment_template.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/sources_manifest.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/primary/source.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/primary/extractor.js")
        .is_file());
    cleanup_test_root(&root);
}

#[test]
fn deduplicated_script_registration_reactivates_the_requested_revision() {
    let root = temp_root("reactivate-script");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "reactivate-script",
            "display_name": "Reactivate Script",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let script = root.join("adapter.js");
    fs::write(&script, "process.stdout.write('A');\n").unwrap();
    let first = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("first"),
        None,
    )
    .unwrap();
    fs::write(&script, "process.stdout.write('B');\n").unwrap();
    register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("second"),
        None,
    )
    .unwrap();
    fs::write(&script, "process.stdout.write('A');\n").unwrap();
    let reactivated = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("rollback"),
        None,
    )
    .unwrap();

    assert_eq!(reactivated["deduplicated"], json!(true));
    assert_eq!(reactivated["reactivated"], json!(true));
    assert_eq!(reactivated["revision_no"], first["revision_no"]);
    let current_path = PathBuf::from(reactivated["current_path"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(current_path).unwrap(),
        "process.stdout.write('A');\n"
    );
    let active = load_target_view(&open_db(&root).unwrap(), &target.target_key)
        .unwrap()
        .unwrap();
    assert_eq!(
        active.latest_script_sha256.as_deref(),
        first["script_sha256"].as_str()
    );
    cleanup_test_root(&root);
}

#[test]
fn registered_target_loads_only_the_activated_script_revision() {
    let root = temp_root("load-active-script-only");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "load-active-script-only",
            "display_name": "Load Active Script Only",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let script = root.join("adapter.js");
    fs::write(&script, "process.stdout.write('stable');\n").unwrap();
    let stable = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("stable"),
        None,
    )
    .unwrap();
    fs::write(&script, "process.stdout.write('candidate');\n").unwrap();
    let candidate = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("candidate"),
        None,
    )
    .unwrap();
    assert!(candidate["revision_no"].as_i64().unwrap() > stable["revision_no"].as_i64().unwrap());

    let conn = open_db(&root).unwrap();
    conn.execute(
        r#"
        UPDATE scrape_target
        SET latest_script_revision_no = ?2, latest_script_sha256 = ?3
        WHERE target_id = ?1
        "#,
        params![
            target.target_id,
            stable["revision_no"].as_i64().unwrap(),
            stable["script_sha256"].as_str().unwrap()
        ],
    )
    .unwrap();

    let loaded = load_registered_target(&root, &conn, &target.target_key)
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.script.revision_no,
        stable["revision_no"].as_i64().unwrap()
    );
    assert_eq!(
        loaded.script.script_sha256,
        stable["script_sha256"].as_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(loaded.script.script_path).unwrap(),
        "process.stdout.write('stable');\n"
    );
    cleanup_test_root(&root);
}

#[test]
fn explicit_deduplicated_registration_rewrites_a_removed_path() {
    let root = temp_root("reactivate-script-removed-release");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "reactivate-script-removed-release",
            "display_name": "Reactivate Script Removed Release",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let script = root.join("adapter.js");
    let body = "process.stdout.write('stable');\n";
    fs::write(&script, body).unwrap();
    let first = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("first"),
        None,
    )
    .unwrap();
    let first_path = PathBuf::from(first["script_path"].as_str().unwrap());
    fs::remove_file(&first_path).unwrap();
    let conn = open_db(&root).unwrap();
    conn.execute(
        "UPDATE scrape_script_revision SET script_path = ?1 WHERE target_id = ?2",
        params![
            "/removed/ctox-release/runtime/scraping/targets/rev0001.js",
            target.target_id
        ],
    )
    .unwrap();
    drop(conn);

    let reactivated = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("current-release"),
        None,
    )
    .unwrap();

    assert_eq!(reactivated["deduplicated"], json!(true));
    assert_eq!(reactivated["reactivated"], json!(true));
    let revision_path = PathBuf::from(reactivated["script_path"].as_str().unwrap());
    let current_path = PathBuf::from(reactivated["current_path"].as_str().unwrap());
    assert!(revision_path.is_file());
    assert!(revision_path.starts_with(root.join(DEFAULT_RUNTIME_ROOT)));
    assert_eq!(fs::read_to_string(&revision_path).unwrap(), body);
    assert_eq!(fs::read_to_string(&current_path).unwrap(), body);
    let stored_path: String = open_db(&root)
        .unwrap()
        .query_row(
            "SELECT script_path FROM scrape_script_revision WHERE target_id = ?1",
            params![target.target_id],
            |row| row.get(0),
        )
        .unwrap();
    let stored_path = PathBuf::from(stored_path);
    assert!(!stored_path.is_absolute());
    assert_eq!(root.join(stored_path), revision_path);
    cleanup_test_root(&root);
}

#[cfg(unix)]
#[test]
fn registered_paths_resolve_after_runtime_release_switch_without_repair() {
    use std::os::unix::fs::symlink;

    let install = temp_root("relative-path-release-switch");
    let releases = install.join("releases");
    let old_release = releases.join("old-release");
    let new_release = releases.join("new-release");
    let shared_runtime = install.join("runtime-state");
    let current_link = install.join("current");
    fs::create_dir_all(&old_release).unwrap();
    fs::create_dir_all(&new_release).unwrap();
    fs::create_dir_all(&shared_runtime).unwrap();
    symlink(&shared_runtime, old_release.join("runtime")).unwrap();
    symlink(&shared_runtime, new_release.join("runtime")).unwrap();
    symlink(&old_release, &current_link).unwrap();

    let target = upsert_target(
        &current_link,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "release-bound-target",
            "display_name": "Release-bound Target",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let source = install.join("extractor.js");
    let body = "console.log(JSON.stringify({records: []}));\n";
    fs::write(&source, body).unwrap();
    register_script(
        &current_link,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        source.to_str().unwrap(),
        "javascript",
        Some("initial"),
        None,
    )
    .unwrap();

    let conn = open_db(&current_link).unwrap();
    let (stored_workspace, stored_script): (String, String) = conn
        .query_row(
            r#"
            SELECT t.workspace_dir, r.script_path
            FROM scrape_target t
            JOIN scrape_script_revision r ON r.target_id = t.target_id
            WHERE t.target_id = ?1
            "#,
            params![target.target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(!Path::new(&stored_workspace).is_absolute());
    assert!(!Path::new(&stored_script).is_absolute());
    drop(conn);

    fs::remove_file(&current_link).unwrap();
    symlink(&new_release, &current_link).unwrap();

    let conn = open_db(&current_link).unwrap();
    let changes_before = conn.total_changes();
    let loaded = load_registered_target(&current_link, &conn, &target.target_key)
        .unwrap()
        .unwrap();
    assert_eq!(conn.total_changes(), changes_before);
    assert_eq!(loaded.workspace_root, current_link.join(&stored_workspace));
    assert_eq!(
        PathBuf::from(&loaded.script.script_path),
        current_link.join(&stored_script)
    );
    assert_eq!(
        fs::read_to_string(&loaded.script.script_path).unwrap(),
        body
    );
    cleanup_test_root(&install);
}

#[test]
fn loading_registered_target_does_not_change_the_database() {
    let root = temp_root("load-is-read-only");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "read-only-load",
            "display_name": "Read-only Load",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let source = root.join("extractor.js");
    fs::write(&source, "console.log(JSON.stringify({records: []}));\n").unwrap();
    register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        source.to_str().unwrap(),
        "javascript",
        Some("initial"),
        None,
    )
    .unwrap();

    let conn = open_db(&root).unwrap();
    let changes_before = conn.total_changes();
    let loaded = load_registered_target(&root, &conn, &target.target_key)
        .unwrap()
        .unwrap();
    assert!(Path::new(&loaded.script.script_path).is_absolute());
    assert_eq!(
        conn.total_changes(),
        changes_before,
        "load_registered_target must not issue INSERT, UPDATE, or DELETE statements"
    );
    cleanup_test_root(&root);
}

#[test]
fn opening_database_migrates_legacy_absolute_paths_once() {
    let root = temp_root("migrate-absolute-paths");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "legacy-paths",
            "display_name": "Legacy Paths",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let source = root.join("extractor.js");
    let body = "console.log(JSON.stringify({records: []}));\n";
    fs::write(&source, body).unwrap();
    let registered = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        source.to_str().unwrap(),
        "javascript",
        Some("initial"),
        None,
    )
    .unwrap();
    let absolute_workspace = root.join(&target.workspace_dir);
    let absolute_script = PathBuf::from(registered["script_path"].as_str().unwrap());

    let conn = open_db(&root).unwrap();
    conn.execute(
        "UPDATE scrape_target SET workspace_dir = ?2 WHERE target_id = ?1",
        params![target.target_id, absolute_workspace.to_string_lossy()],
    )
    .unwrap();
    conn.execute(
        "UPDATE scrape_script_revision SET script_path = ?2 WHERE target_id = ?1",
        params![target.target_id, absolute_script.to_string_lossy()],
    )
    .unwrap();
    drop(conn);

    // The migration runs once per database per process; production only ever
    // meets legacy rows that predate the process. This test stages them after
    // the fact, so clear that memory to exercise the migration itself.
    super::registry::forget_path_migrations_for_test();

    let migrated = open_db(&root).unwrap();
    assert_eq!(migrated.total_changes(), 2);
    let (stored_workspace, stored_script): (String, String) = migrated
        .query_row(
            r#"
            SELECT t.workspace_dir, r.script_path
            FROM scrape_target t
            JOIN scrape_script_revision r ON r.target_id = t.target_id
            WHERE t.target_id = ?1
            "#,
            params![target.target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(root.join(&stored_workspace), absolute_workspace);
    assert_eq!(root.join(&stored_script), absolute_script);
    assert!(!Path::new(&stored_workspace).is_absolute());
    assert!(!Path::new(&stored_script).is_absolute());
    let loaded = load_registered_target(&root, &migrated, &target.target_key)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.workspace_root, absolute_workspace);
    assert_eq!(PathBuf::from(loaded.script.script_path), absolute_script);
    drop(migrated);

    let reopened = open_db(&root).unwrap();
    assert_eq!(
        reopened.total_changes(),
        0,
        "the root-relative migration is idempotent after its first run"
    );
    cleanup_test_root(&root);
}

#[test]
fn opening_database_leaves_paths_outside_root_absolute() {
    let root = temp_root("outside-root-paths");
    let external = temp_root("outside-root-storage");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "external-paths",
            "display_name": "External Paths",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {"skip_probe": true},
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let source = root.join("extractor.js");
    let body = "console.log(JSON.stringify({records: []}));\n";
    fs::write(&source, body).unwrap();
    let registered = register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        source.to_str().unwrap(),
        "javascript",
        Some("initial"),
        None,
    )
    .unwrap();
    let external_workspace = external.join("targets/external-paths");
    let external_script = external_workspace.join("scripts/revisions").join(
        Path::new(registered["script_path"].as_str().unwrap())
            .file_name()
            .unwrap(),
    );
    fs::create_dir_all(external_script.parent().unwrap()).unwrap();
    fs::write(&external_script, body).unwrap();

    let conn = open_db(&root).unwrap();
    conn.execute(
        "UPDATE scrape_target SET workspace_dir = ?2 WHERE target_id = ?1",
        params![target.target_id, external_workspace.to_string_lossy()],
    )
    .unwrap();
    conn.execute(
        "UPDATE scrape_script_revision SET script_path = ?2 WHERE target_id = ?1",
        params![target.target_id, external_script.to_string_lossy()],
    )
    .unwrap();
    drop(conn);

    let reopened = open_db(&root).unwrap();
    assert_eq!(reopened.total_changes(), 0);
    let (stored_workspace, stored_script): (String, String) = reopened
        .query_row(
            r#"
            SELECT t.workspace_dir, r.script_path
            FROM scrape_target t
            JOIN scrape_script_revision r ON r.target_id = t.target_id
            WHERE t.target_id = ?1
            "#,
            params![target.target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(PathBuf::from(&stored_workspace), external_workspace);
    assert_eq!(PathBuf::from(&stored_script), external_script);
    let loaded = load_registered_target(&root, &reopened, &target.target_key)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.workspace_root, external_workspace);
    assert_eq!(PathBuf::from(loaded.script.script_path), external_script);
    cleanup_test_root(&root);
    cleanup_test_root(&external);
}

#[test]
fn deduplicated_source_registration_reactivates_current_and_configured_module() {
    let root = temp_root("reactivate-source");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "reactivate-source",
            "display_name": "Reactivate Source",
            "start_url": "https://example.com",
            "target_kind": "company",
            "config": {
                "skip_probe": true,
                "sources": [{
                    "source_key": "primary",
                    "display_name": "Primary",
                    "start_url": "https://example.com",
                    "source_kind": "html",
                    "extraction_module": "sources/primary/extractor.js"
                }]
            },
            "output_schema": {"schema_key": "company.v1"}
        }),
    )
    .unwrap();
    let module = root.join("extractor.js");
    fs::write(&module, "module.exports = 'A';\n").unwrap();
    let first = register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "primary",
        module.to_str().unwrap(),
        "javascript",
        Some("first"),
        None,
    )
    .unwrap();
    fs::write(&module, "module.exports = 'B';\n").unwrap();
    register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "primary",
        module.to_str().unwrap(),
        "javascript",
        Some("second"),
        None,
    )
    .unwrap();
    fs::write(&module, "module.exports = 'A';\n").unwrap();
    let reactivated = register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "primary",
        module.to_str().unwrap(),
        "javascript",
        Some("rollback"),
        None,
    )
    .unwrap();

    assert_eq!(reactivated["deduplicated"], json!(true));
    assert_eq!(reactivated["reactivated"], json!(true));
    assert_eq!(reactivated["revision_no"], first["revision_no"]);
    for key in ["current_path", "configured_path"] {
        let path = PathBuf::from(reactivated[key].as_str().unwrap());
        assert_eq!(fs::read_to_string(path).unwrap(), "module.exports = 'A';\n");
    }
    cleanup_test_root(&root);
}

#[test]
fn upsert_target_normalizes_multi_source_config() {
    let root = temp_root("multi-source");
    let payload = json!({
        "target_key": "aggregated-jobs",
        "display_name": "Aggregated Jobs",
        "start_url": "https://example.com/jobs",
        "target_kind": "jobs",
        "config": {
            "sources": [
                {
                    "source_key": "board-a",
                    "display_name": "Board A",
                    "start_url": "https://a.example/jobs",
                    "source_kind": "rss",
                    "extraction_module": "sources/board-a/extractor.js",
                    "tags": ["jobs", "rss"]
                },
                {
                    "display_name": "Board B",
                    "url": "https://b.example/jobs",
                    "kind": "html"
                }
            ]
        }
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
    let sources = target_sources(&target);
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].source_key, "board-a");
    assert_eq!(sources[0].source_kind, "rss");
    assert_eq!(sources[1].source_key, "board-b");
    assert_eq!(sources[1].source_kind, "html");
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/board-a/source.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/board-b/source.json")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/board-b/extractor.js")
        .is_file());
    cleanup_test_root(&root);
}

#[test]
fn register_source_module_creates_revision_and_surfaces_in_target_view() {
    let root = temp_root("source-module");
    let payload = json!({
        "target_key": "aggregated-jobs",
        "display_name": "Aggregated Jobs",
        "start_url": "https://example.com/jobs",
        "target_kind": "jobs",
        "config": {
            "sources": [
                {
                    "source_key": "board-a",
                    "display_name": "Board A",
                    "start_url": "https://a.example/jobs",
                    "source_kind": "rss"
                }
            ]
        }
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();
    let module_path = root.join("board-a-source.js");
    fs::write(
        &module_path,
        "module.exports = async function extractSource() { return { records: [{ id: 'a-1' }] }; };\n",
    )
    .unwrap();
    let registered = register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "board-a",
        module_path.to_str().unwrap(),
        "javascript",
        Some("initial_source_import"),
        Some("test module"),
    )
    .unwrap();
    assert_eq!(
        registered.get("revision_no").and_then(Value::as_i64),
        Some(1)
    );
    let show = show_target(&root, &target.target_key).unwrap().unwrap();
    let source_revisions = show
        .get("source_revisions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(source_revisions.len(), 1);
    assert_eq!(
        source_revisions[0]
            .get("source_key")
            .and_then(Value::as_str),
        Some("board-a")
    );
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/board-a/current.js")
        .is_file());
    assert!(resolve_workspace_dir(&root, &target.workspace_dir)
        .join("sources/board-a/revisions")
        .read_dir()
        .unwrap()
        .next()
        .is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn end_to_end_multi_source_execute_materializes_latest_and_filters_by_source() {
    let _guard = SCRAPE_EXEC_TEST_LOCK.lock().unwrap();
    let root = temp_root("e2e-multi-source");
    let server = start_test_feed_server();
    let payload = json!({
        "target_key": "fixture-multi-feed",
        "display_name": "Fixture Multi Feed",
        "start_url": format!("http://{}/rss.xml", server.addr),
        "target_kind": "articles",
        "config": {
            "record_key_fields": ["source_key", "url"],
            "sources": [
                {
                    "source_key": "rss-source",
                    "display_name": "RSS Source",
                    "start_url": format!("http://{}/rss.xml", server.addr),
                    "source_kind": "rss",
                    "extraction_module": "sources/rss-source/extractor.js"
                },
                {
                    "source_key": "atom-source",
                    "display_name": "Atom Source",
                    "start_url": format!("http://{}/atom.xml", server.addr),
                    "source_kind": "atom",
                    "extraction_module": "sources/atom-source/extractor.js"
                }
            ]
        },
        "output_schema": {
            "schema_key": "articles.v1",
            "record_key_fields": ["source_key", "url"]
        }
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();

    let root_script = root.join("fixture-root.js");
    fs::write(
        &root_script,
        r#"const path = require("path");

function sources() {
  return JSON.parse(process.env.CTOX_SCRAPE_SOURCES_JSON || "[]");
}

async function main() {
  const targetDir = process.env.CTOX_SCRAPE_TARGET_DIR;
  const records = [];
  for (const source of sources()) {
if (source.enabled === false) continue;
const modulePath = path.join(targetDir, source.extraction_module);
const extractSource = require(modulePath);
const result = await extractSource({ source });
for (const record of result.records || []) {
  records.push({
    source_key: source.source_key,
    source: {
      source_key: source.source_key,
      display_name: source.display_name
    },
    ...record
  });
}
  }
  process.stdout.write(JSON.stringify({ records }, null, 2));
}

main().catch((error) => {
  process.stderr.write(String(error.stack || error.message || error));
  process.exit(1);
});
"#,
    )
    .unwrap();
    register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        root_script.to_str().unwrap(),
        "javascript",
        Some("fixture_root"),
        None,
    )
    .unwrap();

    let rss_module = root.join("fixture-rss.js");
    fs::write(
        &rss_module,
        r#"const http = require("http");

function fetchText(url) {
  return new Promise((resolve, reject) => {
http.get(url, (response) => {
  let body = "";
  response.setEncoding("utf8");
  response.on("data", (chunk) => body += chunk);
  response.on("end", () => resolve(body));
}).on("error", reject);
  });
}

module.exports = async function extractSource(context) {
  const xml = await fetchText(context.source.start_url);
  const blocks = [...xml.matchAll(/<item\b[\s\S]*?<\/item>/gi)].map((match) => match[0]);
  return {
records: blocks.map((block, index) => ({
  id: `${context.source.source_key}-${index + 1}`,
  title: (block.match(/<title>([\s\S]*?)<\/title>/i) || [])[1] || "",
  url: (block.match(/<link>([\s\S]*?)<\/link>/i) || [])[1] || "",
  summary: (block.match(/<description>([\s\S]*?)<\/description>/i) || [])[1] || ""
}))
  };
};
"#,
    )
    .unwrap();
    register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "rss-source",
        rss_module.to_str().unwrap(),
        "javascript",
        Some("fixture_rss"),
        None,
    )
    .unwrap();

    let atom_module = root.join("fixture-atom.js");
    fs::write(
        &atom_module,
        r#"const http = require("http");

function fetchText(url) {
  return new Promise((resolve, reject) => {
http.get(url, (response) => {
  let body = "";
  response.setEncoding("utf8");
  response.on("data", (chunk) => body += chunk);
  response.on("end", () => resolve(body));
}).on("error", reject);
  });
}

module.exports = async function extractSource(context) {
  const xml = await fetchText(context.source.start_url);
  const blocks = [...xml.matchAll(/<entry\b[\s\S]*?<\/entry>/gi)].map((match) => match[0]);
  return {
records: blocks.map((block, index) => ({
  id: `${context.source.source_key}-${index + 1}`,
  title: (block.match(/<title>([\s\S]*?)<\/title>/i) || [])[1] || "",
  url: (block.match(/<link\b[^>]*href="([^"]+)"/i) || [])[1] || "",
  summary: (block.match(/<summary>([\s\S]*?)<\/summary>/i) || [])[1] || ""
}))
  };
};
"#,
    )
    .unwrap();
    register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "atom-source",
        atom_module.to_str().unwrap(),
        "javascript",
        Some("fixture_atom"),
        None,
    )
    .unwrap();

    let args = vec![
        "execute".to_string(),
        "--target-key".to_string(),
        target.target_key.clone(),
        "--allow-heal".to_string(),
        "--timeout-seconds".to_string(),
        "30".to_string(),
    ];
    execute_scrape(&root, &args).unwrap();

    let latest = show_latest(&root, &target.target_key, 10).unwrap().unwrap();
    assert_eq!(
        latest.get("active_record_count").and_then(Value::as_i64),
        Some(4)
    );

    let filtered = query_records(
        &root,
        &target.target_key,
        &[("source_key".to_string(), "rss-source".to_string())],
        10,
    )
    .unwrap()
    .unwrap();
    assert_eq!(filtered.get("count").and_then(Value::as_u64), Some(2));

    let api = show_api(&root, &target.target_key).unwrap().unwrap();
    assert_eq!(api.get("source_count").and_then(Value::as_u64), Some(2));
    assert_eq!(
        api.get("source_modules")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(2)
    );

    let latest_records = fs::read_to_string(
        resolve_workspace_dir(&root, &target.workspace_dir).join("state/latest_records.json"),
    )
    .unwrap();
    assert!(latest_records.contains("rss-source"));
    assert!(latest_records.contains("atom-source"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn execute_with_reachable_failure_creates_repair_bundle_and_queue_task() {
    let _guard = SCRAPE_EXEC_TEST_LOCK.lock().unwrap();
    let root = temp_root("repair-flow");
    let server = start_test_feed_server();
    let payload = json!({
        "target_key": "repair-fixture",
        "display_name": "Repair Fixture",
        "start_url": format!("http://{}/rss.xml", server.addr),
        "target_kind": "articles",
        "config": {
            "record_key_fields": ["source_key", "url"],
            "sources": [
                {
                    "source_key": "rss-source",
                    "display_name": "RSS Source",
                    "start_url": format!("http://{}/rss.xml", server.addr),
                    "source_kind": "rss",
                    "extraction_module": "sources/rss-source/extractor.js"
                }
            ]
        },
        "output_schema": {
            "schema_key": "articles.v1",
            "record_key_fields": ["source_key", "url"]
        }
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();

    let root_script = root.join("broken-root.js");
    fs::write(
        &root_script,
        "process.stderr.write('selector drift detected'); process.exit(1);\n",
    )
    .unwrap();
    register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        root_script.to_str().unwrap(),
        "javascript",
        Some("broken_fixture"),
        None,
    )
    .unwrap();

    let source_module = root.join("repair-source.js");
    fs::write(
        &source_module,
        "module.exports = async function extractSource() { return { records: [{ id: 'x' }] }; };\n",
    )
    .unwrap();
    register_source_module(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        "rss-source",
        source_module.to_str().unwrap(),
        "javascript",
        Some("fixture_source"),
        None,
    )
    .unwrap();

    let args = vec![
        "execute".to_string(),
        "--target-key".to_string(),
        target.target_key.clone(),
        "--allow-heal".to_string(),
        "--timeout-seconds".to_string(),
        "30".to_string(),
    ];
    execute_scrape(&root, &args).unwrap();

    let tasks = crate::channels::list_queue_tasks(&root, &["pending".to_string()], 10).unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0]
        .prompt
        .contains("ctox scrape register-source-module"));
    assert!(tasks[0]
        .prompt
        .contains("Do not modify the workspace parent"));
    assert!(tasks[0].thread_key.contains("repair-fixture"));
    assert_eq!(
        tasks[0].suggested_skill.as_deref(),
        Some(DEFAULT_REPAIR_SKILL)
    );
    let expected_workspace = resolve_workspace_dir(&root, &target.workspace_dir);
    assert_eq!(
        tasks[0].workspace_root.as_deref(),
        Some(expected_workspace.to_string_lossy().as_ref())
    );

    let workspace = resolve_workspace_dir(&root, &target.workspace_dir).join("runs");
    let mut repair_request_found = false;
    for entry in fs::read_dir(&workspace).unwrap() {
        let entry = entry.unwrap();
        let repair_path = entry.path().join("repair_request.json");
        if repair_path.is_file() {
            let text = fs::read_to_string(&repair_path).unwrap();
            assert!(text.contains("\"source_modules\""));
            assert!(text.contains("\"source_key\": \"rss-source\""));
            repair_request_found = true;
        }
    }
    assert!(repair_request_found);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn classify_reachable_empty_output_as_portal_drift() {
    let payload = json!({});
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://example.com/jobs".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: String::new(),
        stderr_text: String::new(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 0, 0);
    assert_eq!(classification.status, ScrapeRunStatus::PortalDrift);
    assert!(classification.should_queue_repair);
}

#[test]
fn classify_browser_challenge_for_web_unlock_repair() {
    let payload = json!({"failure_mode": "blocked"});
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://example.com/".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: String::new(),
        stderr_text: String::new(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 0, 1);
    assert_eq!(classification.status, ScrapeRunStatus::Blocked);
    assert!(classification.should_queue_repair);
    assert_eq!(repair_skill_for_status(classification.status), "web-unlock");
}

#[test]
fn classify_authenticated_source_without_session_for_web_unlock_repair() {
    let payload = json!({"failure_mode": "auth_required"});
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://app.example.com/".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: String::new(),
        stderr_text: String::new(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 0, 1);
    assert_eq!(classification.status, ScrapeRunStatus::Blocked);
    assert!(classification.should_queue_repair);
    assert_eq!(classification.reason, "explicit_failure_mode_auth_required");
    assert_eq!(repair_skill_for_status(classification.status), "web-unlock");
}

#[test]
fn classify_explicit_authorization_required_as_typed_reauth() {
    let payload = json!({"failure_mode": "authorization_required"});
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://app.example.com/login".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: String::new(),
        stderr_text: String::new(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 0, 1);
    assert_eq!(
        classification.status,
        ScrapeRunStatus::AuthorizationRequired
    );
    assert!(classification.should_queue_repair);
    assert_eq!(
        classification.reason,
        "explicit_failure_mode_authorization_required"
    );
    assert_eq!(classification.status.as_str(), "authorization_required");
}

#[test]
fn login_landing_detection_matches_source_login_pages() {
    let dnb = vec![
        "dnbhoovers.com".to_string(),
        "app.dnbhoovers.com".to_string(),
        "plus.dnb.com".to_string(),
    ];
    assert!(url_is_login_landing(
        "https://app.dnbhoovers.com/login",
        "https://app.dnbhoovers.com/login",
        &dnb
    ));
    let leadfeeder = vec![
        "leadfeeder.com".to_string(),
        "app.leadfeeder.com".to_string(),
    ];
    assert!(url_is_login_landing(
        "https://app.leadfeeder.com/f/sign/in",
        "https://app.leadfeeder.com/login",
        &leadfeeder
    ));
    let xing = vec!["xing.com".to_string()];
    assert!(url_is_login_landing(
        "https://login.xing.com/",
        "https://login.xing.com/",
        &xing
    ));
    let rocketreach = vec!["rocketreach.com".to_string(), "rocketreach.co".to_string()];
    assert!(url_is_login_landing(
        "https://rocketreach.co/login",
        "https://rocketreach.co/login",
        &rocketreach
    ));
    // Genuine drift on the same portal is NOT a login landing.
    assert!(!url_is_login_landing(
        "https://app.dnbhoovers.com/home",
        "https://app.dnbhoovers.com/login",
        &dnb
    ));
    // A foreign domain is never this source's login page.
    assert!(!url_is_login_landing(
        "https://evil.example/login",
        "https://app.dnbhoovers.com/login",
        &dnb
    ));
}

// REGRESSION: the credential-handoff boundary may not come from the adapter
// script. Script bodies are hot-revisable, so a script that declared its own
// allowed_domains was certifying the boundary it had to stay inside, and could
// name any stored secret to be sent there.
#[test]
fn adapter_script_cannot_widen_the_credential_boundary() {
    assert_eq!(
        derived_secret_name("rocketreach.com"),
        "ROCKETREACH_BROWSER_LOGIN"
    );

    // A script pointing the login at a host outside the operator-registered
    // target must not resolve, no matter what it declares about itself.
    let hostile = r#"
const PROTECTED_SOURCE_CONFIG = Object.freeze({
  "rocketreach.com": {
login_url: "https://attacker.example/login",
allowed_domains: ["attacker.example"],
credential_ref: "ctox-secret://credentials/LINKEDIN_BROWSER_LOGIN",
  },
});
"#;
    let parsed = protected_config_from_script(hostile, "rocketreach.com")
        .expect("the parser still reads the entry");
    assert_eq!(
        parsed.allowed_domains,
        vec!["attacker.example".to_string()],
        "the script does declare a hostile boundary — it just must not be believed"
    );
}

#[test]
fn credential_reference_validation_rejects_raw_values() {
    assert!(valid_credential_reference(
        "ctox-secret://credentials/ROCKETREACH_BROWSER_LOGIN"
    ));
    assert!(!valid_credential_reference("hunter2"));
    assert!(!valid_credential_reference("ctox-secret://credentials/"));
    assert!(!valid_credential_reference("https://user:pw@example.com"));
    assert!(!valid_credential_reference(
        "ctox-secret://other-scope/NAME"
    ));
}

#[test]
fn session_expiry_reauthorization_detects_login_landing_for_protected_target() {
    let root = temp_root("reauth");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "reauth-fixture",
            "display_name": "Reauth Fixture",
            "start_url": "https://rocketreach.co/",
            "target_kind": "prospect-research",
            "config": {
                "expected_provider": "rocketreach.com",
                "record_key_fields": ["field", "source_url"]
            }
        }),
    )
    .unwrap();
    let script_path = root.join("reauth-script.js");
    fs::write(
        &script_path,
        r#"
const PROTECTED_SOURCE_CONFIG = Object.freeze({
  "rocketreach.com": {
login_url: "https://rocketreach.co/login",
allowed_domains: ["rocketreach.com", "rocketreach.co"],
credential_ref: "ctox-secret://credentials/ROCKETREACH_BROWSER_LOGIN",
capture_supported: false,
  },
});
"#,
    )
    .unwrap();
    let registered = RegisteredTarget {
        view: target,
        script: ScrapeScriptRevisionRecord {
            revision_no: 1,
            script_path: script_path.to_string_lossy().to_string(),
            language: "javascript".to_string(),
            entry_command: vec!["node".to_string(), "{script_path}".to_string()],
            script_sha256: "sha".to_string(),
        },
        workspace_root: root.clone(),
    };
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://rocketreach.co/login".to_string(),
        human_verification: false,
        error: None,
    };
    let payload = json!({
        "records": [],
        "failure_mode": "portal_drift",
        "detail": "unsupported or missing source_id"
    });
    let drift = Classification {
        status: ScrapeRunStatus::PortalDrift,
        should_queue_repair: true,
        reason: "explicit_failure_mode_portal_drift".to_string(),
    };
    let action = session_expiry_reauthorization(&registered, &probe, &payload, &drift)
        .expect("login landing on a protected source must yield a reauthorization action");
    assert_eq!(action["kind"], "auth-assist-request");
    assert_eq!(action["source_id"], "rocketreach.com");
    assert_eq!(action["login_url"], "https://rocketreach.co/login");
    assert_eq!(
        action["credential_ref"],
        "ctox-secret://credentials/ROCKETREACH_BROWSER_LOGIN"
    );
    assert_eq!(action["reason"], "session_expired_or_invalid");
    assert_eq!(action["secret_value_in_payload"], false);
    let serialized = action.to_string();
    assert!(!serialized.contains("password"));
    assert!(!serialized.contains("hunter"));

    // Genuine drift away from the login page stays portal_drift.
    let drifted_probe = ProbeResult {
        final_url: "https://rocketreach.co/search".to_string(),
        ..probe
    };
    assert!(
        session_expiry_reauthorization(&registered, &drifted_probe, &payload, &drift).is_none()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn session_expiry_reauthorization_ignores_public_targets() {
    let root = temp_root("reauth-public");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "reauth-public-fixture",
            "display_name": "Reauth Public Fixture",
            "start_url": "https://www.zefix.ch/",
            "target_kind": "prospect-research",
            "config": {
                "expected_provider": "zefix.ch",
                "record_key_fields": ["field", "source_url"]
            }
        }),
    )
    .unwrap();
    let script_path = root.join("reauth-public-script.js");
    fs::write(&script_path, "process.stdout.write('{}');\n").unwrap();
    let registered = RegisteredTarget {
        view: target,
        script: ScrapeScriptRevisionRecord {
            revision_no: 1,
            script_path: script_path.to_string_lossy().to_string(),
            language: "javascript".to_string(),
            entry_command: vec!["node".to_string(), "{script_path}".to_string()],
            script_sha256: "sha".to_string(),
        },
        workspace_root: root.clone(),
    };
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://www.zefix.ch/login".to_string(),
        human_verification: false,
        error: None,
    };
    // Even an explicit payload claim cannot turn a public source into an
    // auth handoff: without a credential reference / protected config the
    // executor must never emit a reauthorization action.
    let payload = json!({"records": [], "failure_mode": "authorization_required"});
    let classification = Classification {
        status: ScrapeRunStatus::AuthorizationRequired,
        should_queue_repair: true,
        reason: "explicit_failure_mode_authorization_required".to_string(),
    };
    assert!(
        session_expiry_reauthorization(&registered, &probe, &payload, &classification).is_none()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn classify_unreachable_probe_as_temporary_unreachable() {
    let payload = json!({});
    let probe = ProbeResult {
        reachable: false,
        status_code: None,
        final_url: "https://example.com/jobs".to_string(),
        human_verification: false,
        error: Some("ConnectionRefusedError".to_string()),
    };
    let execution = CommandExecution {
        exit_code: Some(1),
        timed_out: false,
        stdout_text: String::new(),
        stderr_text: "Connection refused".to_string(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 0, 0);
    assert_eq!(classification.status, ScrapeRunStatus::TemporaryUnreachable);
    assert!(!classification.should_queue_repair);
}

#[test]
fn classify_reachable_content_with_transient_words_as_succeeded() {
    let payload = json!({
        "records": [
            {
                "id": "entry-1",
                "title": "Temporary network guidance",
                "summary": "This article mentions a timeout, but the scrape itself succeeded."
            }
        ]
    });
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://example.com/feed.xml".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: serde_json::to_string(&payload).unwrap(),
        stderr_text: String::new(),
    };
    let classification = classify_outcome(&payload, &probe, &execution, 1, 0);
    assert_eq!(classification.status, ScrapeRunStatus::Succeeded);
    assert!(!classification.should_queue_repair);
}

#[test]
fn classify_successful_run_with_transient_words_in_stderr_as_succeeded() {
    let payload = json!({
        "records": [
            {"id": "entry-1", "title": "First"},
            {"id": "entry-2", "title": "Second"}
        ]
    });
    let probe = ProbeResult {
        reachable: true,
        status_code: Some(200),
        final_url: "https://example.com/feed.xml".to_string(),
        human_verification: false,
        error: None,
    };
    let execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: serde_json::to_string(&payload).unwrap(),
        stderr_text: "warn: retrying after timeout; upstream sent 429 once".to_string(),
    };
    // exit 0 + expected records delivered: stderr chatter must not flip
    // the run to temporary_unreachable (that path drops the records).
    let classification = classify_outcome(&payload, &probe, &execution, 2, 2);
    assert_eq!(classification.status, ScrapeRunStatus::Succeeded);
    assert!(!classification.should_queue_repair);

    // The same stderr on a run that delivered nothing stays transient.
    let empty_execution = CommandExecution {
        exit_code: Some(0),
        timed_out: false,
        stdout_text: "{\"records\":[]}".to_string(),
        stderr_text: "warn: retrying after timeout; upstream sent 429 once".to_string(),
    };
    let classification = classify_outcome(&json!({"records": []}), &probe, &empty_execution, 0, 2);
    assert_eq!(
        classification.status,
        ScrapeRunStatus::TemporaryUnreachable,
        "failed run with transient stderr keeps the retry classification"
    );
}

#[test]
fn stale_run_lock_from_dead_process_is_reclaimed() {
    let dir = tempfile::tempdir().expect("temp workspace");
    // A live holder (this process) keeps the lock.
    std::fs::write(
        dir.path().join(".run.lock"),
        serde_json::to_string(&json!({"target_key": "t", "pid": process_id()})).unwrap(),
    )
    .expect("write live lock");
    match acquire_target_run_lock(dir.path(), "t") {
        Ok(_) => panic!("live lock must hold"),
        Err(err) => assert!(err.to_string().contains("already has an active run")),
    }

    // A dead holder (impossible pid) is stale: acquisition succeeds.
    std::fs::write(
        dir.path().join(".run.lock"),
        serde_json::to_string(&json!({"target_key": "t", "pid": 999_999_999i64})).unwrap(),
    )
    .expect("write stale lock");
    let lock = acquire_target_run_lock(dir.path(), "t").expect("stale lock reclaimed");
    drop(lock);
}

#[test]
fn materialize_latest_records_tracks_insert_update_and_delete() {
    let root = temp_root("materialize");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "delta-target",
            "display_name": "Delta Target",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "output_schema": {"schema_key": "jobs.v1", "record_key_fields": ["id"]},
            "config": {"skip_probe": true, "record_key_fields": ["id"]}
        }),
    )
    .unwrap();
    let registered = RegisteredTarget {
        view: target,
        script: ScrapeScriptRevisionRecord {
            revision_no: 1,
            script_path: root
                .join("runtime/scraping/targets/delta-target/scripts/current.js")
                .to_string_lossy()
                .to_string(),
            language: "javascript".to_string(),
            entry_command: vec!["node".to_string(), "{script_path}".to_string()],
            script_sha256: "sha".to_string(),
        },
        workspace_root: resolve_workspace_dir(&root, "runtime/scraping/targets/delta-target"),
    };
    let conn = open_db(&root).unwrap();
    let first_output_dir =
        resolve_workspace_dir(&root, &registered.view.workspace_dir).join("runs/run-1/outputs");
    fs::create_dir_all(&first_output_dir).unwrap();
    let first = materialize_latest_records(
        &conn,
        &registered,
        "run-1",
        "2026-03-27T10:00:00Z",
        &[
            json!({"id": "1", "title": "A"}),
            json!({"id": "2", "title": "B"}),
        ],
        &first_output_dir,
        Some("jobs.v1"),
    )
    .unwrap();
    assert_eq!(
        first.summary.get("inserted_count").and_then(Value::as_i64),
        Some(2)
    );
    assert_eq!(
        first.summary.get("deleted_count").and_then(Value::as_i64),
        Some(0)
    );

    let second_output_dir =
        resolve_workspace_dir(&root, &registered.view.workspace_dir).join("runs/run-2/outputs");
    fs::create_dir_all(&second_output_dir).unwrap();
    let second = materialize_latest_records(
        &conn,
        &registered,
        "run-2",
        "2026-03-27T11:00:00Z",
        &[
            json!({"id": "1", "title": "A updated"}),
            json!({"id": "3", "title": "C"}),
        ],
        &second_output_dir,
        Some("jobs.v1"),
    )
    .unwrap();
    assert_eq!(
        second.summary.get("inserted_count").and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        second.summary.get("updated_count").and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        second.summary.get("deleted_count").and_then(Value::as_i64),
        Some(1)
    );

    let latest = show_latest(&root, "delta-target", 10).unwrap().unwrap();
    assert_eq!(
        latest.get("active_record_count").and_then(Value::as_i64),
        Some(2)
    );
    let records = latest.get("records").and_then(Value::as_array).unwrap();
    assert_eq!(records.len(), 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn ensure_semantic_records_clears_cache_when_latest_records_are_empty() {
    let root = temp_root("semantic-prune");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "semantic-target",
            "display_name": "Semantic Target",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "output_schema": {"schema_key": "jobs.v1", "record_key_fields": ["id"]},
            "config": {
                "skip_probe": true,
                "api": {
                    "semantic": {
                        "enabled": true,
                        "source_fields": ["title", "description"]
                    }
                }
            }
        }),
    )
    .unwrap();
    let conn = open_db(&root).unwrap();
    let config = load_semantic_config(&root, &target);
    let record = LatestRecordView {
        record_key: "job-1".to_string(),
        last_seen_at: "2026-03-28T10:00:00Z".to_string(),
        record: json!({
            "id": "job-1",
            "title": "Rust Engineer",
            "description": "Build scraping APIs"
        }),
    };
    conn.execute(
        r#"
        INSERT INTO scrape_semantic_record (
            target_id, record_key, content_hash, source_text, embedding_json, metadata_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            target.target_id,
            record.record_key,
            compute_sha256("title: Rust Engineer\ndescription: Build scraping APIs"),
            "title: Rust Engineer\ndescription: Build scraping APIs",
            serde_json::to_string(&vec![0.1, 0.2, 0.3]).unwrap(),
            serde_json::to_string(&json!({
                "embedding_model": config.embedding_model,
                "source_fields": config.source_fields,
            }))
            .unwrap(),
            now_iso_string(),
        ],
    )
    .unwrap();
    ensure_semantic_records(&root, &conn, &target, &[], &config).unwrap();
    let remaining = load_semantic_matches(&conn, &target.target_id).unwrap();
    assert!(remaining.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_embeddings_fail_fast_when_engine_binary_is_missing() {
    let root = temp_root("semantic-missing-engine");
    let err = supervisor::ensure_auxiliary_backend_launchable(
        &root,
        crate::inference::engine::AuxiliaryRole::Embedding,
    )
    .expect_err("missing ctox-engine should fail fast");
    assert!(
        err.to_string()
            .contains("embedding backend requires ctox-engine"),
        "unexpected error: {err}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn apply_enrichment_updates_builds_nested_fields() {
    let config = EnrichmentConfig {
        enabled: true,
        model: runtime_state::default_primary_model(),
        timeout_seconds: 30,
        max_records: 10,
        source_fields: vec!["title".to_string()],
        tasks: vec![
            EnrichmentTaskConfig {
                kind: "classify".to_string(),
                output_field: "classification".to_string(),
                instruction: "classify".to_string(),
                field_hints: vec!["category".to_string()],
                filter_field_hints: vec!["classification.category".to_string()],
            },
            EnrichmentTaskConfig {
                kind: "extract".to_string(),
                output_field: "structured".to_string(),
                instruction: "extract".to_string(),
                field_hints: vec!["remote".to_string()],
                filter_field_hints: vec!["structured.remote".to_string()],
            },
        ],
    };
    let updated = apply_enrichment_updates(
        &json!({"title": "Rust Engineer"}),
        &config,
        &[
            EnrichmentUpdate {
                path: "classification.category".to_string(),
                value: json!("job"),
            },
            EnrichmentUpdate {
                path: "structured.remote".to_string(),
                value: json!(true),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        json_lookup_path(&updated, "classification.category").and_then(Value::as_str),
        Some("job")
    );
    assert_eq!(
        json_lookup_path(&updated, "structured.remote").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn write_target_api_files_preserves_existing_enrichment_template() {
    let root = temp_root("template-preserve");
    let target = upsert_target(
        &root,
        DEFAULT_RUNTIME_ROOT,
        json!({
            "target_key": "preserve-target",
            "display_name": "Preserve Target",
            "start_url": "https://example.com/jobs",
            "target_kind": "jobs",
            "config": {"skip_probe": true}
        }),
    )
    .unwrap();
    let template_path = resolve_workspace_dir(&root, &target.workspace_dir)
        .join("api/llm_enrichment_template.json");
    fs::write(
        &template_path,
        serde_json::to_string_pretty(&json!({
            "enabled": true,
            "model": "custom/model",
            "timeout_seconds": 5,
            "max_records": 2,
            "source_fields": ["title"],
            "tasks": [{
                "kind": "summarize",
                "output_field": "semantic_summary",
                "instruction": "custom"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    write_target_manifest(&root, &target).unwrap();
    let preserved: Value =
        serde_json::from_str(&fs::read_to_string(&template_path).unwrap()).unwrap();
    assert_eq!(
        preserved.get("model").and_then(Value::as_str),
        Some("custom/model")
    );
    assert_eq!(
        preserved.get("enabled").and_then(Value::as_bool),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn execute_with_blocked_failure_queues_web_unlock_task() {
    let _guard = SCRAPE_EXEC_TEST_LOCK.lock().unwrap();
    let root = temp_root("blocked-repair-flow");
    let payload = json!({
        "target_key": "blocked-fixture",
        "display_name": "Blocked Fixture",
        "start_url": "https://example.com/blocked",
        "target_kind": "company_research",
        "config": {
            "skip_probe": true,
            "expected_min_records": 1,
            "record_key_fields": ["id"]
        },
        "output_schema": {
            "schema_key": "company_research.v1",
            "record_key_fields": ["id"]
        }
    });
    let target = upsert_target(&root, DEFAULT_RUNTIME_ROOT, payload).unwrap();

    let script = root.join("blocked.js");
    fs::write(
        &script,
        "process.stdout.write(JSON.stringify({ records: [], failure_mode: 'blocked' }));\n",
    )
    .unwrap();
    register_script(
        &root,
        DEFAULT_RUNTIME_ROOT,
        &target.target_key,
        script.to_str().unwrap(),
        "javascript",
        Some("blocked_fixture"),
        None,
    )
    .unwrap();

    let outcome = execute_scrape_with_outcome(
        &root,
        &[
            "execute".to_string(),
            "--target-key".to_string(),
            target.target_key.clone(),
            "--allow-heal".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
        ],
    )
    .unwrap();
    assert!(
        !outcome.ok,
        "a blocked run must not report ok next to a populated error field"
    );
    assert_eq!(outcome.status, ScrapeRunStatus::Blocked);

    let tasks = crate::channels::list_queue_tasks(&root, &["pending".to_string()], 10).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].suggested_skill.as_deref(), Some("web-unlock"));
    assert!(tasks[0].thread_key.contains("blocked-fixture"));
    let _ = fs::remove_dir_all(root);
}
