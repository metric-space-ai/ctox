#![cfg(target_os = "linux")]
use ctox_process_network::{Guard, SOCKET};
use std::path::Path;
use std::process::Command;

#[test]
fn e20_net_domain_escape() {
    // The lane also runs this against the real CTOX binary, exercising its
    // early self-exec dispatch before daemon/store startup.
    let executable = std::env::var_os("CTOX_E20_TEST_EXECUTABLE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_BIN_EXE_ctox-process-network")).to_path_buf());
    let exe = executable.as_path();
    let guard = Guard::start(exe, &["example.com".into()]).expect("network owner starts");
    let sandbox = |args: &[&str]| {
        let mut c = Command::new("/usr/bin/bwrap");
        c.args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ]);
        c.arg("--ro-bind").arg(exe).arg(exe);
        c.arg("--ro-bind")
            .arg(&guard.socket)
            .arg(SOCKET)
            .args(["--remount-ro", "/", "--"]);
        guard.wrap(&mut c, exe);
        c.args(args)
            .env_clear()
            .env("HTTP_PROXY", "http://example.org:80")
            .env("NO_PROXY", "*")
            .output()
            .expect("sandbox executes")
    };
    let curl = |url, direct| {
        let mut args = vec![
            "/usr/bin/curl",
            "--silent",
            "--show-error",
            "--max-time",
            "15",
            "--connect-timeout",
            "5",
        ];
        if direct {
            args.extend(["--noproxy", "*"]);
        }
        args.push(url);
        sandbox(&args)
    };
    for url in ["http://example.com/", "https://example.com/"] {
        let allowed = curl(url, false);
        assert!(
            allowed.status.success(),
            "approved destination failed: {}",
            String::from_utf8_lossy(&allowed.stderr)
        );
        assert!(
            String::from_utf8_lossy(&allowed.stdout).contains("Example Domain"),
            "approved destination not reached: {}",
            String::from_utf8_lossy(&allowed.stdout)
        );
    }
    let denied = curl("http://example.org/", false);
    let body = String::from_utf8_lossy(&denied.stdout);
    assert!(
        !body.contains("Example Domain"),
        "E20-NET DOMAIN ESCAPE: {body}"
    );
    assert!(
        body.contains("blocked"),
        "foreign domain not explicitly rejected: {body} / {}",
        String::from_utf8_lossy(&denied.stderr)
    );
    let direct = curl("http://1.1.1.1/", true);
    assert!(!direct.status.success(), "E20-NET DIRECT ESCAPE");
    let connect = curl("https://example.org/", false);
    assert!(
        !connect.status.success() && String::from_utf8_lossy(&connect.stderr).contains("403"),
        "E20-NET CONNECT ESCAPE: {}",
        String::from_utf8_lossy(&connect.stderr)
    );
    let dns_target = format!("http://e20-net-{}.example.org/", std::process::id());
    let dns_denied = curl(&dns_target, false);
    assert!(
        String::from_utf8_lossy(&dns_denied.stdout).contains("blocked"),
        "foreign DNS target not rejected: {}",
        String::from_utf8_lossy(&dns_denied.stderr)
    );
    let sockets = sandbox(&["/usr/bin/perl", "-e", "socket(S,1,1,0); print 'UNIX='.(0+$!).qq(\\n); socket(V,40,1,0); print 'VSOCK='.(0+$!).qq(\\n);"]);
    let body = String::from_utf8_lossy(&sockets.stdout);
    assert!(
        sockets.status.success() && body == "UNIX=1\nVSOCK=1\n",
        "E20-NET SOCKET ESCAPE: {body} / {}",
        String::from_utf8_lossy(&sockets.stderr)
    );
    let socket = guard.socket.clone();
    drop(guard);
    assert!(!socket.exists(), "per-call proxy socket survives its owner");
}
