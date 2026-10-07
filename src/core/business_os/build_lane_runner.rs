// Origin: CTOX
// License: AGPL-3.0-only

//! Transport-neutral Linux build lane script planner. The caller authorizes the
//! registered computer, prepares source and installs this script via pinned SSH.
//! Paths beneath the lane must be owned exclusively by the lane service account.

use super::computer_capabilities::{validate_capabilities, BuildCapability, ComputerCapability};

#[derive(Debug, Clone)]
pub struct BuildLanePlan {
    pub run_dir: String,
    pub source_dir: String,
    pub target_dir: String,
    pub script: String,
}

fn quote(value: &str) -> anyhow::Result<String> {
    anyhow::ensure!(!value.contains('\0'), "shell argument contains NUL");
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn identifier(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid opaque build identifier"
    );
    Ok(())
}

/// Render a bounded job and detached launcher without interpreting command args.
/// `timeout_seconds` is explicit operator policy, capped at 24 hours. Worker
/// environment declares the grant's cap; callers must reject command flags that
/// override it. This is capacity admission, not a sandbox for untrusted programs.
pub fn plan(
    grant: &BuildCapability,
    owner_id: &str,
    task_id: &str,
    run_id: &str,
    source_id: &str,
    args: &[String],
    timeout_seconds: u32,
) -> anyhow::Result<BuildLanePlan> {
    validate_capabilities(&mut vec![ComputerCapability::Build(grant.clone())], false)?;
    for id in [owner_id, task_id, run_id, source_id] {
        identifier(id)?;
    }
    anyhow::ensure!(!args.is_empty() && !args[0].is_empty(), "missing command");
    anyhow::ensure!(
        (1..=86400).contains(&timeout_seconds),
        "invalid job timeout"
    );
    let root = grant.lane_root.as_str();
    let run_dir = format!("{root}/runs/{task_id}/{run_id}");
    let source_dir = format!("{root}/sources/{source_id}");
    let target_dir = format!("{root}/target/{source_id}");
    let command = args
        .iter()
        .map(|arg| quote(arg))
        .collect::<anyhow::Result<Vec<_>>>()?
        .join(" ");
    let mut delimiter = "CTOX_JOB".to_owned();
    while command.contains(&delimiter) || root.contains(&delimiter) {
        delimiter.push_str("_END");
    }
    let script = format!(
        r#"#!/bin/bash
set -eu
umask 077
root={root}
run={run}
source={source}
target={target}
mkdir -p -- "$root/runs/{task_id}" "$root/run" "$root/wait" "$target"
# An existing run is never relaunched or overwritten.
mkdir -- "$run" || exit 73
cat > "$run/job.sh" <<'{delimiter}'
#!/bin/bash
set -eu
umask 077
run={run}
root={root}
source={source}
target={target}
finish() {{
    rc=$?
    trap - EXIT
    exec 9>&-
    [[ -z "${{ticket:-}}" ]] || rm -f -- "$ticket"
    timeout --kill-after=2s 10s rm -rf -- "$run/tmp" || true
    date -u +%FT%TZ > "$run/finished.tmp"
    mv -- "$run/finished.tmp" "$run/finished"
    printf '%s\n' "$rc" > "$run/exit.tmp"
    mv -- "$run/exit.tmp" "$run/exit"
}}
trap finish EXIT
date -u +%FT%TZ > "$run/started"
priority=$(awk -v owner={owner} '$1 == owner {{ print $2; exit }}' "$root/priorities" 2>/dev/null || true)
[[ "$priority" =~ ^[012]$ ]] || priority=2
ticket="$root/wait/$priority-$(date +%s%N)-native-{task_id}-{run_id}"
touch -- "$ticket"
deadline=$((SECONDS + {timeout}))
leased=0
while [[ "$leased" == 0 ]]; do
    first=$(LC_ALL=C ls -1 "$root/wait" | LC_ALL=C sort | head -1)
    if [[ "$first" == "${{ticket##*/}}" ]]; then
        for ((slot=1; slot<={slots}; slot++)); do
            exec 9>"$root/slot-$slot.lock"
            if flock -n 9; then leased=1; break; fi
            exec 9>&-
        done
    fi
    [[ "$leased" == 0 ]] || break
    (( SECONDS < deadline )) || exit 75
    sleep 0.25
done
remaining=$((deadline - SECONDS))
(( remaining > 0 )) || exit 75
rm -f -- "$ticket"
ticket=""
printf '%s\n' "$slot" > "$run/slot"
available=$(df -Pk -- "$root" | awk 'END {{ print $4 }}')
[[ "$available" =~ ^[0-9]+$ ]] || exit 74
(( available >= {floor} )) || exit 74
cd -- "$source"
export TMPDIR="$run/tmp"
mkdir -- "$TMPDIR"
export CARGO_TARGET_DIR="$target"
export CARGO_BUILD_JOBS={jobs} RUST_TEST_THREADS={jobs} CMAKE_BUILD_PARALLEL_LEVEL={jobs}
export MAKEFLAGS='-j{jobs}'
timeout --signal=TERM --kill-after=10s "$remaining"s {command}
{delimiter}
chmod 700 "$run/job.sh"
# setsid detaches the session; all descriptors are redirected before returning.
nohup setsid bash "$run/job.sh" </dev/null >"$run/log" 2>&1 &
printf '%s\n' "$!" > "$run/pid"
printf '%s\n' "$!" > "$root/run/native-{task_id}-{run_id}.pid"
printf '%s\n' "$run"
"#,
        owner = quote(owner_id)?,
        root = quote(root)?,
        run = quote(&run_dir)?,
        source = quote(&source_dir)?,
        target = quote(&target_dir)?,
        slots = grant.slots,
        jobs = grant.jobs,
        floor = u64::from(grant.disk_floor_gib) * 1024 * 1024,
        timeout = timeout_seconds,
    );
    Ok(BuildLanePlan {
        run_dir,
        source_dir,
        target_dir,
        script,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{Duration, Instant},
    };

    fn fixture() -> (tempfile::TempDir, BuildCapability) {
        let dir = tempfile::tempdir().unwrap();
        let grant = BuildCapability {
            ssh_endpoint_ref: "fixture".into(),
            slots: 1,
            jobs: 2,
            lane_root: dir
                .path()
                .join("lane ' $(touch INJECTED)")
                .to_str()
                .unwrap()
                .into(),
            disk_floor_gib: 1,
            toolchains: vec!["rust".into()],
        };
        (dir, grant)
    }

    fn launch(plan: &BuildLanePlan) {
        let mut child = Command::new("bash")
            .arg("-s")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(plan.script.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }

    fn wait_file(path: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(value) = fs::read_to_string(path) {
                return value;
            }
            assert!(Instant::now() < deadline, "missing {path}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn detached_job_preserves_literal_args_log_and_exit() {
        let (_dir, grant) = fixture();
        let hostile = "quote' $(touch INJECTED)\nCTOX_JOB\nlast";
        let args = vec!["bash".into(), "-c".into(),
            "printf '%s' \"$1\"; printf '\\nworkers=%s target=%s' \"$CARGO_BUILD_JOBS\" \"$CARGO_TARGET_DIR\"; exit 17".into(),
            "fixture".into(), hostile.into()];
        let plan = plan(&grant, "fixture", "task", "run", "source", &args, 10).unwrap();
        fs::create_dir_all(&plan.source_dir).unwrap();
        launch(&plan);
        assert_eq!(wait_file(&format!("{}/exit", plan.run_dir)).trim(), "17");
        let log = fs::read_to_string(format!("{}/log", plan.run_dir)).unwrap();
        assert!(log.starts_with(hostile));
        assert!(log.contains(&format!("workers=2 target={}", plan.target_dir)));
        assert!(!std::path::Path::new(&format!("{}/INJECTED", plan.source_dir)).exists());
        assert!(std::path::Path::new(&format!("{}/finished", plan.run_dir)).exists());
        let status = Command::new("bash")
            .arg("-c")
            .arg(&plan.script)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
    }

    #[test]
    fn actual_flock_contention_and_timeout_release_slot() {
        let (_dir, grant) = fixture();
        let first = plan(
            &grant,
            "fixture",
            "task",
            "first",
            "source",
            &["sleep".into(), "30".into()],
            3,
        )
        .unwrap();
        fs::create_dir_all(&first.source_dir).unwrap();
        launch(&first);
        wait_file(&format!("{}/slot", first.run_dir));
        let second = plan(
            &grant,
            "fixture",
            "task",
            "second",
            "source",
            &["true".into()],
            10,
        )
        .unwrap();
        launch(&second);
        assert_eq!(wait_file(&format!("{}/exit", second.run_dir)).trim(), "0");
        assert_eq!(wait_file(&format!("{}/slot", second.run_dir)).trim(), "1");
        assert_eq!(wait_file(&format!("{}/exit", first.run_dir)).trim(), "124");
        let third = plan(
            &grant,
            "fixture",
            "task",
            "third",
            "source",
            &["true".into()],
            10,
        )
        .unwrap();
        launch(&third);
        assert_eq!(wait_file(&format!("{}/exit", third.run_dir)).trim(), "0");
    }

    #[test]
    fn impossible_disk_floor_and_traversal_fail_closed() {
        let (_dir, mut grant) = fixture();
        grant.disk_floor_gib = u32::MAX;
        let job = plan(
            &grant,
            "fixture",
            "task",
            "floor",
            "source",
            &["true".into()],
            10,
        )
        .unwrap();
        fs::create_dir_all(&job.source_dir).unwrap();
        launch(&job);
        assert_eq!(wait_file(&format!("{}/exit", job.run_dir)).trim(), "74");
        assert!(plan(
            &grant,
            "fixture",
            "../task",
            "run",
            "source",
            &["true".into()],
            1
        )
        .is_err());
    }

    #[test]
    fn prototype_slot_namespace_is_shared_with_external_flock() {
        let (_dir, grant) = fixture();
        fs::create_dir_all(&grant.lane_root).unwrap();
        let marker = format!("{}/external-holder-ready", grant.lane_root);
        let mut holder = Command::new("bash")
            .arg("-c")
            .arg("exec 9>\"$1/slot-1.lock\"; flock 9; printf ready > \"$2\"; read -r release")
            .arg("fixture")
            .arg(&grant.lane_root)
            .arg(&marker)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        wait_file(&marker);
        let job = plan(
            &grant,
            "fixture",
            "task",
            "external",
            "source",
            &["true".into()],
            10,
        )
        .unwrap();
        fs::create_dir_all(&job.source_dir).unwrap();
        launch(&job);
        wait_file(&format!("{}/started", job.run_dir));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!std::path::Path::new(&format!("{}/exit", job.run_dir)).exists());
        use std::io::Write;
        holder
            .stdin
            .take()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        assert!(holder.wait().unwrap().success());
        assert_eq!(wait_file(&format!("{}/exit", job.run_dir)).trim(), "0");
        assert_eq!(wait_file(&format!("{}/slot", job.run_dir)).trim(), "1");
    }

    #[test]
    fn higher_priority_queue_waits_until_deadline_and_tmp_stays_on_lane() {
        let (_dir, grant) = fixture();
        fs::create_dir_all(format!("{}/wait", grant.lane_root)).unwrap();
        let queued = format!("{}/wait/0-0000000000000000000-prototype", grant.lane_root);
        fs::write(&queued, "").unwrap();
        let blocked = plan(
            &grant,
            "unknown",
            "task",
            "blocked",
            "source",
            &["true".into()],
            1,
        )
        .unwrap();
        fs::create_dir_all(&blocked.source_dir).unwrap();
        launch(&blocked);
        wait_file(&format!("{}/started", blocked.run_dir));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!std::path::Path::new(&format!("{}/exit", blocked.run_dir)).exists());
        assert_eq!(wait_file(&format!("{}/exit", blocked.run_dir)).trim(), "75");
        assert!(!std::path::Path::new(&format!("{}/slot", blocked.run_dir)).exists());
        fs::remove_file(queued).unwrap();
        let allowed = plan(
            &grant,
            "unknown",
            "task",
            "allowed",
            "source",
            &[
                "bash".into(),
                "-c".into(),
                "printf '%s' \"$TMPDIR\"; touch \"$TMPDIR/scratch\"".into(),
            ],
            10,
        )
        .unwrap();
        launch(&allowed);
        assert_eq!(wait_file(&format!("{}/exit", allowed.run_dir)).trim(), "0");
        assert_eq!(
            fs::read_to_string(format!("{}/log", allowed.run_dir)).unwrap(),
            format!("{}/tmp", allowed.run_dir)
        );
        assert!(!std::path::Path::new(&format!("{}/tmp", allowed.run_dir)).exists());
        assert_eq!(
            fs::read_dir(format!("{}/wait", grant.lane_root))
                .unwrap()
                .count(),
            0
        );
    }
}
