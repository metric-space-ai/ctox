#![forbid(unsafe_code)]

use aria2_rust::options::OptionSet;
use aria2_rust::session::Session;
use clap::{Arg, ArgAction, Command};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let m = Command::new("aria2c")
        .version(aria2_rust::VERSION)
        .about("aria2-rust — clean-room aria2")
        .arg(Arg::new("dir").short('d').long("dir").num_args(1))
        .arg(Arg::new("out").short('o').long("out").num_args(1))
        .arg(Arg::new("split").short('s').long("split").num_args(1))
        .arg(Arg::new("max-connection-per-server").short('x').long("max-connection-per-server").num_args(1))
        .arg(Arg::new("http-no-cache").long("http-no-cache").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("http-accept-gzip").long("http-accept-gzip").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-http-pipelining").long("enable-http-pipelining").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-http-keep-alive").long("enable-http-keep-alive").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-http-pipelining").long("max-http-pipelining").num_args(1))
        .arg(Arg::new("no-want-digest-header").long("no-want-digest-header").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("content-disposition-default-utf8").long("content-disposition-default-utf8").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("use-head").long("use-head").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("load-cookies").long("load-cookies").num_args(1))
        .arg(Arg::new("save-cookies").long("save-cookies").num_args(1))
        .arg(Arg::new("piece-length").long("piece-length").num_args(1))
        .arg(Arg::new("min-split-size").long("min-split-size").num_args(1))
        .arg(Arg::new("file-allocation").long("file-allocation").num_args(1))
        .arg(Arg::new("no-file-allocation-limit").long("no-file-allocation-limit").num_args(1))
        .arg(Arg::new("gid").long("gid").num_args(1))
        .arg(Arg::new("timeout").long("timeout").num_args(1))
        .arg(Arg::new("connect-timeout").long("connect-timeout").num_args(1))
        .arg(Arg::new("user-agent").long("user-agent").num_args(1))
        .arg(Arg::new("referer").long("referer").num_args(1))
        .arg(Arg::new("header").long("header").num_args(1).action(ArgAction::Append))
        .arg(Arg::new("http-user").long("http-user").num_args(1))
        .arg(Arg::new("http-auth-challenge").long("http-auth-challenge").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("http-passwd").long("http-passwd").num_args(1))
        .arg(Arg::new("http-proxy").long("http-proxy").num_args(1))
        .arg(Arg::new("https-proxy").long("https-proxy").num_args(1))
        .arg(Arg::new("https-proxy-user").long("https-proxy-user").num_args(1))
        .arg(Arg::new("https-proxy-passwd").long("https-proxy-passwd").num_args(1))
        .arg(Arg::new("all-proxy").long("all-proxy").num_args(1))
        .arg(Arg::new("all-proxy-user").long("all-proxy-user").num_args(1))
        .arg(Arg::new("all-proxy-passwd").long("all-proxy-passwd").num_args(1))
        .arg(Arg::new("no-proxy").long("no-proxy").num_args(1))
        .arg(Arg::new("http-proxy-user").long("http-proxy-user").num_args(1))
        .arg(Arg::new("http-proxy-passwd").long("http-proxy-passwd").num_args(1))
        .arg(Arg::new("proxy-method").long("proxy-method").num_args(1))
        .arg(Arg::new("continue").short('c').long("continue").action(ArgAction::SetTrue))
        .arg(Arg::new("allow-overwrite").long("allow-overwrite").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("auto-file-renaming").long("auto-file-renaming").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("no-overwrite").long("no-overwrite").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("conditional-get").long("conditional-get").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("remote-time").long("remote-time").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("always-resume").long("always-resume").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-resume-failure-tries").long("max-resume-failure-tries").num_args(1))
        .arg(Arg::new("parameterized-uri").long("parameterized-uri").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-tries").long("max-tries").num_args(1))
        .arg(Arg::new("retry-wait").long("retry-wait").num_args(1))
        .arg(Arg::new("max-file-not-found").long("max-file-not-found").num_args(1))
        .arg(Arg::new("max-download-limit").long("max-download-limit").num_args(1))
        .arg(Arg::new("max-upload-limit").long("max-upload-limit").num_args(1))
        .arg(Arg::new("lowest-speed-limit").long("lowest-speed-limit").num_args(1))
        .arg(Arg::new("netrc-path").long("netrc-path").num_args(1))
        .arg(Arg::new("no-netrc").long("no-netrc").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("dry-run").long("dry-run").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-overall-download-limit").long("max-overall-download-limit").num_args(1))
        .arg(Arg::new("max-overall-upload-limit").long("max-overall-upload-limit").num_args(1))
        .arg(Arg::new("force-sequential").long("force-sequential").short('Z').num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("reuse-uri").long("reuse-uri").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("uri-selector").long("uri-selector").num_args(1))
        .arg(Arg::new("server-stat-if").long("server-stat-if").num_args(1))
        .arg(Arg::new("server-stat-of").long("server-stat-of").num_args(1))
        .arg(Arg::new("server-stat-timeout").long("server-stat-timeout").num_args(1))
        .arg(Arg::new("on-download-complete").long("on-download-complete").num_args(1))
        .arg(Arg::new("on-download-error").long("on-download-error").num_args(1))
        .arg(Arg::new("on-download-start").long("on-download-start").num_args(1))
        .arg(Arg::new("on-download-pause").long("on-download-pause").num_args(1))
        .arg(Arg::new("on-download-stop").long("on-download-stop").num_args(1))
        .arg(Arg::new("on-bt-download-complete").long("on-bt-download-complete").num_args(1))
        .arg(Arg::new("interface").long("interface").num_args(1))
        .arg(Arg::new("multiple-interface").long("multiple-interface").num_args(1))
        .arg(Arg::new("disable-ipv6").long("disable-ipv6").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("remove-control-file").long("remove-control-file").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-download-result").long("max-download-result").num_args(1))
        .arg(Arg::new("keep-unfinished-download-result").long("keep-unfinished-download-result").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("async-dns-server").long("async-dns-server").num_args(1))
        .arg(Arg::new("enable-async-dns6").long("enable-async-dns6").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("dns-timeout").long("dns-timeout").num_args(1))
        .arg(Arg::new("async-dns").long("async-dns").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("pause").long("pause").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("force-save").long("force-save").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("stream-piece-selector").long("stream-piece-selector").num_args(1))
        .arg(Arg::new("allow-piece-length-change").long("allow-piece-length-change").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("pause-metadata").long("pause-metadata").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("save-session-interval").long("save-session-interval").num_args(1))
        .arg(Arg::new("auto-save-interval").long("auto-save-interval").num_args(1))
        .arg(Arg::new("disk-cache").long("disk-cache").num_args(1))
        .arg(Arg::new("rpc-save-upload-metadata").long("rpc-save-upload-metadata").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("save-not-found").long("save-not-found").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("select-least-used-host").long("select-least-used-host").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-rpc").long("enable-rpc").action(ArgAction::SetTrue))
        .arg(Arg::new("enable-room-share").long("enable-room-share").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("room-password").long("room-password").num_args(1))
        .arg(Arg::new("room-listen-port").long("room-listen-port").num_args(1))
        .arg(Arg::new("room-udp-port").long("room-udp-port").num_args(1))
        .arg(Arg::new("room-name").long("room-name").num_args(1))
        .arg(Arg::new("rpc-listen-port").long("rpc-listen-port").num_args(1))
        .arg(Arg::new("rpc-listen-all").long("rpc-listen-all").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("rpc-allow-origin-all").long("rpc-allow-origin-all").action(ArgAction::SetTrue))
        .arg(Arg::new("rpc-secret").long("rpc-secret").num_args(1))
        .arg(Arg::new("rpc-user").long("rpc-user").num_args(1))
        .arg(Arg::new("rpc-passwd").long("rpc-passwd").num_args(1))
        .arg(Arg::new("log").short('l').long("log").num_args(1))
        .arg(Arg::new("log-level").long("log-level").num_args(1))
        .arg(Arg::new("download-result").long("download-result").num_args(1))
        .arg(Arg::new("summary-interval").long("summary-interval").num_args(1))
        .arg(Arg::new("console-log-level").long("console-log-level").num_args(1))
        .arg(Arg::new("stderr").long("stderr").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("human-readable").long("human-readable").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("show-console-readout").long("show-console-readout").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("truncate-console-readout").long("truncate-console-readout").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-color").long("enable-color").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("socket-recv-buffer-size").long("socket-recv-buffer-size").num_args(1))
        .arg(Arg::new("dscp").long("dscp").num_args(1))
        .arg(Arg::new("rlimit-nofile").long("rlimit-nofile").num_args(1))
        .arg(Arg::new("rpc-max-request-size").long("rpc-max-request-size").num_args(1))
        .arg(Arg::new("rpc-secure").long("rpc-secure").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("rpc-certificate").long("rpc-certificate").num_args(1))
        .arg(Arg::new("rpc-private-key").long("rpc-private-key").num_args(1))
        .arg(Arg::new("stop").long("stop").num_args(1))
        .arg(Arg::new("stop-with-process").long("stop-with-process").num_args(1))
        .arg(Arg::new("check-certificate").long("check-certificate").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("ca-certificate").long("ca-certificate").num_args(1))
        .arg(Arg::new("certificate").long("certificate").num_args(1))
        .arg(Arg::new("private-key").long("private-key").num_args(1))
        .arg(Arg::new("min-tls-version").long("min-tls-version").num_args(1))
        .arg(Arg::new("ftp-user").long("ftp-user").num_args(1))
        .arg(Arg::new("ftp-passwd").long("ftp-passwd").num_args(1))
        .arg(Arg::new("ftp-proxy").long("ftp-proxy").num_args(1))
        .arg(Arg::new("ftp-proxy-user").long("ftp-proxy-user").num_args(1))
        .arg(Arg::new("ftp-proxy-passwd").long("ftp-proxy-passwd").num_args(1))
        .arg(Arg::new("ftp-pasv").long("ftp-pasv").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("ftp-reuse-connection").long("ftp-reuse-connection").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("ftp-type").long("ftp-type").num_args(1))
        .arg(Arg::new("ssh-host-key-md").long("ssh-host-key-md").num_args(1))
        .arg(Arg::new("torrent-file").short('T').long("torrent-file").num_args(1))
        .arg(Arg::new("listen-port").long("listen-port").num_args(1))
        .arg(Arg::new("bt-tracker").long("bt-tracker").num_args(1))
        .arg(Arg::new("bt-exclude-tracker").long("bt-exclude-tracker").num_args(1))
        .arg(Arg::new("bt-tracker-timeout").long("bt-tracker-timeout").num_args(1))
        .arg(Arg::new("bt-tracker-connect-timeout").long("bt-tracker-connect-timeout").num_args(1))
        .arg(Arg::new("bt-tracker-interval").long("bt-tracker-interval").num_args(1))
        .arg(Arg::new("peer-id-prefix").long("peer-id-prefix").num_args(1))
        .arg(Arg::new("peer-connection-timeout").long("peer-connection-timeout").num_args(1))
        .arg(Arg::new("peer-agent").long("peer-agent").num_args(1))
        .arg(Arg::new("bt-external-ip").long("bt-external-ip").num_args(1))
        .arg(Arg::new("bt-require-crypto").long("bt-require-crypto").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-force-encryption").long("bt-force-encryption").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-min-crypto-level").long("bt-min-crypto-level").num_args(1))
        .arg(Arg::new("bt-prioritize-piece").long("bt-prioritize-piece").num_args(1))
        .arg(Arg::new("bt-remove-unselected-file").long("bt-remove-unselected-file").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-detach-seed-only").long("bt-detach-seed-only").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-max-open-files").long("bt-max-open-files").num_args(1))
        .arg(Arg::new("bt-max-peers").long("bt-max-peers").num_args(1))
        .arg(Arg::new("bt-request-peer-speed-limit").long("bt-request-peer-speed-limit").num_args(1))
        .arg(Arg::new("bt-stop-timeout").long("bt-stop-timeout").num_args(1))
        .arg(Arg::new("bt-timeout").long("bt-timeout").num_args(1))
        .arg(Arg::new("bt-request-timeout").long("bt-request-timeout").num_args(1))
        .arg(Arg::new("bt-keep-alive-interval").long("bt-keep-alive-interval").num_args(1))
        .arg(Arg::new("max-outstanding-request").long("max-outstanding-request").num_args(1))
        .arg(Arg::new("bt-save-metadata").long("bt-save-metadata").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-metadata-only").long("bt-metadata-only").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-load-saved-metadata").long("bt-load-saved-metadata").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-peer-exchange").long("enable-peer-exchange").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-dht").long("enable-dht").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-dht6").long("enable-dht6").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("dht-listen-port").long("dht-listen-port").num_args(1))
        .arg(Arg::new("dht-entry-point").long("dht-entry-point").num_args(1))
        .arg(Arg::new("dht-entry-point6").long("dht-entry-point6").num_args(1))
        .arg(Arg::new("dht-listen-addr").long("dht-listen-addr").num_args(1))
        .arg(Arg::new("dht-listen-addr6").long("dht-listen-addr6").num_args(1))
        .arg(Arg::new("dht-file-path").long("dht-file-path").num_args(1))
        .arg(Arg::new("dht-file-path6").long("dht-file-path6").num_args(1))
        .arg(Arg::new("dht-message-timeout").long("dht-message-timeout").num_args(1))
        .arg(Arg::new("bt-enable-lpd").long("bt-enable-lpd").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-lpd-interface").long("bt-lpd-interface").num_args(1))
        .arg(Arg::new("select-file").long("select-file").num_args(1))
        .arg(Arg::new("index-out").long("index-out").num_args(1).action(ArgAction::Append))
        .arg(Arg::new("show-files").short('S').long("show-files").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("seed-ratio").long("seed-ratio").num_args(1))
        .arg(Arg::new("seed-time").long("seed-time").num_args(1))
        .arg(Arg::new("follow-torrent").long("follow-torrent").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-bittorrent").long("enable-bittorrent").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("metalink-file").short('M').long("metalink-file").num_args(1))
        .arg(Arg::new("metalink-base-uri").long("metalink-base-uri").num_args(1))
        .arg(Arg::new("follow-metalink").long("follow-metalink").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("enable-metalink").long("enable-metalink").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("metalink-language").long("metalink-language").num_args(1))
        .arg(Arg::new("metalink-os").long("metalink-os").num_args(1))
        .arg(Arg::new("metalink-location").long("metalink-location").num_args(1))
        .arg(Arg::new("metalink-version").long("metalink-version").num_args(1))
        .arg(Arg::new("metalink-preferred-protocol").long("metalink-preferred-protocol").num_args(1))
        .arg(Arg::new("metalink-enable-unique-protocol").long("metalink-enable-unique-protocol").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("check-integrity").long("check-integrity").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("hash-check-only").long("hash-check-only").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-hash-check-seed").long("bt-hash-check-seed").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-enable-hook-after-hash-check").long("bt-enable-hook-after-hash-check").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("bt-seed-unverified").long("bt-seed-unverified").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("checksum").long("checksum").num_args(1))
        .arg(Arg::new("realtime-chunk-checksum").long("realtime-chunk-checksum").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("input-file").short('i').long("input-file").num_args(1))
        .arg(Arg::new("deferred-input").long("deferred-input").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("save-session").long("save-session").num_args(1))
        .arg(Arg::new("conf-path").long("conf-path").num_args(1))
        .arg(Arg::new("no-conf").long("no-conf").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("max-concurrent-downloads").long("max-concurrent-downloads").short('j').num_args(1))
        .arg(Arg::new("startup-idle-time").long("startup-idle-time").num_args(1))
        .arg(Arg::new("max-downloads").long("max-downloads").num_args(1))
        .arg(Arg::new("optimize-concurrent-downloads").long("optimize-concurrent-downloads").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("daemon").short('D').long("daemon").num_args(0..=1).default_missing_value("true"))
        .arg(Arg::new("pid-file").long("pid-file").num_args(1))
        .arg(Arg::new("quiet").short('q').long("quiet").action(ArgAction::SetTrue))
        .arg(Arg::new("URI").num_args(0..))
        .get_matches();

    let mut opts = OptionSet::with_defaults();
    let no_conf = m.get_one::<String>("no-conf").map(|s| s != "false").unwrap_or(false);
    let conf_path = m.get_one::<String>("conf-path").map(|s| s.as_str());
    if let Err(e) = aria2_rust::options::load_conf(&mut opts, conf_path, no_conf) {
        eprintln!("conf: {e}");
        std::process::exit(1);
    }
    if let Some(v) = m.get_one::<String>("dir") { opts.set("dir", v.clone()); }
    if let Some(v) = m.get_one::<String>("out") { opts.set("out", v.clone()); }
    if let Some(v) = m.get_one::<String>("split") { opts.set("split", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-connection-per-server") { opts.set("max-connection-per-server", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-no-cache") { opts.set("http-no-cache", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-accept-gzip") { opts.set("http-accept-gzip", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-http-pipelining") { opts.set("enable-http-pipelining", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-http-keep-alive") { opts.set("enable-http-keep-alive", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-http-pipelining") { opts.set("max-http-pipelining", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-want-digest-header") { opts.set("no-want-digest-header", v.clone()); }
    if let Some(v) = m.get_one::<String>("content-disposition-default-utf8") { opts.set("content-disposition-default-utf8", v.clone()); }
    if let Some(v) = m.get_one::<String>("use-head") { opts.set("use-head", v.clone()); }
    if let Some(v) = m.get_one::<String>("load-cookies") { opts.set("load-cookies", v.clone()); }
    if let Some(v) = m.get_one::<String>("save-cookies") { opts.set("save-cookies", v.clone()); }
    if let Some(v) = m.get_one::<String>("piece-length") { opts.set("piece-length", v.clone()); }
    if let Some(v) = m.get_one::<String>("min-split-size") { opts.set("min-split-size", v.clone()); }
    if let Some(v) = m.get_one::<String>("file-allocation") { opts.set("file-allocation", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-file-allocation-limit") { opts.set("no-file-allocation-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("gid") { opts.set("gid", v.clone()); }
    if let Some(v) = m.get_one::<String>("timeout") { opts.set("timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("connect-timeout") { opts.set("connect-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("user-agent") { opts.set("user-agent", v.clone()); }
    if let Some(v) = m.get_one::<String>("referer") { opts.set("referer", v.clone()); }
    if let Some(vs) = m.get_many::<String>("header") {
        let joined = vs.cloned().collect::<Vec<_>>().join("\n");
        opts.set("header", joined);
    }
    if let Some(v) = m.get_one::<String>("http-user") { opts.set("http-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-auth-challenge") { opts.set("http-auth-challenge", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-passwd") { opts.set("http-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-proxy") { opts.set("http-proxy", v.clone()); }
    if let Some(v) = m.get_one::<String>("https-proxy") { opts.set("https-proxy", v.clone()); }
    if let Some(v) = m.get_one::<String>("https-proxy-user") { opts.set("https-proxy-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("https-proxy-passwd") { opts.set("https-proxy-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("all-proxy") { opts.set("all-proxy", v.clone()); }
    if let Some(v) = m.get_one::<String>("all-proxy-user") { opts.set("all-proxy-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("all-proxy-passwd") { opts.set("all-proxy-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-proxy") { opts.set("no-proxy", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-proxy-user") { opts.set("http-proxy-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("http-proxy-passwd") { opts.set("http-proxy-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("proxy-method") { opts.set("proxy-method", v.clone()); }
    if m.get_flag("continue") { opts.set("continue", "true"); }
    if let Some(v) = m.get_one::<String>("allow-overwrite") { opts.set("allow-overwrite", v.clone()); }
    if let Some(v) = m.get_one::<String>("auto-file-renaming") { opts.set("auto-file-renaming", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-overwrite") { opts.set("no-overwrite", v.clone()); }
    if let Some(v) = m.get_one::<String>("conditional-get") { opts.set("conditional-get", v.clone()); }
    if let Some(v) = m.get_one::<String>("remote-time") { opts.set("remote-time", v.clone()); }
    if let Some(v) = m.get_one::<String>("always-resume") { opts.set("always-resume", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-resume-failure-tries") { opts.set("max-resume-failure-tries", v.clone()); }
    if let Some(v) = m.get_one::<String>("parameterized-uri") { opts.set("parameterized-uri", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-tries") { opts.set("max-tries", v.clone()); }
    if let Some(v) = m.get_one::<String>("retry-wait") { opts.set("retry-wait", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-file-not-found") { opts.set("max-file-not-found", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-download-limit") { opts.set("max-download-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-upload-limit") { opts.set("max-upload-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("lowest-speed-limit") { opts.set("lowest-speed-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("netrc-path") { opts.set("netrc-path", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-netrc") { opts.set("no-netrc", v.clone()); }
    if let Some(v) = m.get_one::<String>("dry-run") { opts.set("dry-run", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-overall-download-limit") { opts.set("max-overall-download-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-overall-upload-limit") { opts.set("max-overall-upload-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("force-sequential") { opts.set("force-sequential", v.clone()); }
    if let Some(v) = m.get_one::<String>("reuse-uri") { opts.set("reuse-uri", v.clone()); }
    if let Some(v) = m.get_one::<String>("uri-selector") { opts.set("uri-selector", v.clone()); }
    if let Some(v) = m.get_one::<String>("server-stat-if") { opts.set("server-stat-if", v.clone()); }
    if let Some(v) = m.get_one::<String>("server-stat-of") { opts.set("server-stat-of", v.clone()); }
    if let Some(v) = m.get_one::<String>("server-stat-timeout") { opts.set("server-stat-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-download-complete") { opts.set("on-download-complete", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-download-error") { opts.set("on-download-error", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-download-start") { opts.set("on-download-start", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-download-pause") { opts.set("on-download-pause", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-download-stop") { opts.set("on-download-stop", v.clone()); }
    if let Some(v) = m.get_one::<String>("on-bt-download-complete") { opts.set("on-bt-download-complete", v.clone()); }
    if let Some(v) = m.get_one::<String>("interface") { opts.set("interface", v.clone()); }
    if let Some(v) = m.get_one::<String>("multiple-interface") { opts.set("multiple-interface", v.clone()); }
    if let Some(v) = m.get_one::<String>("disable-ipv6") { opts.set("disable-ipv6", v.clone()); }
    if let Some(v) = m.get_one::<String>("remove-control-file") { opts.set("remove-control-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-download-result") { opts.set("max-download-result", v.clone()); }
    if let Some(v) = m.get_one::<String>("keep-unfinished-download-result") { opts.set("keep-unfinished-download-result", v.clone()); }
    if let Some(v) = m.get_one::<String>("async-dns-server") { opts.set("async-dns-server", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-async-dns6") { opts.set("enable-async-dns6", v.clone()); }
    if let Some(v) = m.get_one::<String>("dns-timeout") { opts.set("dns-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("async-dns") { opts.set("async-dns", v.clone()); }
    if let Some(v) = m.get_one::<String>("pause") { opts.set("pause", v.clone()); }
    if let Some(v) = m.get_one::<String>("force-save") { opts.set("force-save", v.clone()); }
    if let Some(v) = m.get_one::<String>("stream-piece-selector") { opts.set("stream-piece-selector", v.clone()); }
    if let Some(v) = m.get_one::<String>("allow-piece-length-change") { opts.set("allow-piece-length-change", v.clone()); }
    if let Some(v) = m.get_one::<String>("pause-metadata") { opts.set("pause-metadata", v.clone()); }
    if let Some(v) = m.get_one::<String>("save-session-interval") { opts.set("save-session-interval", v.clone()); }
    if let Some(v) = m.get_one::<String>("auto-save-interval") { opts.set("auto-save-interval", v.clone()); }
    if let Some(v) = m.get_one::<String>("disk-cache") { opts.set("disk-cache", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-secret") { opts.set("rpc-secret", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-room-share") { opts.set("enable-room-share", v.clone()); }
    if let Some(v) = m.get_one::<String>("room-password") { opts.set("room-password", v.clone()); }
    if let Some(v) = m.get_one::<String>("room-listen-port") { opts.set("room-listen-port", v.clone()); }
    if let Some(v) = m.get_one::<String>("room-udp-port") { opts.set("room-udp-port", v.clone()); }
    if let Some(v) = m.get_one::<String>("room-name") { opts.set("room-name", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-user") { opts.set("rpc-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-passwd") { opts.set("rpc-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("log") { opts.set("log", v.clone()); }
    if let Some(v) = m.get_one::<String>("log-level") { opts.set("log-level", v.clone()); }
    if let Some(v) = m.get_one::<String>("download-result") { opts.set("download-result", v.clone()); }
    if let Some(v) = m.get_one::<String>("summary-interval") { opts.set("summary-interval", v.clone()); }
    if let Some(v) = m.get_one::<String>("console-log-level") { opts.set("console-log-level", v.clone()); }
    if let Some(v) = m.get_one::<String>("stderr") { opts.set("stderr", v.clone()); }
    if let Some(v) = m.get_one::<String>("human-readable") { opts.set("human-readable", v.clone()); }
    if let Some(v) = m.get_one::<String>("show-console-readout") { opts.set("show-console-readout", v.clone()); }
    if let Some(v) = m.get_one::<String>("truncate-console-readout") { opts.set("truncate-console-readout", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-color") { opts.set("enable-color", v.clone()); }
    if let Some(v) = m.get_one::<String>("socket-recv-buffer-size") { opts.set("socket-recv-buffer-size", v.clone()); }
    if let Some(v) = m.get_one::<String>("dscp") { opts.set("dscp", v.clone()); }
    if let Some(v) = m.get_one::<String>("rlimit-nofile") { opts.set("rlimit-nofile", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-max-request-size") { opts.set("rpc-max-request-size", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-secure") { opts.set("rpc-secure", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-certificate") { opts.set("rpc-certificate", v.clone()); }
    if let Some(v) = m.get_one::<String>("rpc-private-key") { opts.set("rpc-private-key", v.clone()); }
    if let Some(v) = m.get_one::<String>("stop") { opts.set("stop", v.clone()); }
    if let Some(v) = m.get_one::<String>("stop-with-process") { opts.set("stop-with-process", v.clone()); }
    if m.get_flag("quiet") { opts.set("quiet", "true"); }
    if let Some(v) = m.get_one::<String>("rpc-save-upload-metadata") { opts.set("rpc-save-upload-metadata", v.clone()); }
    if let Some(v) = m.get_one::<String>("save-not-found") { opts.set("save-not-found", v.clone()); }
    if let Some(v) = m.get_one::<String>("select-least-used-host") { opts.set("select-least-used-host", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-concurrent-downloads") { opts.set("max-concurrent-downloads", v.clone()); }
    if let Some(v) = m.get_one::<String>("startup-idle-time") { opts.set("startup-idle-time", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-downloads") { opts.set("max-downloads", v.clone()); }
    if let Some(v) = m.get_one::<String>("optimize-concurrent-downloads") { opts.set("optimize-concurrent-downloads", v.clone()); }
    if let Some(v) = m.get_one::<String>("daemon") { opts.set("daemon", v.clone()); }
    if let Some(v) = m.get_one::<String>("pid-file") { opts.set("pid-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("check-certificate") { opts.set("check-certificate", v.clone()); }
    if let Some(v) = m.get_one::<String>("ca-certificate") { opts.set("ca-certificate", v.clone()); }
    if let Some(v) = m.get_one::<String>("certificate") { opts.set("certificate", v.clone()); }
    if let Some(v) = m.get_one::<String>("private-key") { opts.set("private-key", v.clone()); }
    if let Some(v) = m.get_one::<String>("min-tls-version") { opts.set("min-tls-version", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-user") { opts.set("ftp-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-passwd") { opts.set("ftp-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-proxy") { opts.set("ftp-proxy", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-proxy-user") { opts.set("ftp-proxy-user", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-proxy-passwd") { opts.set("ftp-proxy-passwd", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-pasv") { opts.set("ftp-pasv", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-reuse-connection") { opts.set("ftp-reuse-connection", v.clone()); }
    if let Some(v) = m.get_one::<String>("ftp-type") { opts.set("ftp-type", v.clone()); }
    if let Some(v) = m.get_one::<String>("ssh-host-key-md") { opts.set("ssh-host-key-md", v.clone()); }
    if let Some(v) = m.get_one::<String>("torrent-file") { opts.set("torrent-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("listen-port") { opts.set("listen-port", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-tracker") { opts.set("bt-tracker", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-exclude-tracker") { opts.set("bt-exclude-tracker", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-tracker-timeout") { opts.set("bt-tracker-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-tracker-connect-timeout") { opts.set("bt-tracker-connect-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-tracker-interval") { opts.set("bt-tracker-interval", v.clone()); }
    if let Some(v) = m.get_one::<String>("peer-id-prefix") { opts.set("peer-id-prefix", v.clone()); }
    if let Some(v) = m.get_one::<String>("peer-connection-timeout") { opts.set("peer-connection-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("peer-agent") { opts.set("peer-agent", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-external-ip") { opts.set("bt-external-ip", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-require-crypto") { opts.set("bt-require-crypto", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-force-encryption") { opts.set("bt-force-encryption", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-min-crypto-level") { opts.set("bt-min-crypto-level", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-prioritize-piece") { opts.set("bt-prioritize-piece", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-remove-unselected-file") { opts.set("bt-remove-unselected-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-detach-seed-only") { opts.set("bt-detach-seed-only", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-max-open-files") { opts.set("bt-max-open-files", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-max-peers") { opts.set("bt-max-peers", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-request-peer-speed-limit") { opts.set("bt-request-peer-speed-limit", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-stop-timeout") { opts.set("bt-stop-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-timeout") { opts.set("bt-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-request-timeout") { opts.set("bt-request-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-keep-alive-interval") { opts.set("bt-keep-alive-interval", v.clone()); }
    if let Some(v) = m.get_one::<String>("max-outstanding-request") { opts.set("max-outstanding-request", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-save-metadata") { opts.set("bt-save-metadata", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-metadata-only") { opts.set("bt-metadata-only", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-load-saved-metadata") { opts.set("bt-load-saved-metadata", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-peer-exchange") { opts.set("enable-peer-exchange", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-dht") { opts.set("enable-dht", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-dht6") { opts.set("enable-dht6", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-listen-port") { opts.set("dht-listen-port", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-entry-point") { opts.set("dht-entry-point", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-entry-point6") { opts.set("dht-entry-point6", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-listen-addr") { opts.set("dht-listen-addr", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-listen-addr6") { opts.set("dht-listen-addr6", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-file-path") { opts.set("dht-file-path", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-file-path6") { opts.set("dht-file-path6", v.clone()); }
    if let Some(v) = m.get_one::<String>("dht-message-timeout") { opts.set("dht-message-timeout", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-enable-lpd") { opts.set("bt-enable-lpd", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-lpd-interface") { opts.set("bt-lpd-interface", v.clone()); }
    if let Some(v) = m.get_one::<String>("select-file") { opts.set("select-file", v.clone()); }
    if let Some(vs) = m.get_many::<String>("index-out") {
        let joined = vs.cloned().collect::<Vec<_>>().join("\n");
        opts.set("index-out", joined);
    }
    if let Some(v) = m.get_one::<String>("show-files") { opts.set("show-files", v.clone()); }
    if let Some(v) = m.get_one::<String>("seed-ratio") { opts.set("seed-ratio", v.clone()); }
    if let Some(v) = m.get_one::<String>("seed-time") { opts.set("seed-time", v.clone()); }
    if let Some(v) = m.get_one::<String>("follow-torrent") { opts.set("follow-torrent", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-bittorrent") { opts.set("enable-bittorrent", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-file") { opts.set("metalink-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-base-uri") { opts.set("metalink-base-uri", v.clone()); }
    if let Some(v) = m.get_one::<String>("follow-metalink") { opts.set("follow-metalink", v.clone()); }
    if let Some(v) = m.get_one::<String>("enable-metalink") { opts.set("enable-metalink", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-language") { opts.set("metalink-language", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-os") { opts.set("metalink-os", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-location") { opts.set("metalink-location", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-version") { opts.set("metalink-version", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-preferred-protocol") { opts.set("metalink-preferred-protocol", v.clone()); }
    if let Some(v) = m.get_one::<String>("metalink-enable-unique-protocol") { opts.set("metalink-enable-unique-protocol", v.clone()); }
    if let Some(v) = m.get_one::<String>("check-integrity") { opts.set("check-integrity", v.clone()); }
    if let Some(v) = m.get_one::<String>("hash-check-only") { opts.set("hash-check-only", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-hash-check-seed") { opts.set("bt-hash-check-seed", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-enable-hook-after-hash-check") { opts.set("bt-enable-hook-after-hash-check", v.clone()); }
    if let Some(v) = m.get_one::<String>("bt-seed-unverified") { opts.set("bt-seed-unverified", v.clone()); }
    if let Some(v) = m.get_one::<String>("checksum") { opts.set("checksum", v.clone()); }
    if let Some(v) = m.get_one::<String>("realtime-chunk-checksum") { opts.set("realtime-chunk-checksum", v.clone()); }
    if let Some(v) = m.get_one::<String>("input-file") { opts.set("input-file", v.clone()); }
    if let Some(v) = m.get_one::<String>("deferred-input") { opts.set("deferred-input", v.clone()); }
    if let Some(v) = m.get_one::<String>("save-session") { opts.set("save-session", v.clone()); }
    if let Some(v) = m.get_one::<String>("conf-path") { opts.set("conf-path", v.clone()); }
    if let Some(v) = m.get_one::<String>("no-conf") { opts.set("no-conf", v.clone()); }

    let uris: Vec<String> = m.get_many::<String>("URI").map(|v| v.cloned().collect()).unwrap_or_default();
    if opts.bool("show-files", false) {
        if let Some(p) = opts.get("torrent-file").filter(|s| !s.is_empty()) {
            match std::fs::read(p).and_then(|b| {
                aria2_rust::bt::format_show_files(&b).map_err(|e| std::io::Error::other(e.to_string()))
            }) {
                Ok(listing) => {
                    print!("{listing}");
                    return;
                }
                Err(e) => {
                    eprintln!("show-files: {e}");
                    std::process::exit(1);
                }
            }
        }
        if let Some(p) = opts.get("metalink-file").filter(|s| !s.is_empty()) {
            match std::fs::read(p).and_then(|b| {
                aria2_rust::metalink::parse_bytes(&b)
                    .map(|f| aria2_rust::metalink::format_show_files(&f))
                    .map_err(|e| std::io::Error::other(e.to_string()))
            }) {
                Ok(listing) => {
                    print!("{listing}");
                    return;
                }
                Err(e) => {
                    eprintln!("show-files: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
    if opts.bool("daemon", false) {
        daemonize();
    }
    if let Some(p) = opts.get("pid-file").filter(|s| !s.is_empty()) {
        if let Err(e) = std::fs::write(p, format!("{}\n", std::process::id())) {
            eprintln!("pid-file: {e}");
            std::process::exit(1);
        }
    }
    let session = Session::new(opts.clone()).expect("session");
    let _ = session.load_input_file().await;

    if m.get_flag("enable-rpc") {
        let port: u16 = m.get_one::<String>("rpc-listen-port").and_then(|s| s.parse().ok()).unwrap_or(6800);
        let listen_all = m.get_one::<String>("rpc-listen-all").map(|s| s != "false").unwrap_or(false);
        if !uris.is_empty() {
            let _ = session.add_uri_and_start(uris, OptionSet::new()).await;
        }
        if let Err(e) = tokio::select! {
            r = aria2_rust::rpc::serve(Arc::clone(&session), listen_all, port) => r,
            _ = session.wait_stopped() => Ok(()),
        } {
            eprintln!("rpc: {e}");
            std::process::exit(1);
        }
        return;
    }

    if uris.is_empty() && opts.get("torrent-file").filter(|s| !s.is_empty()).is_none()
        && opts.get("metalink-file").filter(|s| !s.is_empty()).is_none()
        && opts.get("input-file").filter(|s| !s.is_empty()).is_none()
    {
        eprintln!("aria2c: no URI");
        std::process::exit(1);
    }
    if !uris.is_empty()
        || opts.get("torrent-file").filter(|s| !s.is_empty()).is_some()
        || opts.get("metalink-file").filter(|s| !s.is_empty()).is_some()
    {
        let gid = session.add_uri_and_start(uris, OptionSet::new()).await.expect("add");
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let active = session.tell_active().await;
            let waiting = session.tell_waiting(0, 10_000).await;
            if active.iter().chain(waiting.iter()).any(|st| {
                st.get("status").and_then(|v| v.as_str()) == Some("error")
            }) {
                let st = session.tell_status(gid.as_str()).await.ok();
                eprintln!(
                    "{}",
                    st.as_ref()
                        .and_then(|s| s.get("errorMessage"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("error")
                );
                std::process::exit(1);
            }
            if active.is_empty() && waiting.is_empty() && !session.has_deferred().await {
                break;
            }
        }
        return;
    }
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let active = session.tell_active().await;
        let waiting = session.tell_waiting(0, 10_000).await;
        if active.iter().chain(waiting.iter()).any(|st| {
            st.get("status").and_then(|v| v.as_str()) == Some("error")
        }) {
            std::process::exit(1);
        }
        if active.is_empty() && waiting.is_empty() && !session.has_deferred().await {
            break;
        }
    }
}

/// C++ `-D, --daemon`: parent returns immediately; child keeps the session/RPC.
fn daemonize() -> ! {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("daemon: {e}");
            std::process::exit(1);
        }
    };
    let mut cmd = std::process::Command::new(exe);
    for a in std::env::args().skip(1) {
        if a == "-D" || a == "--daemon" || a.starts_with("--daemon=") {
            continue;
        }
        cmd.arg(a);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("daemon: {e}");
            std::process::exit(1);
        }
    }
}
