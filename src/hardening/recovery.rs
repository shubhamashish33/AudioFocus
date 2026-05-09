use std::sync::Arc;
use crate::hardening::watchdog::Watchdog;

pub struct RecoveryCoordinator {
    watchdog: Arc<Watchdog>,
}

impl RecoveryCoordinator {
    pub fn new(watchdog: Arc<Watchdog>) -> Self {
        Self { watchdog }
    }

    pub fn monitor_and_recover(&self, host: &crate::tray::RuntimeHost) {
        let failed_workers = self.watchdog.check_health();
        if !failed_workers.is_empty() {
            tracing::warn!(failed_workers = ?failed_workers, "Attempting automatic recovery of stalled subsystems");
            // For now, we perform a full restart to ensure consistent state.
            // Requirement C/D: safely recreate subsystem automatically.
            if let Err(error) = host.restart() {
                tracing::error!(%error, "Automatic recovery failed; runtime may be unstable");
            } else {
                tracing::info!("Automatic recovery successful");
            }
        }
    }
}
