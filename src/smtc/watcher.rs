use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{mpsc, Arc, Weak},
    time::{Duration, Instant},
};

use windows::{
    core::Interface,
    Foundation::{EventRegistrationToken, TypedEventHandler},
    Media::Control::{
        CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager, MediaPropertiesChangedEventArgs,
        PlaybackInfoChangedEventArgs, SessionsChangedEventArgs,
    },
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

use crate::{
    arbitration::ArbitrationHandle,
    error::{AudioFocusError, Result},
    hardening::Watchdog,
    identity::IdentitySystem,
    media_events::{MediaEvent, MediaMetadata, PlaybackState},
    media_source::{MediaSource, MediaSourceId},
    process::ProcessResolver,
    shutdown::ShutdownSignal,
};

use super::{
    controller::{TransportAction, TransportResult},
    translate::{metadata_from_smtc, playback_state_from_smtc},
    SmtcSessionKey,
};

const DUPLICATE_EVENT_WINDOW: Duration = Duration::from_millis(150);

pub enum SmtcWorkerMessage {
    SessionsChanged,
    CurrentSessionChanged,
    PlaybackInfoChanged {
        key: SmtcSessionKey,
    },
    MediaPropertiesChanged {
        key: SmtcSessionKey,
    },
    TransportCommand {
        source_id: MediaSourceId,
        action: TransportAction,
        reply: mpsc::Sender<Result<TransportResult>>,
    },
}

pub fn run_smtc_worker(
    shutdown: ShutdownSignal,
    sender: mpsc::Sender<SmtcWorkerMessage>,
    receiver: mpsc::Receiver<SmtcWorkerMessage>,
    identity_system: Arc<IdentitySystem>,
    arbitration: ArbitrationHandle,
    watchdog: Arc<Watchdog>,
) -> Result<()> {
    let _apartment = WinRtMtaApartment::initialize()?;
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
    let sender = Arc::new(SmtcMessageSink::new(sender));
    let mut state = SmtcWatcherState::new(manager, identity_system, arbitration, Arc::downgrade(&sender))?;

    state.reconcile_sessions()?;
    state.emit_current_session_changed()?;

    while !shutdown.is_requested() {
        watchdog.heartbeat("smtc-session-monitor");
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => state.handle_message(message)?,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }

    Ok(())
}

struct SmtcMessageSink {
    sender: mpsc::Sender<SmtcWorkerMessage>,
}

impl SmtcMessageSink {
    fn new(sender: mpsc::Sender<SmtcWorkerMessage>) -> Self {
        Self { sender }
    }

    fn send(&self, message: SmtcWorkerMessage) {
        if let Err(error) = self.sender.send(message) {
            tracing::warn!(%error, "failed to enqueue SMTC worker message");
        }
    }
}

struct SmtcWatcherState {
    manager: GlobalSystemMediaTransportControlsSessionManager,
    identity_system: Arc<IdentitySystem>,
    arbitration: ArbitrationHandle,
    resolver: ProcessResolver,
    sink: Weak<SmtcMessageSink>,
    manager_tokens: ManagerEventTokens,
    sessions: HashMap<SmtcSessionKey, TrackedSmtcSession>,
    source_to_keys: HashMap<MediaSourceId, HashSet<SmtcSessionKey>>,
    last_events: HashMap<(MediaSourceId, &'static str), Instant>,
}

impl SmtcWatcherState {
    fn new(
        manager: GlobalSystemMediaTransportControlsSessionManager,
        identity_system: Arc<IdentitySystem>,
        arbitration: ArbitrationHandle,
        sink: Weak<SmtcMessageSink>,
    ) -> Result<Self> {
        let manager_tokens = ManagerEventTokens::register(&manager, sink.clone())?;
        Ok(Self {
            manager,
            identity_system,
            arbitration,
            resolver: ProcessResolver,
            sink,
            manager_tokens,
            sessions: HashMap::new(),
            source_to_keys: HashMap::new(),
            last_events: HashMap::new(),
        })
    }

    fn handle_message(&mut self, message: SmtcWorkerMessage) -> Result<()> {
        match message {
            SmtcWorkerMessage::SessionsChanged => self.reconcile_sessions(),
            SmtcWorkerMessage::CurrentSessionChanged => self.emit_current_session_changed(),
            SmtcWorkerMessage::PlaybackInfoChanged { key } => {
                self.refresh_playback_for_key(&key)?;
                Ok(())
            }
            SmtcWorkerMessage::MediaPropertiesChanged { key } => {
                self.refresh_metadata_for_key(&key)?;
                Ok(())
            }
            SmtcWorkerMessage::TransportCommand {
                source_id,
                action,
                reply,
            } => {
                let result = self.execute_transport(source_id, action);
                let _ = reply.send(result);
                Ok(())
            }
        }
    }

    fn reconcile_sessions(&mut self) -> Result<()> {
        let sessions = self.manager.GetSessions()?;
        let mut observed = HashSet::new();

        for index in 0..sessions.Size()? {
            let session = sessions.GetAt(index)?;
            let source_app_user_model_id = session.SourceAppUserModelId()?.to_string_lossy();
            let source = self
                .resolver
                .resolve_media_source(&source_app_user_model_id);

            let tab_key = tab_key_from_session(&session);
            let Some(source) = self
                .identity_system
                .resolve_smtc_source(source, Some(&tab_key))
            else {
                continue;
            };

            let key = SmtcSessionKey::from_session(&source, &session);
            observed.insert(key.clone());

            if !self.sessions.contains_key(&key) {
                let tracked = TrackedSmtcSession::register(
                    key.clone(),
                    source.clone(),
                    session.clone(),
                    self.sink.clone(),
                )?;
                self.source_to_keys
                    .entry(source.id.clone())
                    .or_default()
                    .insert(key.clone());
                self.sessions.insert(key.clone(), tracked);
                tracing::info!(
                    source_id = %source.id,
                    source_kind = %source.kind,
                    source_capability = %source.capability,
                    source_type = %source.source_type,
                    source_app_user_model_id = %source.source_app_user_model_id,
                    process_id = source.process.as_ref().map(|process| process.process_id),
                    executable_path = source.process.as_ref().and_then(|process| process.executable_path.as_ref()).map(|path| path.display().to_string()),
                    "detected SMTC session"
                );
                self.refresh_playback_for_key(&key)?;
                self.refresh_metadata_for_key(&key)?;
            }
        }

        let stale = self
            .sessions
            .keys()
            .filter(|key| !observed.contains(*key))
            .cloned()
            .collect::<Vec<_>>();

        for key in stale {
            if let Some(removed) = self.sessions.remove(&key) {
                let mut last_session_for_source = false;
                if let Some(keys) = self.source_to_keys.get_mut(&removed.source.id) {
                    keys.remove(&key);
                    if keys.is_empty() {
                        self.source_to_keys.remove(&removed.source.id);
                        last_session_for_source = true;
                    }
                }
                if last_session_for_source {
                    // Per-tab browser sources outlive the SourceRegistry's
                    // PID-based stale collector (the browser process keeps
                    // running). Evict here when the last session for the
                    // source ends so the registry doesn't grow unboundedly.
                    self.identity_system.registry().remove(&removed.source.id);
                }
                self.emit(MediaEvent::MediaStopped {
                    source: removed.source.clone(),
                    metadata: removed.metadata.clone(),
                });
            }
        }

        Ok(())
    }

    fn emit_current_session_changed(&mut self) -> Result<()> {
        let session = self.manager.GetCurrentSession().ok();
        let source = session.as_ref().and_then(|session| {
            let aumid = session.SourceAppUserModelId().ok()?;
            let resolved = self.resolver.resolve_media_source(&aumid.to_string_lossy());
            let tab_key = tab_key_from_session(session);
            self.identity_system
                .resolve_smtc_source(resolved, Some(&tab_key))
        });

        self.emit(MediaEvent::ActiveSessionChanged { source });
        Ok(())
    }

    fn refresh_playback_for_key(&mut self, key: &SmtcSessionKey) -> Result<()> {
        let Some(tracked) = self.sessions.get_mut(key) else {
            return Ok(());
        };

        let playback_info = match tracked.session.GetPlaybackInfo() {
            Ok(playback_info) => playback_info,
            Err(error) => {
                tracing::warn!(%error, smtc_key = %key, "dropping invalid SMTC session reference");
                self.sessions.remove(key);
                return Ok(());
            }
        };

        let state = playback_state_from_smtc(playback_info.PlaybackStatus()?);
        if tracked.playback_state == state {
            return Ok(());
        }

        tracked.playback_state = state.clone();
        let source = tracked.source.clone();
        let metadata = tracked.metadata.clone();

        match state {
            PlaybackState::Playing => self.emit(MediaEvent::MediaStarted { source, metadata }),
            PlaybackState::Paused => self.emit(MediaEvent::MediaPaused { source, metadata }),
            PlaybackState::Stopped => self.emit(MediaEvent::MediaStopped { source, metadata }),
            PlaybackState::Unknown => {}
        }

        Ok(())
    }

    fn refresh_metadata_for_key(&mut self, key: &SmtcSessionKey) -> Result<()> {
        let Some(tracked) = self.sessions.get_mut(key) else {
            return Ok(());
        };

        let properties = match tracked.session.TryGetMediaPropertiesAsync() {
            Ok(operation) => operation.get(),
            Err(error) => Err(error),
        };
        let properties = match properties {
            Ok(properties) => properties,
            Err(error) => {
                tracing::warn!(%error, smtc_key = %key, "failed to read SMTC media metadata");
                return Ok(());
            }
        };

        let metadata = metadata_from_smtc(&properties);
        if tracked.metadata.fingerprint() == metadata.fingerprint() {
            return Ok(());
        }

        tracked.metadata = metadata.clone();
        let source = tracked.source.clone();
        let metadata_cloned = metadata.clone();
        
        self.emit(MediaEvent::MediaMetadataChanged {
            source,
            metadata: metadata_cloned,
        });
        Ok(())
    }

    fn execute_transport(
        &mut self,
        source_id: MediaSourceId,
        action: TransportAction,
    ) -> Result<TransportResult> {
        let keys = self
            .source_to_keys
            .get(&source_id)
            .cloned()
            .ok_or_else(|| AudioFocusError::Smtc(format!("unknown SMTC source {source_id}")))?;

        let mut all_accepted = true;
        for key in keys {
            let Some(tracked) = self.sessions.get(&key) else {
                continue;
            };

            let accepted_by_session = match action {
                TransportAction::Pause => tracked.session.TryPauseAsync()?.get()?,
                TransportAction::Play => tracked.session.TryPlayAsync()?.get()?,
            };

            if !accepted_by_session {
                all_accepted = false;
            }

            tracing::info!(
                source_id = %source_id,
                smtc_key = %key,
                action = ?action,
                accepted_by_session,
                "SMTC transport command completed for session"
            );
        }

        Ok(TransportResult {
            accepted_by_session: all_accepted,
        })
    }

    fn emit(&mut self, event: MediaEvent) {
        if self.is_duplicate(&event) {
            return;
        }

        log_media_event(&event);
        let _ = self.arbitration.submit(crate::arbitration::ArbitrationEvent::Media(event));
    }

    fn is_duplicate(&mut self, event: &MediaEvent) -> bool {
        let Some(source) = event.source() else {
            return false;
        };
        let key = (source.id.clone(), event.name());
        let now = Instant::now();
        if self
            .last_events
            .get(&key)
            .is_some_and(|previous| now.duration_since(*previous) < DUPLICATE_EVENT_WINDOW)
        {
            return true;
        }
        self.last_events.insert(key, now);
        false
    }
}

struct TrackedSmtcSession {
    source: MediaSource,
    session: GlobalSystemMediaTransportControlsSession,
    playback_state: PlaybackState,
    metadata: MediaMetadata,
    tokens: SessionEventTokens,
}

impl TrackedSmtcSession {
    fn register(
        key: SmtcSessionKey,
        source: MediaSource,
        session: GlobalSystemMediaTransportControlsSession,
        sink: Weak<SmtcMessageSink>,
    ) -> Result<Self> {
        let tokens = SessionEventTokens::register(&session, key, sink)?;
        Ok(Self {
            source,
            session,
            playback_state: PlaybackState::Unknown,
            metadata: MediaMetadata::default(),
            tokens,
        })
    }
}

impl Drop for TrackedSmtcSession {
    fn drop(&mut self) {
        self.tokens.unregister(&self.session);
    }
}

struct ManagerEventTokens {
    current_session_changed: EventRegistrationToken,
    sessions_changed: EventRegistrationToken,
}

impl ManagerEventTokens {
    fn register(
        manager: &GlobalSystemMediaTransportControlsSessionManager,
        sink: Weak<SmtcMessageSink>,
    ) -> Result<Self> {
        let current_sink = sink.clone();
        let current_session_changed =
            manager.CurrentSessionChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSessionManager,
                CurrentSessionChangedEventArgs,
            >::new(move |_sender, _args| {
                // WinRT event handlers stay non-blocking; the worker owns all reconciliation.
                if let Some(sink) = current_sink.upgrade() {
                    sink.send(SmtcWorkerMessage::CurrentSessionChanged);
                }
                Ok(())
            }))?;

        let sessions_changed =
            manager.SessionsChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSessionManager,
                SessionsChangedEventArgs,
            >::new(move |_sender, _args| {
                if let Some(sink) = sink.upgrade() {
                    sink.send(SmtcWorkerMessage::SessionsChanged);
                }
                Ok(())
            }))?;

        Ok(Self {
            current_session_changed,
            sessions_changed,
        })
    }

    fn unregister(&self, manager: &GlobalSystemMediaTransportControlsSessionManager) {
        let _ = manager.RemoveCurrentSessionChanged(self.current_session_changed);
        let _ = manager.RemoveSessionsChanged(self.sessions_changed);
    }
}

