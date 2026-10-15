//! E20 escape tests for the `process.run` OS sandbox. Each confinement has a
//! mutation proof in `e20_mutations.py` (confinement off → this test red).
use super::*;

const APP: &str = "outbound-lead-generation";

fn allow(f: &Fixture, entries: Value) {
    crate::inference::runtime_env::set_runtime_env_value(
        f.root.path(),
        process::ALLOWLIST_KEY,
        &entries.to_string(),
    )
    .unwrap();
}
fn run(f: &Fixture, program: &str, script: &str, timeout_ms: u64) -> Value {
    f.call(
        &Host::new(APP, "staff"),
        "process",
        "run",
        json!({"program":program,"args":["-c",script],"timeout_ms":timeout_ms}),
    )
    .unwrap()
}
fn workspace(f: &Fixture) -> std::path::PathBuf {
    f.root.path().join("runtime/app-workspaces").join(APP)
}
fn sh_entry(limits: Value) -> Value {
    json!({"sh":{"program":"/bin/sh","env":{"PATH":"/usr/bin:/bin"},"limits":limits},
           "bash":{"program":"/bin/bash","env":{"PATH":"/usr/bin:/bin"}}})
}

#[test]
fn app_capabilities_e20_read_outside_workspace_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({})));
    let secrets = crate::paths::secrets_db(f.root.path());
    std::fs::create_dir_all(secrets.parent().unwrap()).unwrap();
    std::fs::write(&secrets, "E20-SECRET-STORE").unwrap();
    std::fs::write(f.root.path().join("install.toml"), "E20-CTOX-ROOT").unwrap();
    let sibling = f.root.path().join("runtime/app-workspaces/news");
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::write(sibling.join("data.txt"), "E20-OTHER-APP").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(elsewhere.path().join("host.txt"), "E20-HOST-FILE").unwrap();
    std::fs::create_dir_all(workspace(&f)).unwrap();
    std::fs::write(workspace(&f).join("own.txt"), "E20-OWN-WORKSPACE").unwrap();
    let script = format!(
        "cat own.txt; cat '{}' '{}' '{}' '{}' 2>/dev/null; ls \"$HOME\" /home /root /run 2>/dev/null; cat /etc/hostname >/dev/null && echo SYSTEM-READ",
        secrets.display(),
        f.root.path().join("install.toml").display(),
        sibling.join("data.txt").display(),
        elsewhere.path().join("host.txt").display(),
    );
    let out = run(&f, "sh", &script, 10_000);
    let stdout = out["stdout"].as_str().unwrap();
    assert!(
        stdout.contains("E20-OWN-WORKSPACE"),
        "workspace must stay readable: {out}"
    );
    assert!(
        stdout.contains("SYSTEM-READ"),
        "system trees must stay readable: {out}"
    );
    for marker in [
        "E20-SECRET-STORE",
        "E20-CTOX-ROOT",
        "E20-OTHER-APP",
        "E20-HOST-FILE",
    ] {
        assert!(
            !stdout.contains(marker),
            "E20 read escape: {marker} visible: {out}"
        );
    }
}

#[test]
fn app_capabilities_e20_write_outside_workspace_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({})));
    let elsewhere = tempfile::tempdir().unwrap();
    let targets = [
        f.root.path().join("escape-root.txt"),
        f.root.path().join("runtime/escape-state.txt"),
        elsewhere.path().join("escape-host.txt"),
    ];
    std::fs::create_dir_all(f.root.path().join("runtime")).unwrap();
    let mut script = String::from("echo ok > inside.txt; echo t > /tmp/scratch.txt; ");
    for target in &targets {
        script.push_str(&format!("echo x > '{}' 2>/dev/null; ", target.display()));
    }
    script.push_str("echo x > /etc/e20-escape 2>/dev/null; echo x > /e20-escape 2>/dev/null; true");
    let out = run(&f, "sh", &script, 10_000);
    assert_eq!(out["exit"], 0, "{out}");
    assert!(
        workspace(&f).join("inside.txt").is_file(),
        "workspace must stay writable: {out}"
    );
    for target in &targets {
        assert!(
            !target.exists(),
            "E20 write escape: {} was written",
            target.display()
        );
    }
}

#[test]
fn app_capabilities_e20_network_without_approval_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({})));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let out = run(
        &f,
        "bash",
        &format!("(exec 3<>/dev/tcp/127.0.0.1/{port}) 2>/dev/null && echo LOCAL-CONNECTED; (exec 3<>/dev/tcp/1.1.1.1/443) 2>/dev/null && echo NET-CONNECTED; true"),
        10_000,
    );
    let stdout = out["stdout"].as_str().unwrap();
    assert!(
        !stdout.contains("CONNECTED") && listener.accept().is_err(),
        "E20 network escape: {out}"
    );
    // Domain-restricted egress is not built: approval is unavailable, never open.
    allow(
        &f,
        json!({"web":{"program":"/bin/sh","net_domains":["example.com"]},
               "sh":{"program":"/bin/sh"}}),
    );
    let host = Host::new(APP, "staff");
    assert_eq!(
        code(f.call(
            &host,
            "process",
            "run",
            json!({"program":"web","args":["-c","true"]})
        )),
        ErrorCode::Unavailable
    );
    assert_eq!(
        code(f.call(
            &host,
            "process",
            "run",
            json!({"program":"sh","args":["-c","true"],"net_domains":["example.com"]})
        )),
        ErrorCode::Forbidden,
        "a call may name targets only when the entry allows it"
    );
}

