use std::time::Duration;

/// Lifecycle budgets are a typed contract, shared by bring-up and upgrades.
/// Cold migrations/peer initialization can occupy several minutes. A cutover
/// must allow that bounded bring-up window plus ordinary shutdown cleanup.
#[derive(Clone, Copy, Debug)]
pub struct ServiceLifecycleTimeouts {
    pub startup: Duration,
    pub shutdown: Duration,
}

impl ServiceLifecycleTimeouts {
    pub const fn release_switch_shutdown(self) -> Duration {
        self.startup.saturating_add(self.shutdown)
    }
}

pub const SERVICE_LIFECYCLE_TIMEOUTS: ServiceLifecycleTimeouts = ServiceLifecycleTimeouts {
    startup: Duration::from_secs(300),
    shutdown: Duration::from_secs(15),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_switch_budget_covers_cold_bringup_and_cleanup() {
        let budgets = SERVICE_LIFECYCLE_TIMEOUTS;
        assert_eq!(budgets.shutdown, Duration::from_secs(15));
        assert_eq!(budgets.release_switch_shutdown(), Duration::from_secs(315));
        let custom = ServiceLifecycleTimeouts {
            startup: Duration::from_secs(420),
            shutdown: Duration::from_secs(30),
        };
        assert_eq!(custom.release_switch_shutdown(), Duration::from_secs(450));
    }
}
