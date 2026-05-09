use std::{sync::Arc, thread, time::Duration};

use crate::{
    com::MtaApartment, error::Result, events::AudioSessionEvent, identity::IdentitySystem,
    registry::AudioSessionRegistry, shutdown::ShutdownSignal, smtc::SmtcRuntime,
    wasapi::WasapiSessionMonitor,
};

#[derive(Debug)]
pub struct AudioFocusMonitor {
    polling_interval: Duration,
}

impl AudioFocusMonitor {
    pub fn new(polling_interval: Duration) -> Self {
        Self { polling_interval }
    }

    pub fn run(&self, shutdown: ShutdownSignal) -> Result<()> {
        let polling_interval = self.polling_interval;
        let worker_shutdown = shutdown.clone();
        let identity_system = Arc::new(IdentitySystem::new());

        let wasapi_identity = Arc::clone(&identity_system);
        let worker = thread::Builder::new()
            .name("wasapi-session-monitor".to_string())
            .spawn(move || run_wasapi_worker(worker_shutdown, polling_interval, wasapi_identity))
            .map_err(|error| crate::error::AudioFocusError::Thread(error.to_string()))?;
        let smtc_runtime = SmtcRuntime::start(shutdown.clone(), Arc::clone(&identity_system))?;
        let _smtc_controller = smtc_runtime.controller();

        while !shutdown.is_requested() {
            thread::sleep(Duration::from_millis(100));
        }

        smtc_runtime.join()?;
        worker.join().map_err(|_| {
            crate::error::AudioFocusError::Thread("WASAPI worker panicked".to_string())
        })?
    }
}

fn run_wasapi_worker(
    shutdown: ShutdownSignal,
    polling_interval: Duration,
    identity_system: Arc<IdentitySystem>,
) -> Result<()> {
    let _com = MtaApartment::initialize()?;
    let mut monitor = WasapiSessionMonitor::from_default_render_endpoint()?;
    let mut registry = AudioSessionRegistry::new(identity_system);

    while !shutdown.is_requested() {
        match monitor.snapshot_sessions() {
            Ok(snapshots) => {
                let events = registry.reconcile(snapshots);
                for event in events {
                    log_event(&event);
                }
                tracing::debug!(tracked_sessions = registry.len(), "registry reconciled");
            }
            Err(error) => {
                tracing::error!(%error, "failed to snapshot WASAPI sessions; rebuilding endpoint binding");
                thread::sleep(Duration::from_millis(500));
                match WasapiSessionMonitor::from_default_render_endpoint() {
                    Ok(rebuilt) => {
                        monitor = rebuilt;
                        tracing::info!("rebuilt WASAPI default render endpoint binding");
                    }
                    Err(rebuild_error) => {
                        tracing::error!(%rebuild_error, "failed to rebuild WASAPI endpoint binding");
                    }
                }
            }
        }

        thread::sleep(polling_interval);
    }

    Ok(())
}

fn log_event(event: &AudioSessionEvent) {
    let snapshot = event.snapshot();
    tracing::info!(
        event = event.name(),
        process_id = snapshot.process_id,
        display_name = %snapshot.display_name,
        session_state = %snapshot.state,
        peak = snapshot.peak,
        session_count = snapshot.session_count,
        "audio session event"
    );
}
