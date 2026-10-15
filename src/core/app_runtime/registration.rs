//! Core registration on the existing executor. No DB, runtime or process owner
//! is created here. The lifecycle owner supplies the authenticated host and its
//! committed activation snapshot, and feeds changes only after their commit.
use crate::{core_host::CoreRuntimeHost, daemon::AppRuntimeDaemon, service::ChangeEvent};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

pub struct AppRuntimeInputs {
    pub host: Arc<CoreRuntimeHost>,
    pub runtime: Handle,
    /// Core must attach the post-commit feed before taking this snapshot, then
    /// drain from its cursor. A snapshot read followed by subscribing is unsafe.
    pub active: Vec<(String, String)>,
}
enum Event {
    Published(String, String),
    Removed(String),
    Command(Value, Value),
    Change(ChangeEvent),
    CommandCompleted(Value, String, Value),
    Skill(String, Value),
    Prompt(String, String, Value),
}
struct Request {
    event: Event,
    reply: oneshot::Sender<Result<Value>>,
}

/// Cloneable, bounded producer handle retained by the Core command/feed owner.
#[derive(Clone)]
pub struct AppRuntimeIngress(mpsc::Sender<Request>);
impl AppRuntimeIngress {
    async fn request(&self, event: Event) -> Result<Value> {
        let (reply, result) = oneshot::channel();
        self.0
            .try_send(Request { event, reply })
            .map_err(|_| anyhow::anyhow!("unavailable: app runtime feed busy or stopped"))?;
        result
            .await
            .context("unavailable: app runtime feed stopped; reconcile committed operation")?
    }
    pub async fn published(&self, app: String, version: String) -> Result<()> {
        self.request(Event::Published(app, version))
            .await
            .map(|_| ())
    }
    pub async fn removed(&self, app: String) -> Result<()> {
        self.request(Event::Removed(app)).await.map(|_| ())
    }
    pub async fn command(&self, command: Value, fence: Value) -> Result<Value> {
        self.request(Event::Command(command, fence)).await
    }
    pub async fn change(&self, event: ChangeEvent) -> Result<Value> {
        self.request(Event::Change(event)).await
    }
    pub async fn command_completed(
        &self,
        binding: Value,
        id: String,
        outcome: Value,
    ) -> Result<Value> {
        self.request(Event::CommandCompleted(binding, id, outcome))
            .await
    }
    pub async fn skill(&self, name: String, actor: Value) -> Result<Value> {
        self.request(Event::Skill(name, actor)).await
    }
    pub async fn prompt(&self, app: String, path: String, actor: Value) -> Result<Value> {
        self.request(Event::Prompt(app, path, actor)).await
    }
}
pub struct AppRuntimeRegistration {
    pub ingress: AppRuntimeIngress,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}
impl AppRuntimeRegistration {
    /// Called exactly once at run_foreground after CUTOVER owns the sync writer.
    pub fn start(root: PathBuf, inputs: AppRuntimeInputs) -> Result<Self> {
        let daemon = AppRuntimeDaemon::production(root, inputs.host.clone())?;
        // Startup is fallible and synchronous: never advertise a runtime that
        // silently failed to load one of its active CAS releases.
        for (app, version) in &inputs.active {
            daemon.bootstrap_published(app, version)?;
        }
        let (send, mut receive) = mpsc::channel::<Request>(16);
        let (stop, mut stopping) = watch::channel(false);
        let authority = inputs.host.authority.clone();
        let task=inputs.runtime.spawn(async move {
            let mut timer=tokio::time::interval(std::time::Duration::from_secs(60));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            timer.tick().await; // first real tick is >= PT1M after registration
            loop {
                tokio::select! {
                    biased;
                    _=stopping.changed()=>break,
                    request=receive.recv()=>{
                        let Some(Request{event,reply})=request else {break};
                        let result=match event {
                            Event::Published(app,ver)=>daemon.published(app,ver).await.map(|_|Value::Null),
                            Event::Removed(app)=>daemon.removed(app).await.map(|_|Value::Null),
                            Event::Command(command,fence)=>daemon.command(command,fence).await,
                            Event::Change(event)=>daemon.change(event).await.map(Value::Array),
                            Event::CommandCompleted(binding,id,outcome)=>daemon.command_completed(binding,id,outcome).await,
                            Event::Skill(name,actor)=>daemon.skill(name,actor).await.and_then(|v|serde_json::to_value(v).map_err(Into::into)),
                            Event::Prompt(app,path,actor)=>daemon.prompt(app,path,actor).await,
                        };
                        // A dropped receiver never cancels an admitted effect.
                        // Core reconciles by operation ID; no automatic replay.
                        let _=reply.send(result);
                    },
                    _=timer.tick()=>{
                        if let Err(error)=daemon.tick().await {
                            let _=authority.log(&Value::Null,&serde_json::json!(["error","App runtime timer failed",{"code":crate::error_code(&error.to_string())}]));
                        }
                    },
                }
            }
            receive.close();
        });
        Ok(Self {
            ingress: AppRuntimeIngress(send),
            stop,
            task: Some(task),
        })
    }
    /// Drain the current bounded call before the Core releases its store owner.
    pub async fn shutdown(mut self) -> Result<()> {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            task.await
                .context("unavailable: app runtime supervisor stopped")?;
        }
        Ok(())
    }
}
impl Drop for AppRuntimeRegistration {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}
