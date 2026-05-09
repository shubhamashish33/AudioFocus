use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use crate::{
    error::{AudioFocusError, Result},
    hardening::EventStormProtector,
    media_events::MediaEvent,
    media_source::{MediaSource, MediaSourceId},
    non_smtc::{NonSmtcPauseController, RetryConfig},
    smtc::SmtcTransportController,
};

use super::{
    debounce::DebounceCoordinator,
    decision::{decide_started, ArbitrationDecision},
    loop_guard::PauseLoopGuard,
    ownership::{mark_pause_observed, mark_paused_by_audiofocus, promote_active, remove_source},
    state::{ArbitrationSnapshot, ArbitrationState, PauseOrigin},
    suppression::SuppressionWindows,
};

#[derive(Clone, Debug)]
pub struct ArbitrationConfig {
    pub duplicate_debounce: Duration,
    pub oscillation_window: Duration,
    pub self_pause_suppression_window: Duration,
    pub pause_loop_window: Duration,
    pub max_pauses_per_source_per_window: usize,
    pub non_smtc_retry: RetryConfig,
}

impl Default for ArbitrationConfig {
    fn default() -> Self {
        Self {
            duplicate_debounce: Duration::from_millis(150),
            oscillation_window: Duration::from_millis(300),
            self_pause_suppression_window: Duration::from_secs(3),
            pause_loop_window: Duration::from_secs(5),
            max_pauses_per_source_per_window: 4,
            non_smtc_retry: RetryConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseRoute {
    Smtc,
    NonSmtc,
}

#[derive(Clone, Debug, Default)]
pub struct ControllerRegistry {
    smtc: Option<SmtcTransportController>,
    non_smtc: Option<NonSmtcPauseController>,
    explicit_routes: Arc<Mutex<HashMap<MediaSourceId, PauseRoute>>>,
}

impl ControllerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_smtc(mut self, controller: SmtcTransportController) -> Self {
        self.smtc = Some(controller);
        self
    }

    pub fn with_non_smtc(mut self, controller: NonSmtcPauseController) -> Self {
        self.non_smtc = Some(controller);
        self
    }

    pub fn set_route(&self, source_id: MediaSourceId, route: PauseRoute) -> Result<()> {
        self.explicit_routes
            .lock()
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?
            .insert(source_id, route);
        Ok(())
    }

    fn route_for(&self, source: &MediaSource) -> Option<PauseRoute> {
        if let Ok(routes) = self.explicit_routes.lock() {
            if let Some(route) = routes.get(&source.id) {
                return Some(*route);
            }
        }

        if self.smtc.is_some() && !source.source_app_user_model_id.is_empty() {
            Some(PauseRoute::Smtc)
        } else if self.non_smtc.is_some() && source.process.is_some() {
            Some(PauseRoute::NonSmtc)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArbitrationEvent {
    Media(MediaEvent),
    SessionInactive { source: MediaSource },
    SessionDisconnected { source: MediaSource },
    SourceRemoved { source: MediaSource },
    PauseCompleted(PauseExecutionResult),
}

impl ArbitrationEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::Media(event) => event.name(),
            Self::SessionInactive { .. } => "SessionInactive",
            Self::SessionDisconnected { .. } => "SessionDisconnected",
            Self::SourceRemoved { .. } => "SourceRemoved",
            Self::PauseCompleted(_) => "PauseCompleted",
        }
    }

    fn source(&self) -> Option<&MediaSource> {
        match self {
            Self::Media(event) => event.source(),
            Self::SessionInactive { source }
            | Self::SessionDisconnected { source }
            | Self::SourceRemoved { source } => Some(source),
            Self::PauseCompleted(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PauseExecutionResult {
    pub generation_id: u64,
    pub source: MediaSource,
    pub requested_by: MediaSource,
    pub success: bool,
    pub route: Option<PauseRoute>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArbitrationHandle {
    sender: Sender<ArbitrationMessage>,
    snapshot: Arc<Mutex<ArbitrationSnapshot>>,
}

impl ArbitrationHandle {
    pub fn submit(&self, event: ArbitrationEvent) -> Result<()> {
        self.sender
            .send(ArbitrationMessage::Event(Box::new(event)))
            .map_err(|error| AudioFocusError::Thread(error.to_string()))
    }

    pub fn snapshot(&self) -> Result<ArbitrationSnapshot> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|error| AudioFocusError::Thread(error.to_string()))
    }
}

#[derive(Debug)]
pub struct ArbitrationEngine {
    handle: ArbitrationHandle,
    enabled: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ArbitrationEngine {
    pub fn start(config: ArbitrationConfig, controllers: ControllerRegistry) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(ArbitrationState::new().snapshot()));
        let enabled = Arc::new(AtomicBool::new(true));
        let handle = ArbitrationHandle {
            sender: sender.clone(),
            snapshot: Arc::clone(&snapshot),
        };

        let worker_enabled = Arc::clone(&enabled);
        let worker = thread::Builder::new()
            .name("arbitration-engine".to_string())
            .spawn(move || {
                let mut worker = ArbitrationWorker::new(config, controllers, sender, snapshot, worker_enabled);
                worker.run(receiver);
            })
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?;

        Ok(Self {
            handle,
            enabled,
            worker: Some(worker),
        })
    }

    pub fn handle(&self) -> ArbitrationHandle {
        self.handle.clone()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn shutdown(mut self) -> Result<()> {
        self.handle
            .sender
            .send(ArbitrationMessage::Shutdown)
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?;
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| AudioFocusError::Thread("arbitration worker panicked".to_string()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum ArbitrationMessage {
    Event(Box<ArbitrationEvent>),
    Shutdown,
}

struct ArbitrationWorker {
    config: ArbitrationConfig,
    controllers: ControllerRegistry,
    sender: Sender<ArbitrationMessage>,
    snapshot: Arc<Mutex<ArbitrationSnapshot>>,
    enabled: Arc<AtomicBool>,
    storm_protector: EventStormProtector,
    state: ArbitrationState,
    debounce: DebounceCoordinator,
    suppression: SuppressionWindows,
    loop_guard: PauseLoopGuard,
    recently_promoted: HashMap<MediaSourceId, std::time::Instant>,
    pending_pause_generations: HashSet<u64>,
}

impl ArbitrationWorker {
    fn new(
        config: ArbitrationConfig,
        controllers: ControllerRegistry,
        sender: Sender<ArbitrationMessage>,
        snapshot: Arc<Mutex<ArbitrationSnapshot>>,
        enabled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            debounce: DebounceCoordinator::new(config.duplicate_debounce),
            suppression: SuppressionWindows::new(config.self_pause_suppression_window),
            loop_guard: PauseLoopGuard::new(
                config.pause_loop_window,
                config.max_pauses_per_source_per_window,
            ),
            config,
            controllers,
            sender,
            snapshot,
            enabled,
            storm_protector: EventStormProtector::new(Duration::from_secs(1), 50),
            state: ArbitrationState::new(),
            recently_promoted: HashMap::new(),
            pending_pause_generations: HashSet::new(),
        }
    }

    fn run(&mut self, receiver: Receiver<ArbitrationMessage>) {
        while let Ok(message) = receiver.recv() {
            match message {
                ArbitrationMessage::Event(event) => self.handle_event(*event),
                ArbitrationMessage::Shutdown => break,
            }
            self.publish_snapshot();
        }
    }

    fn handle_event(&mut self, event: ArbitrationEvent) {
        if !self.storm_protector.check_and_record() {
            return;
        }

        let generation_id = self.state.next_generation();
        let event_name = event.name();

        if let Some(source) = event.source() {
            if self.debounce.should_drop(&source.id, event_name) {
                return;
            }
        }

        match event {
            ArbitrationEvent::Media(MediaEvent::MediaStarted { source, .. }) => {
                if self.enabled.load(Ordering::SeqCst) {
                    self.handle_media_started(source, generation_id);
                } else {
                    self.state.upsert_source(source);
                }
            }
            ArbitrationEvent::Media(MediaEvent::MediaPaused { source, .. }) => {
                self.handle_media_paused(source, generation_id);
            }
            ArbitrationEvent::Media(MediaEvent::MediaStopped { source, .. })
            | ArbitrationEvent::SessionInactive { source } => {
                self.handle_inactive(source, event_name);
            }
            ArbitrationEvent::SessionDisconnected { source }
            | ArbitrationEvent::SourceRemoved { source } => {
                self.handle_removed(source, event_name);
            }
            ArbitrationEvent::Media(MediaEvent::MediaMetadataChanged { source, .. })
            | ArbitrationEvent::Media(MediaEvent::ActiveSessionChanged {
                source: Some(source),
            }) => {
                self.state.upsert_source(source);
            }
            ArbitrationEvent::Media(MediaEvent::ActiveSessionChanged { source: None }) => {}
            ArbitrationEvent::PauseCompleted(result) => {
                self.handle_pause_completed(result);
            }
        }
    }

    fn handle_media_started(&mut self, source: MediaSource, generation_id: u64) {
        if self.is_oscillation(&source.id) {
            tracing::warn!(
                source_id = %source.id,
                generation_id,
                oscillation_ms = self.config.oscillation_window.as_millis(),
                "arbitration ignored rapid oscillation MediaStarted event"
            );
            return;
        }

        self.state.upsert_source(source.clone());
        let simultaneous_conflict = self.is_simultaneous_conflict(&source.id);
        match decide_started(&self.state, &source, simultaneous_conflict) {
            ArbitrationDecision::Noop { reason } => {
                tracing::info!(
                    source_id = %source.id,
                    generation_id,
                    reason,
                    "arbitration made no state change"
                );
            }
            ArbitrationDecision::Promote { source } => {
                tracing::info!(
                    source_id = %source.id,
                    generation_id,
                    "arbitration promoted active source"
                );
                promote_active(&mut self.state, source.clone(), generation_id);
                self.recently_promoted
                    .insert(source.id.clone(), std::time::Instant::now());
            }
            ArbitrationDecision::Switch { from, to } => {
                tracing::info!(
                    from_source_id = %from.id,
                    to_source_id = %to.id,
                    generation_id,
                    "arbitration switching active playback owner"
                );
                promote_active(&mut self.state, to.clone(), generation_id);
                self.recently_promoted
                    .insert(to.id.clone(), std::time::Instant::now());
                self.request_pause(from, to, generation_id, true);
            }
            ArbitrationDecision::RejectChallenger { challenger, active } => {
                tracing::info!(
                    active_source_id = %active.id,
                    rejected_source_id = %challenger.id,
                    generation_id,
                    "arbitration rejected simultaneous challenger"
                );
                self.request_pause(challenger, active, generation_id, false);
            }
        }
    }

    fn handle_media_paused(&mut self, source: MediaSource, generation_id: u64) {
        self.state.upsert_source(source.clone());
        if let Some(pause_generation) = self.suppression.consume_if_suppressed(&source.id) {
            tracing::info!(
                source_id = %source.id,
                generation_id,
                pause_generation,
                "arbitration suppressed self-generated pause event"
            );
            mark_pause_observed(
                &mut self.state,
                &source.id,
                PauseOrigin::AudioFocus {
                    generation_id: pause_generation,
                },
            );
            return;
        }

        tracing::info!(
            source_id = %source.id,
            generation_id,
            "arbitration observed external pause"
        );
        mark_pause_observed(&mut self.state, &source.id, PauseOrigin::External);
    }

    fn handle_inactive(&mut self, source: MediaSource, event_name: &'static str) {
        self.state.upsert_source(source.clone());
        if self.state.currently_active_source.as_ref() == Some(&source.id) {
            tracing::info!(
                source_id = %source.id,
                event = event_name,
                "arbitration released active ownership"
            );
            self.state.currently_active_source = None;
        }
        self.state.push_history(source.id, event_name);
    }

    fn handle_removed(&mut self, source: MediaSource, event_name: &'static str) {
        tracing::info!(
            source_id = %source.id,
            event = event_name,
            "arbitration removed source from ownership state"
        );
        remove_source(&mut self.state, &source.id, event_name);
    }

    fn handle_pause_completed(&mut self, result: PauseExecutionResult) {
        self.pending_pause_generations.remove(&result.generation_id);
        match self.state.pending_pauses.remove(&result.generation_id) {
            Some(mut record) if result.success => {
                record.completed = true;
                self.state
                    .previously_paused_sources
                    .insert(result.source.id.clone(), record);
                self.suppression
                    .suppress_pause_event(result.source.id.clone(), result.generation_id);
                tracing::info!(
                    source_id = %result.source.id,
                    requested_by = %result.requested_by.id,
                    generation_id = result.generation_id,
                    route = ?result.route,
                    "arbitration pause command succeeded"
                );
            }
            Some(record) => {
                if record.rollback_active_on_failure {
                    self.state.currently_active_source = Some(record.paused_source.id.clone());
                    tracing::warn!(
                        source_id = %record.paused_source.id,
                        requested_by = %record.requested_by.id,
                        generation_id = result.generation_id,
                        "arbitration rolled active owner back after pause failure"
                    );
                }
                self.state
                    .previously_paused_sources
                    .remove(&record.paused_source.id);
                tracing::error!(
                    source_id = %result.source.id,
                    requested_by = %result.requested_by.id,
                    generation_id = result.generation_id,
                    route = ?result.route,
                    error = result.error.as_deref(),
                    "arbitration pause command failed; state rolled back"
                );
            }
            None => {
                tracing::warn!(
                    source_id = %result.source.id,
                    generation_id = result.generation_id,
                    "arbitration received stale pause completion"
                );
            }
        }
    }

    fn request_pause(
        &mut self,
        source: MediaSource,
        requested_by: MediaSource,
        generation_id: u64,
        rollback_active_on_failure: bool,
    ) {
        if !self.loop_guard.allow_pause(&source.id) {
            return;
        }

        let route = self.controllers.route_for(&source);
        mark_paused_by_audiofocus(
            &mut self.state,
            source.clone(),
            requested_by.clone(),
            generation_id,
            rollback_active_on_failure,
        );
        self.pending_pause_generations.insert(generation_id);
        self.suppression
            .suppress_pause_event(source.id.clone(), generation_id);

        let sender = self.sender.clone();
        let controllers = self.controllers.clone();
        let config = self.config.clone();
        let dispatch_source = source.clone();
        let dispatch_requested_by = requested_by.clone();
        let spawn_result = thread::Builder::new()
            .name("arbitration-pause-dispatch".to_string())
            .spawn(move || {
                let result = execute_pause(
                    generation_id,
                    dispatch_source,
                    dispatch_requested_by,
                    route,
                    controllers,
                    config,
                );
                let _ = sender.send(ArbitrationMessage::Event(
                    Box::new(ArbitrationEvent::PauseCompleted(result)),
                ));
            });
        if let Err(error) = spawn_result {
            tracing::error!(%error, generation_id, "failed to spawn arbitration pause dispatch");
            self.handle_pause_completed(PauseExecutionResult {
                generation_id,
                source,
                requested_by,
                success: false,
                route,
                error: Some(error.to_string()),
            });
        }
    }

    fn is_oscillation(&mut self, source_id: &MediaSourceId) -> bool {
        let now = std::time::Instant::now();
        self.recently_promoted
            .retain(|_, instant| now.duration_since(*instant) <= self.config.oscillation_window);
        self.recently_promoted.get(source_id).is_some_and(|instant| {
            now.duration_since(*instant) <= self.config.oscillation_window
        })
    }

    fn is_simultaneous_conflict(&mut self, source_id: &MediaSourceId) -> bool {
        let now = std::time::Instant::now();
        self.recently_promoted
            .retain(|_, instant| now.duration_since(*instant) <= self.config.oscillation_window);
        self.state
            .currently_active_source
            .as_ref()
            .filter(|active_id| *active_id != source_id)
            .and_then(|active_id| self.recently_promoted.get(active_id))
            .is_some_and(|instant| now.duration_since(*instant) <= self.config.oscillation_window)
    }

    fn publish_snapshot(&self) {
        match self.snapshot.lock() {
            Ok(mut snapshot) => *snapshot = self.state.snapshot(),
            Err(error) => tracing::error!(%error, "failed to publish arbitration snapshot"),
        }
    }
}

fn execute_pause(
    generation_id: u64,
    source: MediaSource,
    requested_by: MediaSource,
    route: Option<PauseRoute>,
    controllers: ControllerRegistry,
    config: ArbitrationConfig,
) -> PauseExecutionResult {
    match route {
        Some(PauseRoute::Smtc) => execute_smtc_pause(generation_id, source, requested_by, controllers),
        Some(PauseRoute::NonSmtc) => {
            execute_non_smtc_pause(generation_id, source, requested_by, controllers, config)
        }
        None => PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: false,
            route: None,
            error: Some("no pause controller route available".to_string()),
        },
    }
}

fn execute_smtc_pause(
    generation_id: u64,
    source: MediaSource,
    requested_by: MediaSource,
    controllers: ControllerRegistry,
) -> PauseExecutionResult {
    let Some(controller) = controllers.smtc else {
        return PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: false,
            route: Some(PauseRoute::Smtc),
            error: Some("SMTC controller unavailable".to_string()),
        };
    };

    match controller.pause(source.id.clone()) {
        Ok(result) => PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: result.accepted_by_session,
            route: Some(PauseRoute::Smtc),
            error: (!result.accepted_by_session)
                .then(|| "SMTC session rejected pause command".to_string()),
        },
        Err(error) => PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: false,
            route: Some(PauseRoute::Smtc),
            error: Some(error.to_string()),
        },
    }
}

fn execute_non_smtc_pause(
    generation_id: u64,
    source: MediaSource,
    requested_by: MediaSource,
    controllers: ControllerRegistry,
    config: ArbitrationConfig,
) -> PauseExecutionResult {
    let Some(controller) = controllers.non_smtc else {
        return PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: false,
            route: Some(PauseRoute::NonSmtc),
            error: Some("non-SMTC controller unavailable".to_string()),
        };
    };

    match controller.pause(source.clone(), config.non_smtc_retry) {
        Ok(receiver) => match receiver.recv() {
            Ok(result) => PauseExecutionResult {
                generation_id,
                source,
                requested_by,
                success: result.success,
                route: Some(PauseRoute::NonSmtc),
                error: result.last_error,
            },
            Err(error) => PauseExecutionResult {
                generation_id,
                source,
                requested_by,
                success: false,
                route: Some(PauseRoute::NonSmtc),
                error: Some(error.to_string()),
            },
        },
        Err(error) => PauseExecutionResult {
            generation_id,
            source,
            requested_by,
            success: false,
            route: Some(PauseRoute::NonSmtc),
            error: Some(error.to_string()),
        },
    }
}