impl Drop for SmtcWatcherState {
    fn drop(&mut self) {
        self.manager_tokens.unregister(&self.manager);
    }
}

struct SessionEventTokens {
    playback_info_changed: EventRegistrationToken,
    media_properties_changed: EventRegistrationToken,
}

impl SessionEventTokens {
    fn register(
        session: &GlobalSystemMediaTransportControlsSession,
        key: SmtcSessionKey,
        sink: Weak<SmtcMessageSink>,
    ) -> Result<Self> {
        let playback_key = key.clone();
        let playback_sink = sink.clone();
        let playback_info_changed =
            session.PlaybackInfoChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSession,
                PlaybackInfoChangedEventArgs,
            >::new(move |_sender, _args| {
                // Playback and metadata are read on the SMTC worker to avoid callback reentrancy.
                if let Some(sink) = playback_sink.upgrade() {
                    sink.send(SmtcWorkerMessage::PlaybackInfoChanged {
                        key: playback_key.clone(),
                    });
                }
                Ok(())
            }))?;

        let media_properties_changed =
            session.MediaPropertiesChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSession,
                MediaPropertiesChangedEventArgs,
            >::new(move |_sender, _args| {
                if let Some(sink) = sink.upgrade() {
                    sink.send(SmtcWorkerMessage::MediaPropertiesChanged { key: key.clone() });
                }
                Ok(())
            }))?;

        Ok(Self {
            playback_info_changed,
            media_properties_changed,
        })
    }

    fn unregister(&self, session: &GlobalSystemMediaTransportControlsSession) {
        let _ = session.RemovePlaybackInfoChanged(self.playback_info_changed);
        let _ = session.RemoveMediaPropertiesChanged(self.media_properties_changed);
    }
}

