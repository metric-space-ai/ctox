//! Process-wide ownership reservations, including startup and shutdown.
use crate::{
    types::{DbId, StoreError},
    writer::{self, Inner, WriterHandle, WriterOptions},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

static OWNERS: OnceLock<Mutex<HashMap<DbId, Weak<Inner>>>> = OnceLock::new();

/// All Registry values share reservations. SQLite is opened asynchronously on
/// its dedicated OS thread; use `handle.ready().await` to observe startup.
#[derive(Clone, Copy, Default)]
pub struct Registry;
impl Registry {
    pub fn open(&self, db_id: DbId, options: WriterOptions) -> Result<WriterHandle, StoreError> {
        let mut owners = OWNERS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        owners.retain(|_, owner| owner.upgrade().is_some_and(|owner| !owner.stopped()));
        if owners.contains_key(&db_id) {
            return Err(StoreError::AlreadyOwned);
        }
        let handle = writer::start(db_id.clone(), options)?;
        owners.insert(db_id, Arc::downgrade(&handle.inner));
        Ok(handle)
    }
}
