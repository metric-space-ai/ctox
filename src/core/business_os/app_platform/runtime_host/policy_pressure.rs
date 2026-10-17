//! Test-only scheduler pressure at real P6 policy snapshot decoding.
//! The actual Store keeps its 20ms budget, admission and cancellation guards.
use super::*;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock, Weak,
    },
};
struct State {
    snapshots: AtomicUsize,
}
static SOURCES: OnceLock<Mutex<BTreeMap<PathBuf, Weak<State>>>> = OnceLock::new();
thread_local! { static ACTIVE: RefCell<Option<Arc<State>>> = const {RefCell::new(None)}; }
pub(super) struct Pressure {
    path: PathBuf,
    state: Arc<State>,
}
pub(super) struct ReadScope(Option<Arc<State>>);
pub(super) fn install(path: &Path) -> Pressure {
    let state = Arc::new(State {
        snapshots: AtomicUsize::new(0),
    });
    let path = path.to_owned();
    assert!(SOURCES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(path.clone(), Arc::downgrade(&state))
        .is_none());
    Pressure { path, state }
}
impl Pressure {
    pub(super) fn attempts(&self) -> usize {
        self.state.snapshots.load(Ordering::Acquire)
    }
}
impl Drop for Pressure {
    fn drop(&mut self) {
        SOURCES.get().unwrap().lock().unwrap().remove(&self.path);
    }
}
pub(super) fn enter(db: &Connection, binding: &Value) -> ReadScope {
    let state = if binding["command"]["type"] == "matching.match" {
        SOURCES
            .get()
            .and_then(|sources| sources.lock().ok())
            .and_then(|sources| {
                db.path()
                    .and_then(|path| sources.get(Path::new(path)).and_then(Weak::upgrade))
            })
    } else {
        None
    };
    ReadScope(ACTIVE.with(|active| active.replace(state)))
}
impl Drop for ReadScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active.replace(self.0.take());
        });
    }
}
pub(super) fn before_snapshot() {
    let active = ACTIVE.with(|active| active.borrow().clone());
    if let Some(state) = active {
        let count = state.snapshots.fetch_add(1, Ordering::AcqRel) + 1;
        eprintln!("APP_PLATFORM_MATCHING_POLICY_PRESSURE snapshot={count} decode_delay_ms=7 store_budget_ms=20");
        std::thread::sleep(Duration::from_millis(7));
    }
}