#[test]
fn app_capabilities_e20_fork_bomb_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({"processes":24})));
    // Bounded binary fork bomb (254 subshells unconfined): every subshell
    // leaves a marker, spawns two more and then blocks on a FIFO that nobody
    // writes (a builtin redirection, no fork), so no process ever frees its
    // slot and the markers count processes alive at once. Ends by timeout.
    std::fs::create_dir_all(workspace(&f)).unwrap();
    let fifo =
        std::ffi::CString::new(workspace(&f).join("hold").to_string_lossy().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let bomb = "b(){ if [ \"$1\" -gt 0 ]; then (: > \"m.$2a\"; b $(($1-1)) \"$2a\") & (: > \"m.$2b\"; b $(($1-1)) \"$2b\") & fi; read x < hold; }; b 7 x 2>/dev/null";
    let out = run(&f, "sh", bomb, 4_000);
    assert_eq!(out["timed_out"], true, "{out}");
    let started = std::fs::read_dir(workspace(&f))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("m."))
        .count();
    assert!(started > 0, "the bomb never started: {out}");
    assert!(
        started <= 24,
        "E20 process limit escaped: {started} processes: {out}"
    );
}

#[test]
fn app_capabilities_e20_memory_limit_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({"memory_mb":64})));
    // tail -n 1 buffers the whole newline-free 300 MB stream.
    let out = run(
        &f,
        "sh",
        "head -c 300000000 /dev/zero | tail -n 1 | wc -c",
        30_000,
    );
    let stdout = out["stdout"].as_str().unwrap().trim();
    assert_ne!(stdout, "300000000", "E20 memory limit escaped: {out}");
    assert!(
        out["stderr"].as_str().unwrap().contains("memory"),
        "expected an allocation failure: {out}"
    );
    // CPU time is bounded independently of the wall-clock timeout.
    allow(&f, sh_entry(json!({"cpu_seconds":1})));
    let spin = run(&f, "sh", "while :; do :; done", 20_000);
    assert_eq!(
        spin["timed_out"], false,
        "CPU limit must end the run: {spin}"
    );
    assert!(spin["duration_ms"].as_u64().unwrap() < 10_000, "{spin}");
}

#[test]
fn app_capabilities_e20_unavailable_never_unconfined_contract() {
    let f = Fixture::new();
    allow(&f, sh_entry(json!({})));
    crate::inference::runtime_env::set_runtime_env_value(
        f.root.path(),
        sandbox::SANDBOX_KEY,
        r#"{"bwrap":"/nonexistent/bwrap","sandbox_exec":"/nonexistent/sandbox-exec"}"#,
    )
    .unwrap();
    let marker = workspace(&f).join("ran.txt");
    let error = f
        .call(
            &Host::new(APP, "staff"),
            "process",
            "run",
            json!({"program":"sh","args":["-c","echo x > ran.txt"]}),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.contains("sandbox"), "{}", error.message);
    assert!(!marker.exists(), "the program ran without the sandbox");
}

#[test]
fn app_capabilities_e20_untrusted_code_contract() {
    let f = Fixture::new();
    let host_pl = f.root.path().join("host.pl");
    std::fs::write(
        &host_pl,
        "my $c = do { local $/; <STDIN> }; eval $c; die $@ if $@;\n",
    )
    .unwrap();
    let secrets = crate::paths::secrets_db(f.root.path());
    std::fs::create_dir_all(secrets.parent().unwrap()).unwrap();
    std::fs::write(&secrets, "E20-SECRET-STORE").unwrap();
    allow(
        &f,
        json!({"gen":{"interpreter":"/usr/bin/perl","script":host_pl.to_string_lossy(),"untrusted_code":true},
               "gen-sh":{"interpreter":"/bin/bash","script":host_pl.to_string_lossy(),"untrusted_code":true}}),
    );
    let host = Host::new(APP, "staff");
    let generated = format!(
        "print \"GEN-RAN\\n\"; if (open(my $h, '<', '{}')) {{ print <$h>; }}",
        secrets.display()
    );
    let out = f
        .call(
            &host,
            "process",
            "run",
            json!({"program":"gen","stdin":generated}),
        )
        .unwrap();
    let stdout = out["stdout"].as_str().unwrap();
    assert!(stdout.contains("GEN-RAN"), "{out}");
    assert!(
        !stdout.contains("E20-SECRET-STORE"),
        "generated code escaped: {out}"
    );
    assert_eq!(
        code(f.call(
            &host,
            "process",
            "run",
            json!({"program":"gen-sh","stdin":"true"})
        )),
        ErrorCode::Forbidden,
        "a shell is never an interpreter for generated code"
    );
}