struct WinRtMtaApartment;

impl WinRtMtaApartment {
    fn initialize() -> Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        Ok(Self)
    }
}

impl Drop for WinRtMtaApartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

/// Per-session discriminator for browser tab identity. The session pointer is
/// unique-and-stable for the SMTC session's lifetime; hashing avoids leaking a
/// raw memory address into the source id while preserving uniqueness.
fn tab_key_from_session(session: &GlobalSystemMediaTransportControlsSession) -> String {
    let ptr = session.as_raw() as usize;
    let mut hasher = DefaultHasher::new();
    ptr.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn log_media_event(event: &MediaEvent) {
    match event {
        MediaEvent::MediaStarted { source, metadata }
        | MediaEvent::MediaPaused { source, metadata }
        | MediaEvent::MediaStopped { source, metadata }
        | MediaEvent::MediaMetadataChanged { source, metadata } => {
            tracing::info!(
                event = event.name(),
                source_id = %source.id,
                source_kind = %source.kind,
                source_capability = %source.capability,
                source_type = %source.source_type,
                source_app_user_model_id = %source.source_app_user_model_id,
                process_id = source.process.as_ref().map(|process| process.process_id),
                executable_path = source.process.as_ref().and_then(|process| process.executable_path.as_ref()).map(|path| path.display().to_string()),
                title = %metadata.title,
                artist = %metadata.artist,
                album_title = %metadata.album_title,
                "SMTC media event"
            );
        }
        MediaEvent::ActiveSessionChanged { source } => {
            tracing::info!(
                event = event.name(),
                source_id = source.as_ref().map(|source| source.id.as_str().to_string()),
                source_kind = source.as_ref().map(|source| source.kind.to_string()),
                process_id = source
                    .as_ref()
                    .and_then(|source| source.process.as_ref())
                    .map(|process| process.process_id),
                "SMTC active session changed"
            );
        }
    }
}
