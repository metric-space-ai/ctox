//! Core event hooks: immutable CAS versions, stream triggers and task completions.
//! Calls run outside the writer. Core owns feed delivery, timers and real leases.
use crate::core_host::CoreRuntimeHost;
use crate::{AppBundle, RuntimePool};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub seq: u64,
    pub collection: String,
    pub op: String,
    pub record_id: String,
    // Deliberately no record body: masked data is read via ctx.get/read.
}
#[derive(Clone)]
struct ActiveRelease {
    version: String,
    manifest: Value,
}
pub struct AppRuntimeService {
    pool: RuntimePool,
    host: Arc<CoreRuntimeHost>,
    active: Mutex<BTreeMap<String, ActiveRelease>>,
    loaded: Mutex<BTreeMap<String, String>>,
    apps: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}
impl AppRuntimeService {
    pub fn production(root: PathBuf, host: Arc<CoreRuntimeHost>) -> Result<Self> {
        let pool = RuntimePool::production(root, host.clone())?;
        Ok(Self::with_pool(pool, host))
    }
    pub(crate) fn with_pool(pool: RuntimePool, host: Arc<CoreRuntimeHost>) -> Self {
        Self {
            pool,
            host,
            active: Mutex::new(BTreeMap::new()),
            loaded: Mutex::new(BTreeMap::new()),
            apps: Mutex::new(BTreeMap::new()),
        }
    }
    fn app_lock(&self, app: &str) -> Result<Arc<Mutex<()>>> {
        Ok(self
            .apps
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable: app lock"))?
            .entry(app.into())
            .or_default()
            .clone())
    }
    fn bundle(&self, app: &str, version: &str) -> Result<AppBundle> {
        crate::delivery::load_release(
            self.host.as_ref(),
            app,
            version,
            self.host.authority.grants(app, version)?,
        )
    }
    fn load(&self, app: &str, version: &str) -> Result<()> {
        if self
            .loaded
            .lock()
            .unwrap()
            .get(app)
            .is_some_and(|v| v == version)
        {
            return Ok(());
        }
        self.pool.register(self.bundle(app, version)?)?;
        self.loaded
            .lock()
            .unwrap()
            .insert(app.into(), version.into());
        Ok(())
    }
    /// Invoke after the Core publish transaction commits, or during startup
    /// from current business_apps.active_ver. No artifact filesystem access.
    pub fn published(&self, app: &str, version: &str) -> Result<()> {
        let lock = self.app_lock(app)?;
        let _guard = lock
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable: app lock"))?;
        let bundle = self.bundle(app, version)?;
        let manifest = bundle.manifest.clone();
        validate_triggers(&manifest)?;
        self.pool.register(bundle)?; // drains the running call before restart
        self.loaded
            .lock()
            .unwrap()
            .insert(app.into(), version.into());
        self.active.lock().unwrap().insert(
            app.into(),
            ActiveRelease {
                version: version.into(),
                manifest,
            },
        );
        Ok(())
    }
    pub fn removed(&self, app: &str) -> Result<()> {
        // Revocation bypasses the per-app execution lock so an owned call is killed.
        self.pool.unregister(app)?;
        self.loaded.lock().unwrap().remove(app);
        self.active.lock().unwrap().remove(app);
        Ok(())
    }
    fn at_version<T>(
        &self,
        app: &str,
        version: &str,
        run: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let lock = self.app_lock(app)?;
        let _guard = lock
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable: app lock"))?;
        self.load(app, version)?;
        let result = run();
        // Pending task callbacks stay bound to their original immutable version.
        // One process per app: drain/swap to old code, run, then restore current code.
        let active = self.active.lock().unwrap().get(app).cloned();
        if let Some(active) = active.filter(|v| v.version != version) {
            if let Err(error) = self.load(app, &active.version) {
                let _=self.host.authority.log(&json!({"app_id":app,"app_ver":version}),&json!(["error","Cannot restore current handler generation",{"code":crate::error_code(&error.to_string())}]));
            }
        }
        result
    }
    pub fn command(&self, command: Value, fence: Value) -> Result<Value> {
        let app = crate::string(&command, "app_id")?.to_owned();
        let version = crate::string(&command, "app_ver")?.to_owned();
        self.at_version(&app, &version, || self.pool.dispatch(command, fence))
    }
    /// Read-only migration proposals from the admitted CAS release. Core
    /// commits them separately in its schema/generation writer transaction.
    pub fn migrate(
        &self,
        command: Value,
        fence: Value,
        collection: &str,
        docs: Vec<Value>,
    ) -> Result<Value> {
        let app = crate::string(&command, "app_id")?.to_owned();
        let version = crate::string(&command, "app_ver")?.to_owned();
        self.at_version(&app, &version, || {
            self.pool
                .dispatch_migration(command, fence, collection, docs)
        })
    }
    /// Feed this from the ordered Store _changes stream, never table polling.
    /// Core stream cursor replay is safe: trigger.claim is durable and fenced.
    pub fn change(&self, event: &ChangeEvent) -> Result<Vec<Value>> {
        ensure!(
            event.seq > 0 && matches!(event.op.as_str(), "insert" | "update" | "delete"),
            "invalid: change event"
        );
        let active = self.active.lock().unwrap().clone();
        let mut results = Vec::new();
        for (app, release) in active {
            for spec in release.manifest["triggers"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if spec["on"]["collection"] != event.collection || spec["on"]["op"] != event.op {
                    continue;
                }
                if release.manifest["collections"]
                    .get(&event.collection)
                    .is_none()
                {
                    let rule = &release.manifest["foreign_collections"][&event.collection];
                    ensure!(
                        rule["ops"]
                            .as_array()
                            .is_some_and(|ops| ops.iter().any(|v| v == "watch"))
                            && self.host.authority.exported(
                                crate::string(rule, "owner_app")?,
                                &event.collection,
                                "watch"
                            )?,
                        "forbidden: foreign watch withdrawn"
                    );
                }
                results.push(self.trigger(&app,&release,spec,json!({"kind":"change","seq":event.seq,"collection":event.collection,"op":event.op,"record_id":event.record_id}))?);
            }
        }
        Ok(results)
    }
    /// Called by the Core timer at >= PT1M. Due/cursor state is persisted by
    /// trigger.claim, not guessed from process startup time.
    pub fn tick(&self) -> Result<Vec<Value>> {
        let core_now_ms = self.host.authority.now_ms();
        let active = self.active.lock().unwrap().clone();
        let mut results = Vec::new();
        for (app, release) in active {
            for spec in release.manifest["triggers"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(every) = spec["every"].as_str() {
                    let interval = interval_ms(every)?;
                    results.push(self.trigger(
                        &app,
                        &release,
                        spec,
                        json!({"kind":"timer","interval_ms":interval,"now_ms":core_now_ms}),
                    )?);
                }
            }
        }
        Ok(results)
    }
    fn trigger(
        &self,
        app: &str,
        release: &ActiveRelease,
        spec: &Value,
        event: Value,
    ) -> Result<Value> {
        let id = format!("{app}:{:x}", Sha256::digest(serde_json::to_vec(spec)?));
        let mut binding = self.host.authority.event_binding(
            app,
            &release.version,
            &json!({"kind":"trigger","trigger_id":id,"event":event}),
        )?;
        binding["manifest"] = release.manifest.clone();
        binding["trigger_id"] = json!(id);
        binding["command"]["client_context"]["trigger_id"] = json!(id);
        binding["command"]["payload"] = event.clone();
        self.host.authority.admit(&release.manifest, &binding)?;
        let claim = self.host.platform(
            &binding,
            "trigger.claim",
            &json!({"trigger_id":id,"event":event}),
        )?;
        if claim["claimed"] != true {
            return Ok(json!({"status":"skipped","trigger_id":id}));
        }
        let command = binding["command"].clone();
        let fence = binding["fence"].clone();
        let entry = crate::string(spec, "handler")?.to_owned();
        let result = self.at_version(app, &release.version, || {
            self.pool
                .dispatch_entry(command.clone(), fence, &entry, "trigger", command)
        });
        let outcome = match &result {
            Ok(value) => json!({"status":"completed","result":value}),
            Err(error) => json!({"status":"failed","code":crate::error_code(&error.to_string())}),
        };
        self.host.platform(
            &binding,
            "trigger.finish",
            &json!({"trigger_id":id,"event":event,"outcome":outcome}),
        )?;
        result
    }
    /// Core supplies the stored task binding, not an app-provided callback path.
    /// claim is one durable CAS per task; failures/uncertain calls are not replayed.
    pub fn completed(
        &self,
        mut binding: Value,
        task_id: &str,
        succeeded: bool,
        outcome: Value,
    ) -> Result<Value> {
        let app = crate::string(&binding["command"], "app_id")?.to_owned();
        let version = crate::string(&binding["command"], "app_ver")?.to_owned();
        binding["manifest"] = self.bundle(&app, &version)?.manifest;
        self.host.authority.admit(&binding["manifest"], &binding)?;
        let receipt = self.host.platform(
            &binding,
            "completion.claim",
            &json!({"task_id":task_id,"succeeded":succeeded}),
        )?;
        if receipt["claimed"] != true {
            return Ok(json!({"status":"skipped","task_id":task_id}));
        }
        let entry = crate::string(&receipt, "entry")?.to_owned();
        crate::reference_file(&entry)?;
        let result = self.at_version(&app, &version, || {
            self.pool.dispatch_entry(
                binding["command"].clone(),
                binding["fence"].clone(),
                &entry,
                "completion",
                outcome,
            )
        });
        let settled = match &result {
            Ok(value) => json!({"status":"completed","result":value}),
            Err(error) => json!({"status":"failed","code":crate::error_code(&error.to_string())}),
        };
        self.host.platform(
            &binding,
            "completion.finish",
            &json!({"task_id":task_id,"outcome":settled}),
        )?;
        result
    }
    pub fn process_id(&self, app: &str) -> Option<u32> {
        self.pool.process_id(app)
    }
}
pub fn interval_ms(value: &str) -> Result<u64> {
    ensure!(value.starts_with('P'), "invalid: ISO interval");
    let mut time = false;
    let mut number = String::new();
    let mut total = 0f64;
    let mut previous = 0;
    let mut parts = 0;
    for c in value[1..].chars() {
        if c == 'T' {
            ensure!(!time && number.is_empty(), "invalid: ISO interval");
            time = true;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            number.push(c);
            continue;
        }
        let (order, multiplier) = match (c, time) {
            ('D', false) => (1, 86400000.),
            ('H', true) => (2, 3600000.),
            ('M', true) => (3, 60000.),
            ('S', true) => (4, 1000.),
            _ => anyhow::bail!("invalid: ISO interval"),
        };
        ensure!(order > previous, "invalid: ISO interval order");
        previous = order;
        parts += 1;
        total += number
            .parse::<f64>()
            .context("invalid: ISO interval number")?
            * multiplier;
        number.clear();
    }
    ensure!(
        parts > 0
            && number.is_empty()
            && total.is_finite()
            && total >= 60000.
            && total.fract() == 0.
            && total <= 9_007_199_254_740_991.,
        "invalid: trigger interval >= PT1M"
    );
    Ok(total as u64)
}
pub fn validate_triggers(manifest: &Value) -> Result<()> {
    if let Some(triggers) = manifest.get("triggers") {
        for trigger in triggers.as_array().context("invalid: triggers")? {
            let entry = crate::string(trigger, "handler")?;
            crate::reference_file(entry)?;
            ensure!(
                trigger.get("on").is_some() != trigger.get("every").is_some(),
                "invalid: one trigger source required"
            );
            if let Some(every) = trigger["every"].as_str() {
                interval_ms(every)?;
            } else {
                let name = crate::string(&trigger["on"], "collection")?;
                ensure!(
                    matches!(
                        trigger["on"]["op"].as_str(),
                        Some("insert" | "update" | "delete")
                    ),
                    "invalid: change operation"
                );
                ensure!(
                    manifest["collections"].get(name).is_some()
                        || manifest["foreign_collections"][name]["ops"]
                            .as_array()
                            .is_some_and(|ops| ops.iter().any(|v| v == "watch")),
                    "forbidden: trigger requires watch"
                );
            }
        }
    }
    Ok(())
}
