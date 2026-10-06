#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::gid::Gid;
use crate::http::{self, HttpJob, HttpProgress, OverallLimiter};
use crate::options::OptionSet;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

#[derive(Clone, Debug)]
pub struct Task {
    pub gid: Gid,
    pub uris: Vec<String>,
    pub dest: PathBuf,
    pub status: String,
    pub error: Option<String>,
    pub progress: HttpProgress,
    pub opts: OptionSet,
    pub torrent: Option<Vec<u8>>,
}

pub struct Session {
    session_id: String,
    opts: Mutex<OptionSet>,
    tasks: Mutex<HashMap<String, Task>>,
    order: Mutex<Vec<String>>,
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
    overall: Arc<OverallLimiter>,
    overall_up: Arc<OverallLimiter>,
    console_out: std::sync::Mutex<String>,
    console_err: std::sync::Mutex<String>,
    stopped: AtomicBool,
    stop_notify: tokio::sync::Notify,
    deferred: Mutex<VecDeque<(Vec<String>, OptionSet)>>,
    idle_until: tokio::time::Instant,
    room: Mutex<Option<Arc<crate::room::RoomHub>>>,
}

impl Session {
    pub fn new(opts: OptionSet) -> Result<Arc<Self>> {
        let overall = OverallLimiter::new();
        if let Some(n) = crate::http::parse_speed(opts.get("max-overall-download-limit").unwrap_or("0")) {
            overall.set_limit(n);
        }
        let overall_up = OverallLimiter::new();
        if let Some(n) = crate::http::parse_speed(opts.get("max-overall-upload-limit").unwrap_or("0")) {
            overall_up.set_limit(n);
        }
        crate::rlimit::apply_nofile(&opts)?;
        let idle = opts.u64("startup-idle-time", 0).min(60);
        let idle_until = tokio::time::Instant::now() + std::time::Duration::from_secs(idle);
        let interval = opts.u64("save-session-interval", 0);
        let stop_secs = opts.u64("stop", 0);
        let stop_pid = opts.get("stop-with-process").filter(|s| !s.is_empty()).map(|s| s.to_string());
        let me = Arc::new(Self {
            session_id: Gid::generate().0,
            opts: Mutex::new(opts),
            tasks: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            cancels: Mutex::new(HashMap::new()),
            overall,
            overall_up,
            console_out: std::sync::Mutex::new(String::new()),
            console_err: std::sync::Mutex::new(String::new()),
            stopped: AtomicBool::new(false),
            stop_notify: tokio::sync::Notify::new(),
            deferred: Mutex::new(VecDeque::new()),
            idle_until,
            room: Mutex::new(None),
        });
        spawn_lifetime_stop(&me, interval, stop_secs, stop_pid);
        {
            let me2 = Arc::clone(&me);
            tokio::spawn(async move {
                me2.ensure_room_share().await;
            });
        }
        Ok(me)
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub async fn wait_stopped(&self) {
        loop {
            let notified = self.stop_notify.notified();
            if self.is_stopped() {
                return;
            }
            notified.await;
        }
    }

    pub async fn stop_application(self: &Arc<Self>) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.stop_notify.notify_waiters();
        let ids: Vec<String> = {
            let g = self.tasks.lock().await;
            g.values()
                .filter(|t| {
                    t.status == "active" || t.status == "waiting" || t.status == "paused"
                })
                .map(|t| t.gid.0.clone())
                .collect()
        };
        for id in ids {
            let _ = self.force_remove(&id).await;
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn console_text(&self) -> String {
        format!("{}{}", self.stdout_text(), self.stderr_text())
    }

    pub fn stdout_text(&self) -> String {
        self.console_out.lock().unwrap().clone()
    }

    pub fn stderr_text(&self) -> String {
        self.console_err.lock().unwrap().clone()
    }

    pub async fn add_uri_and_start(self: &Arc<Self>, uris: Vec<String>, extra: OptionSet) -> Result<Gid> {
        if self.is_stopped() {
            return Err(Error::Other("stopped".into()));
        }
        self.check_max_downloads().await?;
        let mut opts = self.opts.lock().await.clone();
        opts.merge(&extra);
        if extra.get("metalink-file").or_else(|| opts.get("metalink-file")).is_some_and(|s| !s.is_empty()) {
            let path = extra
                .get("metalink-file")
                .or_else(|| opts.get("metalink-file"))
                .unwrap()
                .to_string();
            let bytes = std::fs::read(&path)?;
            let gids = Box::pin(self.add_metalink_and_start(bytes, extra)).await?;
            return gids
                .into_iter()
                .next()
                .ok_or_else(|| Error::Other("metalink: no files".into()));
        }
        if uris.is_empty() && !crate::bt::looks_like_bt(&uris, &opts) {
            return Err(Error::Other("no uri".into()));
        }
        let uris = if opts.bool("parameterized-uri", false) {
            uris.into_iter()
                .flat_map(|u| {
                    if u.starts_with("magnet:") {
                        vec![u]
                    } else {
                        crate::http::expand_parameterized(&u)
                    }
                })
                .collect::<Vec<_>>()
        } else {
            uris
        };
        if opts.bool("force-sequential", false) && uris.len() > 1 {
            let mut one = extra.clone();
            one.set("force-sequential", "false");
            one.set("out", "");
            let mut first: Option<Gid> = None;
            for u in uris {
                let g = Box::pin(self.add_uri_and_start(vec![u], one.clone())).await?;
                if first.is_none() {
                    first = Some(g);
                }
            }
            return first.ok_or_else(|| Error::Other("force-sequential: no uri".into()));
        }
        if let Some(u) = uris.iter().find(|u| u.starts_with("magnet:")) {
            opts.set("magnet", u.clone());
        }
        let gid = self.take_gid(&extra, &opts).await?;
        let dest = if crate::bt::looks_like_bt(&uris, &opts) {
            bt_dest(&uris, &opts)
        } else {
            dest_for(&uris[0], &opts)
        };
        let torrent = crate::bt::load_torrent_bytes(&uris, &opts)
            .ok()
            .filter(|b| !b.is_empty());
        if opts.bool("show-files", false) {
            if let Some(ref t) = torrent {
                let listing = crate::bt::format_show_files(t)?;
                if let Ok(mut g) = self.console_out.lock() {
                    g.push_str(&listing);
                }
                let gid = self.take_gid(&extra, &opts).await?;
                let dest = dest.clone();
                let progress = HttpProgress::with_limiters(
                    Arc::clone(&self.overall),
                    Arc::clone(&self.overall_up),
                );
                let task = Task {
                    gid: gid.clone(),
                    uris: uris.clone(),
                    dest,
                    status: "complete".into(),
                    error: None,
                    progress,
                    opts: opts.clone(),
                    torrent: torrent.clone(),
                };
                self.tasks.lock().await.insert(gid.0.clone(), task);
                self.order.lock().await.push(gid.0.clone());
                return Ok(gid);
            }
        }
        let pause = extra.bool("pause", false) || opts.bool("pause", false);
        let start_now = !pause && self.active_count().await < self.max_concurrent().await;
        let progress = HttpProgress::with_limiters(
            Arc::clone(&self.overall),
            Arc::clone(&self.overall_up),
        );
        let task = Task {
            gid: gid.clone(),
            uris: uris.clone(),
            dest: dest.clone(),
            status: if pause {
                "paused".into()
            } else if start_now {
                "active".into()
            } else {
                "waiting".into()
            },
            error: None,
            progress: progress.clone(),
            opts: opts.clone(),
            torrent,
        };
        self.tasks.lock().await.insert(gid.0.clone(), task);
        self.order.lock().await.push(gid.0.clone());
        if start_now {
            launch_transfer(Arc::clone(self), gid.0.clone());
        }
        Ok(gid)
    }

    /// C++ `--gid=GID`: 16 hex, unique among live downloads.
    async fn take_gid(&self, extra: &OptionSet, opts: &OptionSet) -> Result<Gid> {
        let raw = extra
            .get("gid")
            .or_else(|| opts.get("gid"))
            .filter(|s| !s.is_empty());
        let gid = match raw {
            Some(s) => Gid::parse(s)?,
            None => Gid::generate(),
        };
        if self.tasks.lock().await.contains_key(gid.as_str()) {
            return Err(Error::Other(format!("GID {} is already used", gid)));
        }
        Ok(gid)
    }

    pub async fn add_metalink_and_start(
        self: &Arc<Self>,
        bytes: Vec<u8>,
        mut extra: OptionSet,
    ) -> Result<Vec<Gid>> {
        let files = crate::metalink::parse_bytes(&bytes)?;
        let mut merged = self.opts.lock().await.clone();
        merged.merge(&extra);
        if !merged.bool("enable-metalink", true) {
            return Err(Error::Other("enable-metalink=false".into()));
        }
        save_rpc_upload(&merged, &bytes, metalink_ext(&bytes))?;
        let files = crate::metalink::filter_files(files, &merged);
        if merged.bool("show-files", false) {
            let listing = crate::metalink::format_show_files(&files);
            if let Ok(mut g) = self.console_out.lock() {
                g.push_str(&listing);
            }
            let gid = self.take_gid(&extra, &merged).await?;
            let dest = merged.dir().join(
                files
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("index"),
            );
            let progress = HttpProgress::with_limiters(
                Arc::clone(&self.overall),
                Arc::clone(&self.overall_up),
            );
            progress.completed.store(0, std::sync::atomic::Ordering::Relaxed);
            let task = Task {
                gid: gid.clone(),
                uris: Vec::new(),
                dest,
                status: "complete".into(),
                error: None,
                progress,
                opts: merged,
                torrent: None,
            };
            self.tasks.lock().await.insert(gid.0.clone(), task);
            self.order.lock().await.push(gid.0.clone());
            return Ok(vec![gid]);
        }
        let mut gids = Vec::new();
        for f in files {
            let urls = f.select_urls(&merged);
            if urls.is_empty() {
                continue;
            }
            let mut e = extra.clone();
            e.set("metalink-file", "");
            if extra.get("out").is_none() {
                let name = std::path::Path::new(&f.name)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.name.clone());
                e.set("out", name);
            }
            if extra.get("checksum").is_none() {
                if let Some(cs) = f.checksum_spec() {
                    e.set("checksum", cs);
                }
            }
            if extra.get("piece-checksum").is_none() {
                if let Some(ps) = f.piece_checksum_spec() {
                    e.set("piece-checksum", ps);
                }
            }
            gids.push(Box::pin(self.add_uri_and_start(urls, e)).await?);
            extra.set("gid", "");
        }
        if gids.is_empty() {
            return Err(Error::Other("metalink: no usable urls".into()));
        }
        Ok(gids)
    }

    pub async fn add_torrent_and_start(
        self: &Arc<Self>,
        torrent: Vec<u8>,
        uris: Vec<String>,
        extra: OptionSet,
    ) -> Result<Gid> {
        self.check_max_downloads().await?;
        let mut opts = self.opts.lock().await.clone();
        opts.merge(&extra);
        if !opts.bool("enable-bittorrent", true) {
            return Err(Error::Bt("enable-bittorrent=false".into()));
        }
        save_rpc_upload(&opts, &torrent, "torrent")?;
        let meta = crate::bt::MetaInfo::from_torrent(&torrent)?;
        if opts.bool("show-files", false) {
            let listing = crate::bt::format_show_files_meta(&meta);
            if let Ok(mut g) = self.console_out.lock() {
                g.push_str(&listing);
            }
            let gid = self.take_gid(&extra, &opts).await?;
            let dest = if let Some(o) = opts.out() {
                opts.dir().join(o)
            } else {
                opts.dir().join(&meta.name)
            };
            let progress = HttpProgress::with_limiters(
                Arc::clone(&self.overall),
                Arc::clone(&self.overall_up),
            );
            let task = Task {
                gid: gid.clone(),
                uris: uris.clone(),
                dest,
                status: "complete".into(),
                error: None,
                progress,
                opts: opts.clone(),
                torrent: Some(torrent),
            };
            self.tasks.lock().await.insert(gid.0.clone(), task);
            self.order.lock().await.push(gid.0.clone());
            return Ok(gid);
        }
        let gid = self.take_gid(&extra, &opts).await?;
        let dest = if let Some(o) = opts.out() {
            opts.dir().join(o)
        } else {
            opts.dir().join(&meta.name)
        };
        let progress = HttpProgress::with_limiters(
            Arc::clone(&self.overall),
            Arc::clone(&self.overall_up),
        );
        let pause = extra.bool("pause", false) || opts.bool("pause", false);
        let start_now = !pause && self.active_count().await < self.max_concurrent().await;
        let task = Task {
            gid: gid.clone(),
            uris: uris.clone(),
            dest: dest.clone(),
            status: if pause {
                "paused".into()
            } else if start_now {
                "active".into()
            } else {
                "waiting".into()
            },
            error: None,
            progress: progress.clone(),
            opts: opts.clone(),
            torrent: Some(torrent.clone()),
        };
        self.tasks.lock().await.insert(gid.0.clone(), task);
        self.order.lock().await.push(gid.0.clone());
        if start_now {
            launch_transfer(Arc::clone(self), gid.0.clone());
        }
        Ok(gid)
    }

    pub async fn pause(self: &Arc<Self>, gid: &str) -> Result<String> {
        let mut g = self.tasks.lock().await;
        let t = g.get_mut(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        if t.status == "complete" || t.status == "error" || t.status == "removed" {
            return Err(Error::Rpc("cannot pause".into()));
        }
        t.status = "paused".into();
        let hooks = event_hooks(t, &["on-download-pause", "on-download-stop"]);
        drop(g);
        if let Some(tx) = self.cancels.lock().await.get(gid) {
            let _ = tx.send(true);
        }
        run_hooks(&hooks);
        kick_waiting(Arc::clone(self));
        Ok(gid.to_string())
    }

    pub async fn pause_all(self: &Arc<Self>) -> Result<String> {
        let gids: Vec<String> = {
            let g = self.tasks.lock().await;
            g.values()
                .filter(|t| t.status == "active")
                .map(|t| t.gid.0.clone())
                .collect()
        };
        for id in gids {
            let _ = self.pause(&id).await;
        }
        Ok("OK".into())
    }

    pub async fn unpause(self: &Arc<Self>, gid: &str) -> Result<String> {
        let can = self.active_count().await < self.max_concurrent().await;
        {
            let mut g = self.tasks.lock().await;
            let t = g.get_mut(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
            if t.status != "paused" && t.status != "waiting" {
                return Err(Error::Rpc("not paused".into()));
            }
            t.opts.set("continue", "true");
            t.error = None;
            if can {
                t.status = "active".into();
            } else {
                t.status = "waiting".into();
                return Ok(gid.to_string());
            }
        }
        launch_transfer(Arc::clone(self), gid.to_string());
        Ok(gid.to_string())
    }

    pub async fn unpause_all(self: &Arc<Self>) -> Result<String> {
        let gids: Vec<String> = {
            let g = self.tasks.lock().await;
            let order = self.order.lock().await;
            order
                .iter()
                .filter(|id| {
                    g.get(*id)
                        .map(|t| t.status == "paused" || t.status == "waiting")
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        };
        for id in gids {
            let _ = self.unpause(&id).await;
        }
        Ok("OK".into())
    }

    pub async fn change_position(&self, gid: &str, pos: i64, how: &str) -> Result<i64> {
        let mut order = self.order.lock().await;
        let cur = order
            .iter()
            .position(|g| g == gid)
            .ok_or_else(|| Error::Rpc("gid not found".into()))?;
        let len = order.len() as i64;
        if len == 0 {
            return Ok(0);
        }
        let dest = match how {
            "POS_SET" => pos,
            "POS_CUR" => cur as i64 + pos,
            "POS_END" => len - 1 + pos,
            _ => return Err(Error::Rpc("how must be POS_SET, POS_CUR, or POS_END".into())),
        };
        let dest = dest.clamp(0, len - 1) as usize;
        let id = order.remove(cur);
        order.insert(dest, id);
        Ok(dest as i64)
    }

    async fn max_concurrent(&self) -> usize {
        let opts = self.opts.lock().await;
        let cap = opts.usize("max-concurrent-downloads", 5).max(1);
        let raw = opts
            .get("optimize-concurrent-downloads")
            .unwrap_or("false")
            .to_string();
        drop(opts);
        let Some((a, b)) = crate::options::parse_optimize_concurrent(&raw) else {
            return cap;
        };
        crate::options::optimize_concurrent_n(a, b, self.overall.download_speed(), cap)
    }

    async fn live_count(&self) -> usize {
        let detach = self.opts.lock().await.bool("bt-detach-seed-only", false);
        self.tasks
            .lock()
            .await
            .values()
            .filter(|t| {
                matches!(t.status.as_str(), "active" | "waiting" | "paused")
                    && !(detach && t.progress.seeding.load(Ordering::SeqCst))
            })
            .count()
    }

    /// C++ `--max-downloads`: cap on live (active+waiting+paused) items. 0 = unlimited.
    async fn check_max_downloads(&self) -> Result<()> {
        let max = self.opts.lock().await.usize("max-downloads", 0);
        if max == 0 {
            return Ok(());
        }
        if self.live_count().await >= max {
            return Err(Error::Rpc(format!("max-downloads {max} reached")));
        }
        Ok(())
    }

    async fn active_count(&self) -> usize {
        let detach = self.opts.lock().await.bool("bt-detach-seed-only", false);
        self.tasks
            .lock()
            .await
            .values()
            .filter(|t| {
                t.status == "active"
                    && !(detach && t.progress.seeding.load(Ordering::SeqCst))
            })
            .count()
    }

    pub async fn get_option(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        Ok(option_map_json(&t.opts))
    }

    pub async fn change_option(&self, gid: &str, extra: OptionSet) -> Result<String> {
        let mut g = self.tasks.lock().await;
        let t = g.get_mut(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        t.opts.merge(&extra);
        Ok("OK".into())
    }

    pub async fn get_global_option(&self) -> Value {
        option_map_json(&*self.opts.lock().await)
    }

    pub async fn change_global_option(self: &Arc<Self>, extra: OptionSet) -> Result<String> {
        if let Some(v) = extra.get("max-overall-download-limit") {
            if let Some(n) = crate::http::parse_speed(v) {
                self.overall.set_limit(n);
            }
        }
        if let Some(v) = extra.get("max-overall-upload-limit") {
            if let Some(n) = crate::http::parse_speed(v) {
                self.overall_up.set_limit(n);
            }
        }
        let roomish = extra.get("room-password").is_some()
            || extra.get("enable-room-share").is_some()
            || extra.get("room-listen-port").is_some()
            || extra.get("room-udp-port").is_some()
            || extra.get("room-name").is_some()
            || extra.get("dir").is_some();
        self.opts.lock().await.merge(&extra);
        if roomish {
            self.ensure_room_share().await;
        }
        Ok("OK".into())
    }

    pub async fn ensure_room_share(self: &Arc<Self>) {
        let o = self.opts.lock().await.clone();
        let pw = o.get("room-password").unwrap_or("").to_string();
        let on = o.bool("enable-room-share", true) && !pw.is_empty();
        let mut slot = self.room.lock().await;
        if !on {
            if let Some(h) = slot.take() {
                h.stop();
            }
            return;
        }
        if let Some(h) = slot.as_ref() {
            if h.password() == pw && h.dir() == o.dir() {
                return;
            }
            h.stop();
        }
        match crate::room::RoomHub::start(&o).await {
            Ok(h) => *slot = Some(h),
            Err(e) => tracing::warn!("room-share: {e}"),
        }
    }

    pub async fn tell_room_peers(&self) -> Vec<crate::room::RoomPeer> {
        let g = self.room.lock().await;
        match g.as_ref() {
            Some(h) => h.peers().await,
            None => Vec::new(),
        }
    }

    pub async fn get_room_files(&self) -> Vec<crate::room::FileOffer> {
        let g = self.room.lock().await;
        match g.as_ref() {
            Some(h) => h.local_files(),
            None => {
                drop(g);
                let o = self.opts.lock().await;
                crate::room::list_files(&o.dir())
            }
        }
    }

    pub async fn copy_from_room(
        self: &Arc<Self>,
        addr: &str,
        port: u16,
        rel: &str,
        deep: bool,
    ) -> Result<Vec<String>> {
        let (pw, url) = {
            let g = self.room.lock().await;
            let h = g.as_ref().ok_or_else(|| Error::Rpc("room share off".into()))?;
            (
                h.password().to_string(),
                h.file_url(addr, port, rel),
            )
        };
        let mut gids = Vec::new();
        if deep {
            let ctrl_rel = format!("{rel}.aria2");
            let ctrl_url = {
                let g = self.room.lock().await;
                let h = g.as_ref().ok_or_else(|| Error::Rpc("room share off".into()))?;
                h.file_url(addr, port, &ctrl_rel)
            };
            let mut extra = OptionSet::new();
            extra.set("header", format!("X-Room-Password: {pw}"));
            extra.set(
                "out",
                format!("{}.aria2", rel.rsplit('/').next().unwrap_or(rel)),
            );
            extra.set("file-allocation", "none");
            extra.set("allow-overwrite", "true");
            extra.set("split", "1");
            extra.set("check-certificate", "false");
            if let Ok(gid) = self.add_uri_and_start(vec![ctrl_url], extra).await {
                gids.push(gid.as_str().to_string());
            }
        }
        let mut extra = OptionSet::new();
        extra.set("header", format!("X-Room-Password: {pw}"));
        extra.set("out", rel.rsplit('/').next().unwrap_or(rel));
        extra.set("file-allocation", "none");
        extra.set("allow-overwrite", "true");
        extra.set("split", "4");
        extra.set("max-connection-per-server", "4");
        extra.set("min-split-size", "1M");
        extra.set("check-certificate", "false");
        if deep {
            extra.set("continue", "true");
        }
        let gid = self.add_uri_and_start(vec![url], extra).await?;
        gids.push(gid.as_str().to_string());
        Ok(gids)
    }

    pub fn get_session_info(&self) -> Value {
        json!({ "sessionId": self.session_id })
    }

    pub async fn save_session(&self) -> Result<String> {
        let (path, force, save_nf) = {
            let o = self.opts.lock().await;
            let path = o
                .get("save-session")
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::Rpc("save-session is not configured".into()))?
                .to_string();
            (
                path,
                o.bool("force-save", false),
                o.bool("save-not-found", true),
            )
        };
        let g = self.tasks.lock().await;
        let order = self.order.lock().await;
        let mut out = String::new();
        for gid in order.iter() {
            let Some(t) = g.get(gid) else {
                continue;
            };
            if t.status == "removed" {
                continue;
            }
            let t_force = force || t.opts.bool("force-save", false);
            let t_nf = save_nf && t.opts.bool("save-not-found", true);
            if t.status == "complete" && !t_force {
                continue;
            }
            if t.status == "error" && !t_force && !t_nf {
                continue;
            }
            out.push_str(&serialize_task(t));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        drop(order);
        drop(g);
        crate::storage::write_file(std::path::Path::new(&path), out.as_bytes())?;
        Ok("OK".into())
    }

    pub async fn load_input_file(self: &Arc<Self>) -> Result<usize> {
        let (path, deferred, save_session) = {
            let o = self.opts.lock().await;
            (
                o.get("input-file")
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                o.bool("deferred-input", false),
                o.get("save-session").is_some_and(|s| !s.is_empty()),
            )
        };
        let Some(path) = path else {
            return Ok(0);
        };
        let text = String::from_utf8_lossy(&crate::storage::read_file(std::path::Path::new(&path))?)
            .into_owned();
        let entries = parse_input_file(&text)?;
        let use_deferred = deferred && !save_session;
        let mut n = 0usize;
        if !use_deferred {
            for (uris, extra) in entries {
                if uris.is_empty() {
                    continue;
                }
                self.add_uri_and_start(uris, extra).await?;
                n += 1;
            }
            return Ok(n);
        }
        let max = self.max_concurrent().await;
        let mut rest = VecDeque::new();
        for (uris, extra) in entries {
            if uris.is_empty() {
                continue;
            }
            if n < max {
                self.add_uri_and_start(uris, extra).await?;
                n += 1;
            } else {
                rest.push_back((uris, extra));
            }
        }
        *self.deferred.lock().await = rest;
        Ok(n)
    }

    pub async fn has_deferred(&self) -> bool {
        !self.deferred.lock().await.is_empty()
    }

    pub async fn get_files(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        Ok(json!(files_json(t)))
    }

    pub async fn get_uris(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        Ok(json!(uris_json(t)))
    }

    pub async fn get_peers(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        let peers = t.progress.peers.lock().unwrap();
        Ok(json!(peers
            .iter()
            .map(|p| json!({
                "peerId": p.peer_id,
                "ip": p.ip,
                "port": p.port.to_string(),
                "bitfield": p.bitfield,
                "amChoking": if p.am_choking { "true" } else { "false" },
                "peerChoking": if p.peer_choking { "true" } else { "false" },
                "seeder": if p.seeder { "true" } else { "false" },
                "downloadSpeed": "0",
                "uploadSpeed": "0",
            }))
            .collect::<Vec<_>>()))
    }

    pub async fn get_servers(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        let servers = t.progress.servers.lock().unwrap();
        let mut by_index: std::collections::BTreeMap<u32, Vec<Value>> = std::collections::BTreeMap::new();
        for s in servers.iter() {
            by_index.entry(s.index).or_default().push(json!({
                "uri": s.uri,
                "currentUri": s.current_uri,
                "downloadSpeed": "0",
            }));
        }
        Ok(json!(by_index
            .into_iter()
            .map(|(idx, servers)| json!({
                "index": idx.to_string(),
                "servers": servers,
            }))
            .collect::<Vec<_>>()))
    }

    /// C++ `aria2.changeUri(gid, fileIndex, delUris, addUris[, position])`.
    pub async fn change_uri(
        &self,
        gid: &str,
        file_index: i64,
        del: Vec<String>,
        add: Vec<String>,
        position: Option<i64>,
    ) -> Result<(i64, i64)> {
        if file_index < 1 {
            return Err(Error::Rpc("fileIndex must be >= 1".into()));
        }
        let mut g = self.tasks.lock().await;
        let t = g.get_mut(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        let nfiles = t
            .torrent
            .as_deref()
            .and_then(|b| crate::bt::MetaInfo::from_torrent(b).ok())
            .map(|m| m.files.len() as i64)
            .unwrap_or(1);
        if file_index > nfiles {
            return Err(Error::Rpc("No such file".into()));
        }
        let mut deleted = 0i64;
        for d in &del {
            while let Some(i) = t.uris.iter().position(|u| u == d) {
                t.uris.remove(i);
                deleted += 1;
            }
        }
        let mut at = match position {
            Some(p) if p < 0 => t.uris.len(),
            Some(p) => (p as usize).min(t.uris.len()),
            None => t.uris.len(),
        };
        let mut added = 0i64;
        for a in add {
            t.uris.insert(at, a);
            at += 1;
            added += 1;
        }
        Ok((deleted, added))
    }

    pub async fn tell_status(&self, gid: &str) -> Result<Value> {
        let g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        Ok(task_json(t))
    }

    pub async fn tell_active(&self) -> Vec<Value> {
        let g = self.tasks.lock().await;
        g.values()
            .filter(|t| t.status == "active")
            .map(task_json)
            .collect()
    }

    pub async fn tell_waiting(&self, _offset: i64, _num: i64) -> Vec<Value> {
        let g = self.tasks.lock().await;
        let order = self.order.lock().await;
        order
            .iter()
            .filter_map(|id| g.get(id))
            .filter(|t| t.status == "waiting" || t.status == "paused")
            .map(task_json)
            .collect()
    }

    pub async fn tell_stopped(&self, _offset: i64, _num: i64) -> Vec<Value> {
        let g = self.tasks.lock().await;
        g.values()
            .filter(|t| t.status == "complete" || t.status == "error" || t.status == "removed")
            .map(task_json)
            .collect()
    }

    pub async fn remove(self: &Arc<Self>, gid: &str) -> Result<String> {
        self.remove_inner(gid, false).await
    }

    pub async fn force_remove(self: &Arc<Self>, gid: &str) -> Result<String> {
        self.remove_inner(gid, true).await
    }

    async fn remove_inner(self: &Arc<Self>, gid: &str, force: bool) -> Result<String> {
        let mut g = self.tasks.lock().await;
        if let Some(t) = g.get_mut(gid) {
            t.status = "removed".into();
            t.progress.halt.store(true, Ordering::Relaxed);
            let hooks = event_hooks(t, &["on-download-stop"]);
            drop(g);
            if force {
                if let Some(tx) = self.cancels.lock().await.get(gid) {
                    let _ = tx.send(true);
                }
            }
            run_hooks(&hooks);
            kick_waiting(Arc::clone(self));
            prune_stopped(self).await;
            return Ok(gid.to_string());
        }
        Err(Error::Rpc("gid not found".into()))
    }

    pub async fn remove_download_result(&self, gid: &str) -> Result<String> {
        let mut g = self.tasks.lock().await;
        let t = g.get(gid).ok_or_else(|| Error::Rpc("gid not found".into()))?;
        if t.status != "complete" && t.status != "error" && t.status != "removed" {
            return Err(Error::Rpc("download is not stopped".into()));
        }
        g.remove(gid);
        drop(g);
        self.order.lock().await.retain(|id| id != gid);
        Ok("OK".into())
    }

    pub async fn purge_download_result(&self) -> Result<String> {
        let mut g = self.tasks.lock().await;
        let gone: Vec<String> = g
            .values()
            .filter(|t| t.status == "complete" || t.status == "error" || t.status == "removed")
            .map(|t| t.gid.0.clone())
            .collect();
        for id in &gone {
            g.remove(id);
        }
        drop(g);
        self.order.lock().await.retain(|id| !gone.iter().any(|d| d == id));
        Ok("OK".into())
    }

    pub async fn global_stat(&self) -> Value {
        let g = self.tasks.lock().await;
        let num_active = g.values().filter(|t| t.status == "active").count();
        let num_waiting = g
            .values()
            .filter(|t| t.status == "waiting" || t.status == "paused")
            .count();
        let num_stopped = g.values().filter(|t| t.status == "complete" || t.status == "error").count();
        json!({
            "downloadSpeed": "0",
            "uploadSpeed": "0",
            "numActive": num_active.to_string(),
            "numWaiting": num_waiting.to_string(),
            "numStopped": num_stopped.to_string(),
            "numStoppedTotal": num_stopped.to_string(),
        })
    }
}

async fn finish_task(me: Arc<Session>, key: &str, r: Result<()>) {
    let mut hooks: Vec<(String, String, std::path::PathBuf)> = Vec::new();
    {
        let mut g = me.tasks.lock().await;
        if let Some(t) = g.get_mut(key) {
            if t.status == "paused" || t.status == "removed" {
                return;
            }
            match r {
                Ok(()) => {
                    if t.status == "active" {
                        t.status = "complete".into();
                        crate::filelog::write(
                            &t.opts,
                            "INFO",
                            &format!(
                                "Download complete gid={key} path={}",
                                t.dest.display()
                            ),
                        );
                        write_download_result(t, &me.console_out, &me.console_err);
                        hooks = event_hooks(t, &["on-download-complete", "on-download-stop"]);
                    }
                }
                Err(e) if e.to_string().contains("canceled") => {
                    if t.status == "active" {
                        return;
                    }
                    if t.status != "paused" && t.status != "removed" {
                        t.status = "paused".into();
                    }
                }
                Err(e) if e.to_string().contains("pause-metadata:") => {
                    let msg = e.to_string();
                    if let Some(p) = msg.split("pause-metadata:").nth(1) {
                        if let Ok(bytes) = std::fs::read(p) {
                            t.torrent = Some(bytes);
                            t.opts.set("torrent-file", p);
                        }
                    }
                    t.status = "paused".into();
                    hooks = event_hooks(t, &["on-download-pause"]);
                }
                Err(e) => {
                    t.status = "error".into();
                    t.error = Some(e.to_string());
                    crate::filelog::write(
                        &t.opts,
                        "ERROR",
                        &format!("Download failed gid={key} error={e}"),
                    );
                    write_download_result(t, &me.console_out, &me.console_err);
                    hooks = event_hooks(t, &["on-download-error", "on-download-stop"]);
                }
            }
        }
    }
    run_hooks(&hooks);
    prune_stopped(&me).await;
    pull_deferred(&me).await;
    kick_waiting(me);
}

fn write_download_result(
    t: &Task,
    stdout: &std::sync::Mutex<String>,
    stderr: &std::sync::Mutex<String>,
) {
    let mode = t.opts.get("download-result").unwrap_or("default");
    if mode.eq_ignore_ascii_case("hide") {
        return;
    }
    let stat = if t.status == "complete" { "OK" } else { "ERR" };
    let path = t.dest.display();
    let gid = t.gid.as_str();
    let n = t.progress.completed.load(Ordering::Relaxed);
    let size = crate::filelog::format_length(&t.opts, n);
    let mut line = format!("Download Results: {gid}|{stat}|{size}|{path}");
    if mode.eq_ignore_ascii_case("full") {
        let uri = t.uris.first().cloned().unwrap_or_default();
        line.push_str(&format!(" URI={uri}"));
        if let Some(err) = &t.error {
            line.push_str(&format!(" error={err}"));
        }
    }
    crate::filelog::write(&t.opts, "NOTICE", &line);
    crate::filelog::write_console(&t.opts, stdout, stderr, "NOTICE", &line);
}

fn event_hooks(t: &Task, keys: &[&str]) -> Vec<(String, String, std::path::PathBuf)> {
    keys.iter()
        .filter_map(|k| {
            t.opts
                .get(k)
                .filter(|s| !s.is_empty())
                .map(|c| (c.to_string(), t.gid.0.clone(), t.dest.clone()))
        })
        .collect()
}

fn run_hooks(hooks: &[(String, String, std::path::PathBuf)]) {
    for (cmd, gid, dest) in hooks {
        run_event_hook(cmd, gid, dest);
    }
}

fn is_stopped(status: &str) -> bool {
    status == "complete" || status == "error" || status == "removed"
}

async fn prune_stopped(me: &Session) {
    let (max, keep_unfinished) = {
        let o = me.opts.lock().await;
        (
            o.usize("max-download-result", 1000),
            o.bool("keep-unfinished-download-result", true),
        )
    };
    let mut g = me.tasks.lock().await;
    let mut order = me.order.lock().await;
    let stopped: Vec<String> = order
        .iter()
        .filter(|id| g.get(*id).is_some_and(|t| is_stopped(&t.status)))
        .cloned()
        .collect();
    let droppable: Vec<String> = stopped
        .iter()
        .filter(|id| {
            let unfinished = g
                .get(*id)
                .is_some_and(|t| t.status == "error" || t.status == "removed");
            !(keep_unfinished && unfinished)
        })
        .cloned()
        .collect();
    if droppable.len() <= max {
        return;
    }
    let drop_n = droppable.len() - max;
    for id in droppable.into_iter().take(drop_n) {
        g.remove(&id);
        order.retain(|x| x != &id);
    }
}

/// C++ DownloadHandler: COMMAND GID NFILES PATH
fn run_event_hook(cmd: &str, gid: &str, dest: &std::path::Path) {
    let mut parts = cmd.split_whitespace();
    let Some(prog) = parts.next() else {
        return;
    };
    let mut c = std::process::Command::new(prog);
    c.args(parts);
    c.arg(gid);
    c.arg("1");
    c.arg(dest.as_os_str());
    let _ = c.status();
}

async fn pull_deferred(me: &Arc<Session>) {
    loop {
        if me.is_stopped() {
            return;
        }
        let active = me.active_count().await;
        let max = me.max_concurrent().await;
        if active >= max {
            return;
        }
        let next = me.deferred.lock().await.pop_front();
        let Some((uris, extra)) = next else {
            return;
        };
        if uris.is_empty() {
            continue;
        }
        if let Err(e) = me.add_uri_and_start(uris, extra).await {
            tracing::warn!("deferred-input: {e}");
        }
    }
}

fn spawn_lifetime_stop(
    me: &Arc<Session>,
    save_interval: u64,
    stop_secs: u64,
    stop_pid: Option<String>,
) {
    if save_interval > 0 {
        let weak = Arc::downgrade(me);
        tokio::spawn(async move {
            let d = std::time::Duration::from_secs(save_interval);
            loop {
                tokio::time::sleep(d).await;
                let Some(s) = weak.upgrade() else {
                    break;
                };
                if s.is_stopped() {
                    break;
                }
                let _ = s.save_session().await;
            }
        });
    }
    if stop_secs > 0 {
        let weak = Arc::downgrade(me);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(stop_secs)).await;
            if let Some(s) = weak.upgrade() {
                s.stop_application().await;
            }
        });
    }
    if let Some(pid_s) = stop_pid {
        if let Ok(pid) = pid_s.parse::<i32>() {
            let weak = Arc::downgrade(me);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let Some(s) = weak.upgrade() else {
                        return;
                    };
                    if s.is_stopped() {
                        return;
                    }
                    if !proc_alive(pid) {
                        s.stop_application().await;
                        return;
                    }
                }
            });
        }
    }
}

fn proc_alive(pid: i32) -> bool {
    pid > 0 && std::path::Path::new(&format!("/proc/{pid}")).is_dir()
}

fn kick_waiting(me: Arc<Session>) {
    tokio::spawn(async move {
        pull_deferred(&me).await;
        if me.is_stopped() {
            return;
        }
        loop {
            let max = me.max_concurrent().await;
            let next = {
                let g = me.tasks.lock().await;
                let order = me.order.lock().await;
                let detach = me.opts.lock().await.bool("bt-detach-seed-only", false);
                let active = g
                    .values()
                    .filter(|t| {
                        t.status == "active"
                            && !(detach && t.progress.seeding.load(Ordering::SeqCst))
                    })
                    .count();
                if active >= max {
                    return;
                }
                order
                    .iter()
                    .find(|id| g.get(*id).is_some_and(|t| t.status == "waiting"))
                    .cloned()
            };
            let Some(id) = next else {
                return;
            };
            {
                let mut g = me.tasks.lock().await;
                if let Some(t) = g.get_mut(&id) {
                    if t.status != "waiting" {
                        continue;
                    }
                    t.status = "active".into();
                } else {
                    return;
                }
            }
            launch_transfer(Arc::clone(&me), id);
        }
    });
}

fn spawn_summary_ticker(me: Arc<Session>, key: String, secs: u64) {
    if secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let d = std::time::Duration::from_secs(secs);
        loop {
            tokio::time::sleep(d).await;
            let g = me.tasks.lock().await;
            let Some(t) = g.get(&key) else {
                break;
            };
            if t.status != "active" {
                break;
            }
            let done = t.progress.completed.load(Ordering::Relaxed);
            let total = t.progress.total.load(Ordering::Relaxed);
            let done_s = crate::filelog::format_length(&t.opts, done);
            let total_s = crate::filelog::format_length(&t.opts, total);
            crate::filelog::write(
                &t.opts,
                "NOTICE",
                &format!(
                    "SUMMARY gid={} completedLength={done_s} totalLength={total_s} path={}",
                    t.gid.as_str(),
                    t.dest.display()
                ),
            );
            if t.opts.bool("show-console-readout", true) {
                let readout = crate::filelog::truncate_readout(
                    &t.opts,
                    &format!(
                        "SUMMARY gid={} completedLength={done_s} totalLength={total_s} path={}",
                        t.gid.as_str(),
                        t.dest.display()
                    ),
                );
                crate::filelog::write_console(
                    &t.opts,
                    &me.console_out,
                    &me.console_err,
                    "NOTICE",
                    &readout,
                );
            }
        }
    });
}

