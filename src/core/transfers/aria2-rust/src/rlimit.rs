//! C++ `--rlimit-nofile` (setrlimit RLIMIT_NOFILE). Default 0 = leave kernel limit.
#![forbid(unsafe_code)]

#[cfg(unix)]
use crate::error::Error;
use crate::error::Result;
use crate::options::OptionSet;
#[cfg(unix)]
use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
use std::sync::atomic::{AtomicU64, Ordering};

static LAST_SOFT: AtomicU64 = AtomicU64::new(0);

pub fn last_soft() -> u64 {
    LAST_SOFT.load(Ordering::SeqCst)
}

#[cfg(unix)]
pub fn current_nofile() -> Rlimit {
    getrlimit(Resource::Nofile)
}

#[cfg(unix)]
pub fn apply_nofile(opts: &OptionSet) -> Result<u64> {
    let n = opts.u64("rlimit-nofile", 0);
    if n == 0 {
        return Ok(0);
    }
    let cur = getrlimit(Resource::Nofile);
    let hard = cur.maximum.unwrap_or(n);
    let want = n.min(hard);
    setrlimit(
        Resource::Nofile,
        Rlimit {
            current: Some(want),
            maximum: cur.maximum,
        },
    )
    .map_err(|e| Error::Other(format!("RLIMIT_NOFILE {want}: {e}")))?;
    let got = getrlimit(Resource::Nofile)
        .current
        .unwrap_or(want);
    LAST_SOFT.store(got, Ordering::SeqCst);
    Ok(got)
}

/// Windows has no RLIMIT_NOFILE (handle limits are not per-process soft/hard
/// pairs), so `--rlimit-nofile` leaves the system limit alone, like the default 0.
#[cfg(windows)]
pub fn apply_nofile(opts: &OptionSet) -> Result<u64> {
    let _ = opts;
    Ok(0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::options::OptionSet;

    #[test]
    fn apply_nofile_sets_soft_then_restores() {
        let before = getrlimit(Resource::Nofile);
        let hard = before.maximum.unwrap_or(u64::MAX);
        let target = 3072u64.min(hard);
        let mut opts = OptionSet::new();
        opts.set("rlimit-nofile", target.to_string());
        let got = apply_nofile(&opts).unwrap();
        assert_eq!(got, target, "soft nofile must become {target}");
        assert_eq!(last_soft(), target);
        setrlimit(Resource::Nofile, before).unwrap();
        let after = getrlimit(Resource::Nofile);
        assert_eq!(after.current, before.current, "must restore caller rlimit");
    }
}
