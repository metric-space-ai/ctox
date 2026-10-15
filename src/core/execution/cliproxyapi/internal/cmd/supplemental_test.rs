// Origin: CTOX supplemental command lifecycle tests; no upstream test credit.
// License: AGPL-3.0-only

use super::auth_manager::*;
use super::login_prompt::{IoPrompt, Prompt, RejectingPrompt};
use super::openai_device_login::{codex_device_login_plan, CODEX_LOGIN_MODE_METADATA_KEY};
use super::openai_login::LoginOptions;
use super::run::{
    start_service, BackgroundService, ProxyService, ServiceCancellation, ServiceFactory,
    ServicePlan,
};
use super::vertex_import::{
    execute_vertex_import, ImportFilesystem, VertexCredentialRecord, VertexCredentialSink,
    VertexImportPlan,
};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct StaticLoginHandler;
impl LoginHandler for StaticLoginHandler {
    fn login(
        &self,
        plan: &LoginPlan,
        _prompt: &dyn Prompt,
        cancellation: &CommandCancellation,
    ) -> Result<LoginRecord, LoginFailure> {
        if cancellation.is_cancelled() {
            return Err(LoginFailure {
                kind: LoginFailureKind::Cancelled,
                message: "cancelled".into(),
            });
        }
        Ok(LoginRecord {
            id: plan.provider.clone(),
            label: "account".into(),
            saved_path: Some(PathBuf::from("auth/account.json")),
        })
    }
}
#[derive(Default)]
struct CapturingOutput {
    info: Mutex<Vec<String>>,
    error: Mutex<Vec<String>>,
}
impl CommandOutput for CapturingOutput {
    fn info(&self, message: &str) {
        self.info.lock().unwrap().push(message.into());
    }
    fn error(&self, message: &str) {
        self.error.lock().unwrap().push(message.into());
    }
}

#[test]
fn login_plans_are_typed_cancellable_and_device_metadata_is_explicit() {
    let manager = AuthManager::new([(
        "codex".into(),
        Arc::new(StaticLoginHandler) as Arc<dyn LoginHandler>,
    )]);
    let output = CapturingOutput::default();
    let cancellation = CommandCancellation::default();
    let command = LoginCommand {
        manager: &manager,
        prompt: &RejectingPrompt,
        output: &output,
        cancellation: &cancellation,
    };
    let plan = codex_device_login_plan(
        CommandConfig {
            config_path: "config.json".into(),
            auth_dir: "auth".into(),
        },
        &LoginOptions {
            no_browser: true,
            callback_port: Some(1455),
        },
    );
    assert_eq!(plan.metadata[CODEX_LOGIN_MODE_METADATA_KEY], "device");
    assert!(command.execute(&plan).is_ok());
    assert_eq!(output.info.lock().unwrap().len(), 3);
    cancellation.cancel();
    assert_eq!(
        command.execute(&plan).unwrap_err().kind,
        LoginFailureKind::Cancelled
    );
}

struct TestService {
    ran: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}
impl ProxyService for TestService {
    fn run(&mut self, cancellation: &ServiceCancellation) -> io::Result<()> {
        self.ran.store(true, Ordering::Release);
        cancellation.wait();
        Ok(())
    }
    fn shutdown(&mut self) -> io::Result<()> {
        self.shutdown.store(true, Ordering::Release);
        Ok(())
    }
}
struct TestFactory {
    ran: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}
impl ServiceFactory for TestFactory {
    fn build(&self, _plan: &ServicePlan) -> io::Result<Box<dyn ProxyService>> {
        Ok(Box::new(TestService {
            ran: self.ran.clone(),
            shutdown: self.shutdown.clone(),
        }))
    }
}

#[test]
fn foreground_and_background_service_paths_own_shutdown() {
    let ran = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let factory = Arc::new(TestFactory {
        ran: ran.clone(),
        shutdown: shutdown.clone(),
    });
    let background = BackgroundService::start(factory, ServicePlan::default()).unwrap();
    for _ in 0..100 {
        if ran.load(Ordering::Acquire) {
            break;
        }
        std::thread::yield_now();
    }
    background.join().unwrap();
    assert!(shutdown.load(Ordering::Acquire));
    struct ImmediateService;
    impl ProxyService for ImmediateService {
        fn run(&mut self, _: &ServiceCancellation) -> io::Result<()> {
            Ok(())
        }
    }
    struct ImmediateFactory;
    impl ServiceFactory for ImmediateFactory {
        fn build(&self, _: &ServicePlan) -> io::Result<Box<dyn ProxyService>> {
            Ok(Box::new(ImmediateService))
        }
    }
    assert!(start_service(
        &ImmediateFactory,
        &ServicePlan::default(),
        &ServiceCancellation::default()
    )
    .is_ok());
}

struct MemoryFs(Vec<u8>);
impl ImportFilesystem for MemoryFs {
    fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}
#[derive(Default)]
struct MemoryVertexSink(Mutex<Option<VertexCredentialRecord>>);
impl VertexCredentialSink for MemoryVertexSink {
    fn save(&self, auth_dir: &Path, record: &VertexCredentialRecord) -> io::Result<PathBuf> {
        *self.0.lock().unwrap() = Some(record.clone());
        Ok(auth_dir.join(&record.id))
    }
}
