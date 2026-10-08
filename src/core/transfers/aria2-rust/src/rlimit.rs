//! C++ `--rlimit-nofile` (setrlimit RLIMIT_NOFILE). Default 0 = leave kernel limit.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
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

pub fn apply_nofile(opts: &OptionSet) -> Result<u64> {
    let n = opts.u64("rlimit-nofile", 0);
    if n == 0 {
        return Ok(0);
    }
    #[cfg(unix)]
    {
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
        let got = getrlimit(Resource::Nofile).current.unwrap_or(want);
        LAST_SOFT.store(got, Ordering::SeqCst);
        Ok(got)
    }
    #[cfg(not(unix))]
    {
        Err(Error::Other(
            "rlimit-nofile is not supported on this platform".into(),
        ))
    }
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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn default_limit_is_unchanged_but_explicit_posix_limit_is_rejected() {
        let mut opts = OptionSet::new();
        assert_eq!(apply_nofile(&opts).unwrap(), 0);
        opts.set("rlimit-nofile", "3072");
        assert!(apply_nofile(&opts)
            .unwrap_err()
            .to_string()
            .contains("not supported"));
    }
}
