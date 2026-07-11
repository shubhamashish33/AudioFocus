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
    pub auto_resume_timeout: Duration,
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
            auto_resume_timeout: Duration::from_secs(300), // 5 minutes
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

        match source.source_type {
            crate::media_source::SourceType::Smtc | crate::media_source::SourceType::Hybrid => {
                Some(PauseRoute::Smtc)
            }
            crate::media_source::SourceType::NonSmtc => Some(PauseRoute::NonSmtc),
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
    ResumeCompleted(ResumeExecutionResult),
}

impl ArbitrationEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::Media(event) => event.name(),
            Self::SessionInactive { .. } => "SessionInactive",
            Self::SessionDisconnected { .. } => "SessionDisconnected",
            Self::SourceRemoved { .. } => "SourceRemoved",
            Self::PauseCompleted(_) => "PauseCompleted",
            Self::ResumeCompleted(_) => "ResumeCompleted",
        }
    }

    fn source(&self) -> Option<&MediaSource> {
        match self {
            Self::Media(event) => event.source(),
            Self::SessionInactive { source }
            | Self::SessionDisconnected { source }
            | Self::SourceRemoved { source } => Some(source),
            Self::PauseCompleted(_) | Self::ResumeCompleted(_) => None,
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
pub struct ResumeExecutionResult {
    pub source: MediaSource,
    pub requested_by: MediaSourceId,
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
    auto_resume: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ArbitrationEngine {
    pub fn start(config: ArbitrationConfig, controllers: ControllerRegistry) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(ArbitrationState::new().snapshot()));
        let enabled = Arc::new(AtomicBool::new(true));
        let auto_resume = Arc::new(AtomicBool::new(true));
        let handle = ArbitrationHandle {
            sender: sender.clone(),
            snapshot: Arc::clone(&snapshot),
        };

        let worker_enabled = Arc::clone(&enabled);
        let worker_auto_resume = Arc::clone(&auto_resume);
        let worker = thread::Builder::new()
            .name("arbitration-engine".to_string())
            .spawn(move || {
                let mut worker = ArbitrationWorker::new(
                    config,
                    controllers,
                    sender,
                    snapshot,
                    worker_enabled,
                    worker_auto_resume,
                );
                worker.run(receiver);
            })
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?;

        Ok(Self {
            handle,
            enabled,
            auto_resume,
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

    pub fn set_auto_resume(&self, enabled: bool) {
        self.auto_resume.store(enabled, Ordering::SeqCst);
    }

    pub fn is_auto_resume_enabled(&self) -> bool {
        self.auto_resume.load(Ordering::SeqCst)
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
    auto_resume: Arc<AtomicBool>,
    storm_protector: EventStormProtector,
    state: ArbitrationState,
    debounce: DebounceCoordinator,
    suppression: SuppressionWindows,
    loop_guard: PauseLoopGuard,
    recently_promoted: HashMap<MediaSourceId, std::time::Instant>,
    pending_resumes: HashSet<MediaSourceId>,
}

impl ArbitrationWorker {
    fn new(
        config: ArbitrationConfig,
        controllers: ControllerRegistry,
        sender: Sender<ArbitrationMessage>,
        snapshot: Arc<Mutex<ArbitrationSnapshot>>,
        enabled: Arc<AtomicBool>,
        auto_resume: Arc<AtomicBool>,
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
            auto_resume,
            storm_protector: EventStormProtector::new(Duration::from_secs(1), 50),
            state: ArbitrationState::new(),
            recently_promoted: HashMap::new(),
            pending_resumes: HashSet::new(),
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
            ArbitrationEvent::ResumeCompleted(result) => {
                self.handle_resume_completed(result);
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
        let released_active =
            mark_pause_observed(&mut self.state, &source.id, PauseOrigin::External);
        if released_active {
            self.maybe_auto_resume(&source.id);
        }
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
            self.maybe_auto_resume(&source.id);
        }
        self.state.push_history(source.id, event_name);
    }

    fn handle_removed(&mut self, source: MediaSource, event_name: &'static str) {
        tracing::info!(
            source_id = %source.id,
            event = event_name,
            "arbitration removed source from ownership state"
        );
        let was_active = self.state.currently_active_source.as_ref() == Some(&source.id);
        remove_source(&mut self.state, &source.id, event_name);
        if was_active {
            self.maybe_auto_resume(&source.id);
        }
    }

    fn maybe_auto_resume(&mut self, stopped_source_id: &MediaSourceId) {
        if !self.auto_resume.load(Ordering::SeqCst) {
            return;
        }

        let now = std::time::Instant::now();
        let candidate = auto_resume_candidate(
            &self.state,
            stopped_source_id,
            now,
            self.config.auto_resume_timeout,
        );

        if let Some(id) = candidate {
            if self.pending_resumes.contains(&id) {
                return;
            }
            if let Some(record) = self.state.previously_paused_sources.get(&id).cloned() {
                tracing::info!(
                    source_id = %id,
                    stopped_source = %stopped_source_id,
                    "arbitration triggering auto-resume for previously paused source"
                );
                self.request_play(record.paused_source, stopped_source_id.clone());
            }
        }
    }

    fn request_play(&mut self, source: MediaSource, requested_by: MediaSourceId) {
        let route = self.controllers.route_for(&source);
        self.pending_resumes.insert(source.id.clone());

        let sender = self.sender.clone();
        let controllers = self.controllers.clone();
        let config = self.config.clone();
        let dispatch_source = source.clone();
        let dispatch_requested_by = requested_by.clone();
        let spawn_result = thread::Builder::new()
            .name("arbitration-play-dispatch".to_string())
            .spawn(move || {
                let result = execute_play(
                    dispatch_source,
                    dispatch_requested_by,
                    route,
                    controllers,
                    config,
                );
                let _ = sender.send(ArbitrationMessage::Event(Box::new(
                    ArbitrationEvent::ResumeCompleted(result),
                )));
            });

        if let Err(error) = spawn_result {
            self.handle_resume_completed(ResumeExecutionResult {
                source,
                requested_by,
                success: false,
                route,
                error: Some(error.to_string()),
            });
        }
    }

    fn handle_resume_completed(&mut self, result: ResumeExecutionResult) {
        self.pending_resumes.remove(&result.source.id);

        if result.success {
            consume_resumed_record(&mut self.state, &result.source.id, &result.requested_by);
            tracing::info!(
                source_id = %result.source.id,
                requested_by = %result.requested_by,
                route = ?result.route,
                "arbitration auto-resume command succeeded"
            );
        } else {
            tracing::error!(
                source_id = %result.source.id,
                requested_by = %result.requested_by,
                route = ?result.route,
                error = result.error.as_deref(),
                "arbitration auto-resume command failed; resume record retained"
            );
        }
    }

    fn handle_pause_completed(&mut self, result: PauseExecutionResult) {
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
                let _ = sender.send(ArbitrationMessage::Event(Box::new(
                    ArbitrationEvent::PauseCompleted(result),
                )));
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

    fn prune_recently_promoted(&mut self) {
        let now = std::time::Instant::now();
        let window = self.config.oscillation_window;
        self.recently_promoted
            .retain(|_, instant| now.duration_since(*instant) <= window);
    }

    fn is_oscillation(&mut self, source_id: &MediaSourceId) -> bool {
        self.prune_recently_promoted();
        self.recently_promoted.contains_key(source_id)
    }

    fn is_simultaneous_conflict(&mut self, source_id: &MediaSourceId) -> bool {
        self.prune_recently_promoted();
        self.state
            .currently_active_source
            .as_ref()
            .filter(|active_id| *active_id != source_id)
            .is_some_and(|active_id| self.recently_promoted.contains_key(active_id))
    }

    fn publish_snapshot(&self) {
        match self.snapshot.lock() {
            Ok(mut snapshot) => *snapshot = self.state.snapshot(),
            Err(error) => tracing::error!(%error, "failed to publish arbitration snapshot"),
        }
    }
}

fn auto_resume_candidate(
    state: &ArbitrationState,
    stopped_source_id: &MediaSourceId,
    now: std::time::Instant,
    timeout: Duration,
) -> Option<MediaSourceId> {
    state
        .previously_paused_sources
        .iter()
        .filter(|(_, record)| {
            record.completed
                && &record.requested_by.id == stopped_source_id
                && now.duration_since(record.requested_at) <= timeout
        })
        .max_by_key(|(_, record)| record.requested_at)
        .map(|(id, _)| id.clone())
}

fn consume_resumed_record(
    state: &mut ArbitrationState,
    resumed_source_id: &MediaSourceId,
    requested_by: &MediaSourceId,
) -> bool {
    let matching_record = state
        .previously_paused_sources
        .get(resumed_source_id)
        .is_some_and(|record| &record.requested_by.id == requested_by);
    if matching_record {
        state.previously_paused_sources.remove(resumed_source_id);
    }
    matching_record
}

fn execute_play(
    source: MediaSource,
    requested_by: MediaSourceId,
    route: Option<PauseRoute>,
    controllers: ControllerRegistry,
    config: ArbitrationConfig,
) -> ResumeExecutionResult {
    let outcome = match route {
        Some(PauseRoute::Smtc) => {
            let Some(controller) = controllers.smtc else {
                return ResumeExecutionResult {
                    source,
                    requested_by,
                    success: false,
                    route,
                    error: Some("SMTC controller unavailable".to_string()),
                };
            };
            match controller.play(source.id.clone()) {
                Ok(result) if result.accepted_by_session => (true, None),
                Ok(_) => (
                    false,
                    Some("SMTC session rejected play command".to_string()),
                ),
                Err(error) => (false, Some(error.to_string())),
            }
        }
        Some(PauseRoute::NonSmtc) => {
            let Some(controller) = controllers.non_smtc else {
                return ResumeExecutionResult {
                    source,
                    requested_by,
                    success: false,
                    route,
                    error: Some("non-SMTC controller unavailable".to_string()),
                };
            };
            match controller.play(source.clone(), config.non_smtc_retry) {
                Ok(receiver) => match receiver.recv() {
                    Ok(result) => (result.success, result.last_error),
                    Err(error) => (false, Some(error.to_string())),
                },
                Err(error) => (false, Some(error.to_string())),
            }
        }
        None => (
            false,
            Some("no play controller route available".to_string()),
        ),
    };

    ResumeExecutionResult {
        source,
        requested_by,
        success: outcome.0,
        route,
        error: outcome.1,
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
        Some(PauseRoute::Smtc) => {
            execute_smtc_pause(generation_id, source, requested_by, controllers)
        }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::media_source::{MediaCapability, MediaSourceKind, ProcessIdentity, SourceType};

    use super::*;

    fn source(id: &str) -> MediaSource {
        MediaSource {
            id: MediaSourceId::new(id),
            kind: MediaSourceKind::DesktopApp,
            source_type: SourceType::NonSmtc,
            capability: MediaCapability::Unknown,
            source_app_user_model_id: id.to_string(),
            process: Some(ProcessIdentity {
                process_id: 42,
                creation_time: 1,
                executable_path: Some(PathBuf::from(format!("C:/Apps/{id}.exe"))),
                executable_name: format!("{id}.exe"),
                package_full_name: None,
            }),
        }
    }

    fn completed_pause(
        state: &mut ArbitrationState,
        paused: MediaSource,
        requested_by: MediaSource,
        generation_id: u64,
    ) {
        mark_paused_by_audiofocus(state, paused.clone(), requested_by, generation_id, true);
        state
            .previously_paused_sources
            .get_mut(&paused.id)
            .expect("pause record")
            .completed = true;
    }

    #[test]
    fn auto_resume_selects_source_paused_by_stopped_owner() {
        let mut state = ArbitrationState::new();
        let music = source("music");
        let browser = source("browser");
        completed_pause(&mut state, music.clone(), browser.clone(), 1);

        let candidate = auto_resume_candidate(
            &state,
            &browser.id,
            std::time::Instant::now(),
            Duration::from_secs(300),
        );

        assert_eq!(candidate, Some(music.id));
    }

    #[test]
    fn auto_resume_rejects_pause_requested_by_another_source() {
        let mut state = ArbitrationState::new();
        let music = source("music");
        let browser = source("browser");
        completed_pause(&mut state, music, source("other"), 1);

        let candidate = auto_resume_candidate(
            &state,
            &browser.id,
            std::time::Instant::now(),
            Duration::from_secs(300),
        );

        assert_eq!(candidate, None);
    }

    #[test]
    fn auto_resume_requires_completed_pause_command() {
        let mut state = ArbitrationState::new();
        let music = source("music");
        let browser = source("browser");
        mark_paused_by_audiofocus(&mut state, music, browser.clone(), 1, true);

        let candidate = auto_resume_candidate(
            &state,
            &browser.id,
            std::time::Instant::now(),
            Duration::from_secs(300),
        );

        assert_eq!(candidate, None);
    }

    #[test]
    fn auto_resume_rejects_expired_pause_record() {
        let mut state = ArbitrationState::new();
        let music = source("music");
        let browser = source("browser");
        completed_pause(&mut state, music, browser.clone(), 1);

        let candidate = auto_resume_candidate(
            &state,
            &browser.id,
            std::time::Instant::now() + Duration::from_secs(301),
            Duration::from_secs(300),
        );

        assert_eq!(candidate, None);
    }

    #[test]
    fn successful_resume_consumes_only_matching_pause_record() {
        let mut state = ArbitrationState::new();
        let music = source("music");
        let browser = source("browser");
        completed_pause(&mut state, music.clone(), browser.clone(), 1);

        assert!(!consume_resumed_record(
            &mut state,
            &music.id,
            &MediaSourceId::new("other")
        ));
        assert!(state.previously_paused_sources.contains_key(&music.id));

        assert!(consume_resumed_record(&mut state, &music.id, &browser.id));
        assert!(!state.previously_paused_sources.contains_key(&music.id));
    }

    #[test]
    fn external_pause_reports_active_ownership_release() {
        let mut state = ArbitrationState::new();
        let browser = source("browser");
        promote_active(&mut state, browser.clone(), 1);

        assert!(mark_pause_observed(
            &mut state,
            &browser.id,
            PauseOrigin::External
        ));
        assert_eq!(state.currently_active_source, None);
    }
}