fn launch_transfer(me: Arc<Session>, key: String) {
    tokio::spawn(async move {
        launch_transfer_inner(me, key).await;
    });
}

async fn launch_transfer_inner(me: Arc<Session>, key: String) {
    if me.is_stopped() {
        return;
    }
    let wait = me.idle_until.saturating_duration_since(tokio::time::Instant::now());
    if !wait.is_zero() {
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = me.wait_stopped() => return,
        }
        if me.is_stopped() {
            return;
        }
    }
    let (uris, dest, opts, progress, torrent) = {
        let g = me.tasks.lock().await;
        let Some(t) = g.get(&key) else {
            return;
        };
        (
            t.uris.clone(),
            t.dest.clone(),
            t.opts.clone(),
            t.progress.clone(),
            t.torrent.clone(),
        )
    };
    if let Some(cmd) = opts.get("on-download-start").filter(|s| !s.is_empty()) {
        run_event_hook(cmd, &key, &dest);
    }
    crate::filelog::write(
        &opts,
        "INFO",
        &format!(
            "Download started gid={key} uri={}",
            uris.first().cloned().unwrap_or_default()
        ),
    );
    spawn_summary_ticker(Arc::clone(&me), key.clone(), opts.u64("summary-interval", 60));
    let (tx, rx) = watch::channel(false);
    me.cancels.lock().await.insert(key.clone(), tx);
    let piece_length = opts.piece_length();
    if torrent.is_some() || crate::bt::looks_like_bt(&uris, &opts) {
        if !opts.bool("enable-bittorrent", true) {
            finish_task(
                Arc::clone(&me),
                &key,
                Err(crate::error::Error::Bt("enable-bittorrent=false".into())),
            )
            .await;
            return;
        }
        let mut opts = opts;
        opts.set("gid", key.clone());
        let job_uris = uris;
        let progress_seed = progress.clone();
        let me_seed = Arc::clone(&me);
        let key_seed = key.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                if me_seed.is_stopped() {
                    return;
                }
                let status = {
                    let g = me_seed.tasks.lock().await;
                    g.get(&key_seed).map(|t| t.status.clone())
                };
                let Some(st) = status else {
                    return;
                };
                if st != "active" {
                    return;
                }
                if progress_seed.seeding.load(Ordering::SeqCst) {
                    kick_waiting(me_seed);
                    return;
                }
            }
        });
        tokio::spawn(async move {
            let r = async {
                let torrent = match torrent {
                    Some(b) if !b.is_empty() => b,
                    _ => crate::bt::load_torrent_bytes(&job_uris, &opts)?,
                };
                let peers = crate::bt::collect_peers(&job_uris);
                crate::bt::download(crate::bt::BtJob {
                    torrent,
                    dest,
                    peers,
                    opts,
                    progress,
                    cancel: rx,
                })
                .await
            }
            .await;
            finish_task(Arc::clone(&me), &key, r).await;
        });
    } else if uris.first().map(|u| u.starts_with("sftp://")).unwrap_or(false) {
        #[cfg(feature = "sftp")]
        {
            let job = crate::sftp::SftpJob {
                uris,
                dest,
                opts,
                progress,
                cancel: rx,
            };
            tokio::spawn(async move {
                let r = crate::sftp::download(job).await;
                finish_task(Arc::clone(&me), &key, r).await;
            });
        }
        #[cfg(not(feature = "sftp"))]
        tokio::spawn(async move {
            finish_task(
                Arc::clone(&me),
                &key,
                Err(Error::Sftp("SFTP support is not enabled in this build".into())),
            )
            .await;
        });
    } else if uris.first().map(|u| u.starts_with("ftp://")).unwrap_or(false) {
        let job = crate::ftp::FtpJob {
            uris,
            dest,
            opts,
            progress,
            cancel: rx,
        };
        tokio::spawn(async move {
            let r = match crate::ftp::download(job).await {
                Ok(()) => {
                    let latest = me
                        .tasks
                        .lock()
                        .await
                        .get(&key)
                        .map(|t| (t.dest.clone(), t.opts.clone()));
                    if let Some((dest, opts)) = latest {
                        crate::checksum::verify_dest(&dest, &opts).await
                    } else {
                        Ok(())
                    }
                }
                Err(e) => Err(e),
            };
            finish_task(Arc::clone(&me), &key, r).await;
        });
    } else {
        let dest_http = dest.clone();
        let uris_f = uris.clone();
        let opts_f = opts.clone();
        let progress_f = progress.clone();
        let rx_bt = rx.clone();
        let job = HttpJob {
            uris,
            dest,
            opts,
            progress,
            piece_length,
            cancel: rx,
        };
        tokio::spawn(async move {
            let r = match http::download(job).await {
                Ok(()) => {
                    let latest = me
                        .tasks
                        .lock()
                        .await
                        .get(&key)
                        .map(|t| t.opts.clone());
                    let v = if let Some(opts) = latest {
                        crate::checksum::verify_dest(&dest_http, &opts).await
                    } else {
                        Ok(())
                    };
                    match v {
                        Ok(()) => {
                            match follow_http_torrent(
                                &dest_http,
                                &uris_f,
                                &opts_f,
                                progress_f.clone(),
                                rx_bt.clone(),
                            )
                            .await
                            {
                                Ok(()) => {
                                    follow_http_metalink(
                                        &dest_http,
                                        &uris_f,
                                        &opts_f,
                                        progress_f,
                                        rx_bt,
                                    )
                                    .await
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                Err(e) => Err(e),
            };
            finish_task(Arc::clone(&me), &key, r).await;
        });
    }
}

/// C++ `--follow-torrent=true|false|mem`: after HTTP GET of a .torrent, start BT.
async fn follow_http_torrent(
    torrent_dest: &PathBuf,
    uris: &[String],
    opts: &OptionSet,
    progress: crate::http::HttpProgress,
    cancel: watch::Receiver<bool>,
) -> Result<()> {
    let mode = opts.get("follow-torrent").unwrap_or("true");
    let mode = match mode {
        "false" | "0" => "false",
        "mem" => "mem",
        _ => "true",
    };
    if mode == "false" {
        return Ok(());
    }
    if !opts.bool("enable-bittorrent", true) {
        return Ok(());
    }
    let remote = uris.iter().any(|u| {
        crate::bt::is_torrent_path(u) && (u.starts_with("http://") || u.starts_with("https://"))
    });
    let suffix = torrent_dest
        .extension()
        .is_some_and(|e| e == "torrent");
    if !remote && !suffix {
        return Ok(());
    }
    let bytes = crate::storage::read_file(torrent_dest)?;
    let meta = match crate::bt::MetaInfo::from_torrent(&bytes) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if mode == "mem" {
        let _ = crate::storage::unlink(torrent_dest);
    }
    let payload = opts.dir().join(&meta.name);
    crate::bt::download(crate::bt::BtJob {
        torrent: bytes,
        dest: payload,
        peers: crate::bt::collect_peers(uris),
        opts: opts.clone(),
        progress,
        cancel,
    })
    .await
}

fn bt_dest(uris: &[String], opts: &OptionSet) -> PathBuf {
    if let Some(o) = opts.out() {
        return opts.dir().join(o);
    }
    if let Ok(bytes) = crate::bt::load_torrent_bytes(uris, opts) {
        if !bytes.is_empty() {
            if let Ok(meta) = crate::bt::MetaInfo::from_torrent(&bytes) {
                if !meta.name.is_empty() {
                    return opts.dir().join(&meta.name);
                }
            }
        }
    }
    if let Some(dn) = uris
        .iter()
        .find(|u| u.starts_with("magnet:"))
        .and_then(|u| crate::bt::magnet_dn(u))
    {
        return opts.dir().join(dn);
    }
    dest_for(uris.first().map(String::as_str).unwrap_or(""), opts)
}

/// C++ `--follow-metalink=true|false|mem`: after HTTP GET of `.meta4`/`.metalink`, fetch payload.
async fn follow_http_metalink(
    metalink_dest: &PathBuf,
    uris: &[String],
    opts: &OptionSet,
    progress: crate::http::HttpProgress,
    cancel: watch::Receiver<bool>,
) -> Result<()> {
    let mode = opts.get("follow-metalink").unwrap_or("true");
    let mode = match mode {
        "false" | "0" => "false",
        "mem" => "mem",
        _ => "true",
    };
    if mode == "false" {
        return Ok(());
    }
    if !opts.bool("enable-metalink", true) {
        return Ok(());
    }
    let remote = uris.iter().any(|u| {
        crate::metalink::looks_like_metalink_uri(u)
            && (u.starts_with("http://") || u.starts_with("https://"))
    });
    let suffix = metalink_dest.extension().is_some_and(|e| {
        e == "meta4" || e == "metalink"
    });
    if !remote && !suffix {
        return Ok(());
    }
    let bytes = crate::storage::read_file(metalink_dest)?;
    let files = match crate::metalink::parse_bytes(&bytes) {
        Ok(f) => crate::metalink::filter_files(f, opts),
        Err(_) => return Ok(()),
    };
    let Some(f) = files.into_iter().next() else {
        return Ok(());
    };
    let urls = f.select_urls(opts);
    if urls.is_empty() {
        return Ok(());
    }
    if mode == "mem" {
        let _ = crate::storage::unlink(metalink_dest);
    }
    let name = std::path::Path::new(&f.name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| f.name.clone());
    let dest = opts.dir().join(name);
    let mut job_opts = opts.clone();
    if let Some(cs) = f.checksum_spec() {
        job_opts.set("checksum", cs);
    }
    if let Some(ps) = f.piece_checksum_spec() {
        job_opts.set("piece-checksum", ps);
    }
    crate::http::download(HttpJob {
        uris: urls,
        dest,
        opts: job_opts,
        progress,
        piece_length: opts.piece_length(),
        cancel,
    })
    .await
}

fn dest_for(uri: &str, opts: &OptionSet) -> PathBuf {
    let dir = opts.dir();
    if let Some(name) = opts.out() {
        return dir.join(name);
    }
    let name = url::Url::parse(uri)
        .ok()
        .and_then(|u| {
            let p = u.path();
            std::path::Path::new(p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "index".into());
    let dest = dir.join(name);
    if opts.bool("conditional-get", false) {
        dest
    } else {
        crate::http::resolve_existing_dest(dest.clone(), opts).unwrap_or(dest)
    }
}

fn option_map_json(opts: &OptionSet) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in &opts.map {
        m.insert(k.clone(), json!(v));
    }
    Value::Object(m)
}

fn task_json(t: &Task) -> Value {
    let total = t.progress.total.load(Ordering::Relaxed);
    let completed = t.progress.completed.load(Ordering::Relaxed);
    json!({
        "gid": t.gid.as_str(),
        "status": t.status,
        "totalLength": total.to_string(),
        "completedLength": completed.to_string(),
        "uploadLength": "0",
        "downloadSpeed": "0",
        "uploadSpeed": "0",
        "connections": "1",
        "errorCode": if t.status == "error" { "1" } else { "0" },
        "errorMessage": t.error.clone().unwrap_or_default(),
        "dir": t.dest.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        "files": files_json(t),
    })
}

fn uris_json(t: &Task) -> Vec<Value> {
    t.uris
        .iter()
        .enumerate()
        .map(|(i, u)| {
            json!({
                "uri": u,
                "status": if i == 0 { "used" } else { "waiting" },
            })
        })
        .collect()
}

fn file_on_disk(path: &PathBuf) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn files_json(t: &Task) -> Vec<Value> {
    let uris = uris_json(t);
    let bytes = t.torrent.clone().or_else(|| {
        crate::bt::load_torrent_bytes(&t.uris, &t.opts)
            .ok()
            .filter(|b| !b.is_empty())
    });
    if let Some(bytes) = bytes {
        if let Ok(meta) = crate::bt::MetaInfo::from_torrent(&bytes) {
            let sel = crate::bt::parse_select_file(t.opts.get("select-file"), meta.files.len());
            return meta
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let path = if meta.is_multi() {
                        t.dest.join(&f.path)
                    } else {
                        t.dest.clone()
                    };
                    let on_disk = file_on_disk(&path);
                    let selected = sel.get(i).copied().unwrap_or(true);
                    json!({
                        "index": (i + 1).to_string(),
                        "path": path.display().to_string(),
                        "length": f.length.to_string(),
                        "completedLength": if selected { on_disk.min(f.length) } else { 0 }.to_string(),
                        "selected": if selected { "true" } else { "false" },
                        "uris": uris.clone(),
                    })
                })
                .collect();
        }
    }
    let total = t.progress.total.load(Ordering::Relaxed);
    let completed = t.progress.completed.load(Ordering::Relaxed);
    let on_disk = file_on_disk(&t.dest);
    let length = if total > 0 { total } else { on_disk };
    let done = if on_disk > 0 { on_disk.min(length) } else { completed.min(length) };
    vec![json!({
        "index": "1",
        "path": t.dest.display().to_string(),
        "length": length.to_string(),
        "completedLength": done.to_string(),
        "selected": "true",
        "uris": uris,
    })]
}

fn serialize_task(t: &Task) -> String {
    let mut s = String::new();
    for u in &t.uris {
        s.push_str(u);
        s.push('\n');
    }
    s.push_str("  gid=");
    s.push_str(t.gid.as_str());
    s.push('\n');
    if t.status == "paused" || t.status == "waiting" {
        s.push_str("  pause=true\n");
    }
    let mut keys: Vec<&String> = t.opts.map.keys().collect();
    keys.sort();
    for k in keys {
        if k == "magnet" || k == "gid" || k == "pause" {
            continue;
        }
        if let Some(v) = t.opts.get(k) {
            s.push_str("  ");
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push('\n');
        }
    }
    s
}

/// C++ aria2 input-file / session file: URI lines, then indented `key=value`.
pub fn parse_input_file(text: &str) -> Result<Vec<(Vec<String>, OptionSet)>> {
    let mut entries: Vec<(Vec<String>, OptionSet)> = Vec::new();
    let mut uris: Vec<String> = Vec::new();
    let mut opts = OptionSet::new();
    let mut saw_opts = false;
    let flush = |uris: &mut Vec<String>, opts: &mut OptionSet, entries: &mut Vec<(Vec<String>, OptionSet)>, saw_opts: &mut bool| {
        if !uris.is_empty() {
            entries.push((std::mem::take(uris), std::mem::replace(opts, OptionSet::new())));
        } else {
            *opts = OptionSet::new();
        }
        *saw_opts = false;
    };
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            let rest = line.trim();
            if let Some((k, v)) = rest.split_once('=') {
                opts.set(k.trim(), v.trim());
                saw_opts = true;
            }
            continue;
        }
        if saw_opts {
            flush(&mut uris, &mut opts, &mut entries, &mut saw_opts);
        }
        uris.push(line.to_string());
    }
    flush(&mut uris, &mut opts, &mut entries, &mut saw_opts);
    Ok(entries)
}

fn metalink_ext(bytes: &[u8]) -> &'static str {
    let s = String::from_utf8_lossy(bytes);
    if s.contains("urn:ietf:params:xml:ns:metalink") {
        "meta4"
    } else {
        "metalink"
    }
}

fn save_rpc_upload(opts: &OptionSet, bytes: &[u8], ext: &str) -> Result<()> {
    if !opts.bool("rpc-save-upload-metadata", true) || bytes.is_empty() {
        return Ok(());
    }
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(bytes);
    let name = format!("{:x}.{ext}", h.finalize());
    let dir = opts.dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_two_downloads() {
        let text = "http://a/1\nhttp://a/2\n  out=x\nhttp://b/3\n  out=y\n  pause=true\n";
        let e = parse_input_file(text).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].0, vec!["http://a/1", "http://a/2"]);
        assert_eq!(e[0].1.get("out"), Some("x"));
        assert_eq!(e[1].0, vec!["http://b/3"]);
        assert_eq!(e[1].1.get("out"), Some("y"));
        assert!(e[1].1.bool("pause", false));
    }
}
