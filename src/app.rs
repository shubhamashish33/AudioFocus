use std::{
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use crate::{
    arbitration::{ArbitrationConfig, ArbitrationEngine, ControllerRegistry},
    com::MtaApartment,
    error::Result,
    events::AudioSessionEvent,
    identity::IdentitySystem,
    non_smtc::NonSmtcPauseController,
    registry::AudioSessionRegistry,
    shutdown::ShutdownSignal,
    smtc::{SmtcRuntime, SmtcWorkerMessage},
    wasapi::WasapiSessionMonitor,
};

#[derive(Debug)]
pub struct AudioFocusMonitor {
    polling_interval: Duration,
}

pub struct AudioFocusRuntime {
    arbitration: Option<ArbitrationEngine>,
    smtc_runtime: SmtcRuntime,
    wasapi_worker: thread::JoinHandle<Result<()>>,
}

impl AudioFocusMonitor {
    pub fn new(polling_interval: Duration) -> Self {
        Self { polling_interval }
    }

    pub fn start(&self, shutdown: ShutdownSignal) -> Result<AudioFocusRuntime> {
        let polling_interval = self.polling_interval;
        let identity_system = Arc::new(IdentitySystem::new());

        let (smtc_sender, smtc_receiver) = mpsc::channel::<SmtcWorkerMessage>();
        let smtc_controller = crate::smtc::SmtcTransportController::new(smtc_sender.clone());
        let non_smtc_controller = NonSmtcPauseController::new();

        let controllers = ControllerRegistry::new()
            .with_smtc(smtc_controller)
            .with_non_smtc(non_smtc_controller);

        let arbitration = ArbitrationEngine::start(
            ArbitrationConfig::default(),
            controllers,
        )?;

        let smtc_runtime = SmtcRuntime::start_with_channel(
            shutdown.clone(),
            Arc::clone(&identity_system),
            arbitration.handle(),
            smtc_sender,
            smtc_receiver,
        )?;

        let wasapi_identity = Arc::clone(&identity_system);
        let wasapi_arbitration = arbitration.handle();
        let wasapi_shutdown = shutdown.clone();

        let wasapi_worker = thread::Builder::new()
            .name("wasapi-session-monitor".to_string())
            .spawn(move || {
                run_wasapi_worker(
                    wasapi_shutdown,
                    polling_interval,
                    wasapi_identity,
                    wasapi_arbitration,
                )
            })
            .map_err(|error| crate::error::AudioFocusError::Thread(error.to_string()))?;

        Ok(AudioFocusRuntime {
            arbitration: Some(arbitration),
            smtc_runtime,
            wasapi_worker,
        })
    }
}

impl AudioFocusRuntime {
    pub fn arbitration(&self) -> &ArbitrationEngine {
        self.arbitration.as_ref().unwrap()
    }

    pub fn shutdown(mut self) -> Result<()> {
        if let Some(arbitration) = self.arbitration.take() {
            arbitration.shutdown()?;
        }
        self.smtc_runtime.join()?;
        self.wasapi_worker.join().map_err(|_| {
            crate::error::AudioFocusError::Thread("WASAPI worker panicked".to_string())
        })??;
        Ok(())
    }
}

fn run_wasapi_worker(
    shutdown: ShutdownSignal,
    polling_interval: Duration,
    identity_system: Arc<IdentitySystem>,
    arbitration: crate::arbitration::ArbitrationHandle,
) -> Result<()> {
    let _com = MtaApartment::initialize()?;
    let mut monitor = WasapiSessionMonitor::from_default_render_endpoint()?;
    let mut registry = AudioSessionRegistry::new(identity_system.clone());

    while !shutdown.is_requested() {
        match monitor.snapshot_sessions() {
            Ok(snapshots) => {
                let events = registry.reconcile(snapshots);
                for event in events {
                    log_event(&event);
                    if let Some(source) = identity_system.resolve_wasapi_session(event.snapshot()) {
                        let _ = arbitration.submit(crate::arbitration::ArbitrationEvent::Media(
                            media_event_from_session(&event, source),
                        ));
                    }
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

fn media_event_from_session(
    event: &AudioSessionEvent,
    source: crate::media_source::MediaSource,
) -> crate::media_events::MediaEvent {
    match event {
        AudioSessionEvent::SessionStarted(_) | AudioSessionEvent::SessionBecameActive(_) => {
            crate::media_events::MediaEvent::MediaStarted {
                source,
                metadata: Default::default(),
            }
        }
        AudioSessionEvent::SessionStopped(_) => crate::media_events::MediaEvent::MediaStopped {
            source,
            metadata: Default::default(),
        },
        AudioSessionEvent::SessionBecameInactive(_) => {
            crate::media_events::MediaEvent::MediaPaused {
                source,
                metadata: Default::default(),
            }
        }
    }
}
