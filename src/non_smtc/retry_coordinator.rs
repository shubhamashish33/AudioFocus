use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{
    com::MtaApartment,
    error::{AudioFocusError, Result},
    media_source::{MediaSource, MediaSourceId},
    process::resolve_process,
    shutdown::ShutdownSignal,
};

use super::{
    dispatcher::{send_media_command, NonSmtcTransportAction},
    validation::WasapiPlaybackValidator,
    window_discovery::enumerate_top_level_windows_for_process,
    window_filtering::ranked_playback_windows,
};

#[derive(Clone, Copy, Debug)]
pub struct RetryConfig {
    pub retry_count: u8,
    pub retry_delay: Duration,
    pub dispatch_timeout: Duration,
    pub validation_timeout: Duration,
    pub validation_interval: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            retry_count: 3,
            retry_delay: Duration::from_millis(150),
            dispatch_timeout: Duration::from_millis(500),
            validation_timeout: Duration::from_secs(2),
            validation_interval: Duration::from_millis(100),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NonSmtcTransportRequest {
    pub source: MediaSource,
    pub action: NonSmtcTransportAction,
    pub config: RetryConfig,
}

#[derive(Clone, Debug)]
pub struct NonSmtcTransportResult {
    pub source_id: MediaSourceId,
    pub process_id: u32,
    pub success: bool,
    pub action: NonSmtcTransportAction,
    pub attempts: u8,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum NonSmtcControllerEvent {
    WindowNotFound {
        source_id: MediaSourceId,
        process_id: u32,
        attempt: u8,
    },
    TransportValidated {
        source_id: MediaSourceId,
        process_id: u32,
        action: NonSmtcTransportAction,
        attempts: u8,
    },
    ControllerError {
        source_id: MediaSourceId,
        process_id: u32,
        attempts: u8,
        error: String,
    },
}

#[derive(Clone, Debug)]
pub struct NonSmtcPauseController {
    sender: Sender<ControllerMessage>,
}

impl NonSmtcPauseController {
    pub fn start(shutdown: ShutdownSignal) -> Result<Self> {
        Self::start_with_events(shutdown, None)
    }

    pub fn start_with_events(
        shutdown: ShutdownSignal,
        event_sender: Option<Sender<NonSmtcControllerEvent>>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name("non-smtc-pause-controller".to_string())
            .spawn(move || {
                if let Err(error) = run_controller_worker(shutdown, receiver, event_sender) {
                    tracing::error!(%error, "non-SMTC pause controller stopped with an error");
                }
            })
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?;

        Ok(Self { sender })
    }

    pub fn pause(
        &self,
        source: MediaSource,
        config: RetryConfig,
    ) -> Result<Receiver<NonSmtcTransportResult>> {
        self.execute(source, NonSmtcTransportAction::Pause, config)
    }

    pub fn play(
        &self,
        source: MediaSource,
        config: RetryConfig,
    ) -> Result<Receiver<NonSmtcTransportResult>> {
        self.execute(source, NonSmtcTransportAction::Play, config)
    }

    fn execute(
        &self,
        source: MediaSource,
        action: NonSmtcTransportAction,
        config: RetryConfig,
    ) -> Result<Receiver<NonSmtcTransportResult>> {
        let (reply, receiver) = mpsc::channel();
        self.sender
            .send(ControllerMessage::Execute {
                request: NonSmtcTransportRequest {
                    source,
                    action,
                    config,
                },
                reply,
            })
            .map_err(|error| AudioFocusError::NonSmtc(error.to_string()))?;
        Ok(receiver)
    }
}

#[derive(Clone, Debug)]
enum ControllerMessage {
    Execute {
        request: NonSmtcTransportRequest,
        reply: Sender<NonSmtcTransportResult>,
    },
}

fn run_controller_worker(
    shutdown: ShutdownSignal,
    receiver: Receiver<ControllerMessage>,
    event_sender: Option<Sender<NonSmtcControllerEvent>>,
) -> Result<()> {
    while !shutdown.is_requested() {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ControllerMessage::Execute { request, reply }) => {
                let event_sender = event_sender.clone();
                thread::Builder::new()
                    .name("non-smtc-pause-attempt".to_string())
                    .spawn(move || {
                        let result = execute_transport_request(request, event_sender);
                        let _ = reply.send(result);
                    })
                    .map_err(|error| AudioFocusError::Thread(error.to_string()))?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

fn execute_transport_request(
    request: NonSmtcTransportRequest,
    event_sender: Option<Sender<NonSmtcControllerEvent>>,
) -> NonSmtcTransportResult {
    let source_id = request.source.id.clone();
    let process_id = request
        .source
        .process
        .as_ref()
        .map(|process| process.process_id)
        .unwrap_or_default();

    let _com = match MtaApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            return failure_result(
                source_id,
                process_id,
                request.action,
                0,
                error.to_string(),
                event_sender.as_ref(),
            );
        }
    };

    let mut validator = match WasapiPlaybackValidator::new() {
        Ok(validator) => validator,
        Err(error) => {
            return failure_result(
                source_id,
                process_id,
                request.action,
                0,
                error.to_string(),
                event_sender.as_ref(),
            );
        }
    };

    execute_transport_request_inner(request, &mut validator, event_sender.as_ref())
}

fn execute_transport_request_inner(
    request: NonSmtcTransportRequest,
    validator: &mut WasapiPlaybackValidator,
    event_sender: Option<&Sender<NonSmtcControllerEvent>>,
) -> NonSmtcTransportResult {
    let Some(process) = request.source.process.as_ref() else {
        return failure_result(
            request.source.id,
            0,
            request.action,
            0,
            "media source has no process identity",
            event_sender,
        );
    };

    let process_id = process.process_id;
    if !process_identity_is_current(process) {
        return failure_result(
            request.source.id,
            process_id,
            request.action,
            0,
            "media source process identity is stale",
            event_sender,
        );
    }
    let mut last_error = None;
    let max_attempts = request.config.retry_count.max(1);

    for attempt in 1..=max_attempts {
        tracing::info!(
            source_id = %request.source.id,
            process_id,
            attempt,
            max_attempts,
            action = request.action.name(),
            "starting non-SMTC transport attempt"
        );

        let windows = enumerate_top_level_windows_for_process(process_id);
        let ranked = ranked_playback_windows(windows);
        let Some(target) = ranked.get((attempt as usize - 1) % ranked.len().max(1)) else {
            last_error = Some("no valid playback window found".to_string());
            tracing::warn!(
                source_id = %request.source.id,
                process_id,
                attempt,
                "no valid non-SMTC playback window found"
            );
            emit_controller_event(
                event_sender,
                NonSmtcControllerEvent::WindowNotFound {
                    source_id: request.source.id.clone(),
                    process_id,
                    attempt,
                },
            );
            thread::sleep(request.config.retry_delay);
            continue;
        };

        let dispatch = send_media_command(
            target.candidate.handle,
            process_id,
            request.action,
            request.config.dispatch_timeout,
        );

        if !dispatch.attempted {
            last_error = Some(format!(
                "WM_APPCOMMAND {} dispatch was not attempted",
                request.action.name()
            ));
            thread::sleep(request.config.retry_delay);
            continue;
        }

        match validator.wait_for_transport_state(
            process_id,
            request.action,
            request.config.validation_timeout,
            request.config.validation_interval,
        ) {
            Ok(true) => {
                tracing::info!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    hwnd = target.candidate.handle.0,
                    action = request.action.name(),
                    "non-SMTC transport validated by WASAPI state"
                );
                emit_controller_event(
                    event_sender,
                    NonSmtcControllerEvent::TransportValidated {
                        source_id: request.source.id.clone(),
                        process_id,
                        action: request.action,
                        attempts: attempt,
                    },
                );
                return NonSmtcTransportResult {
                    source_id: request.source.id,
                    process_id,
                    success: true,
                    action: request.action,
                    attempts: attempt,
                    last_error: None,
                };
            }
            Ok(false) => {
                last_error = Some(format!(
                    "WASAPI did not reach expected state after {}",
                    request.action.name()
                ));
                tracing::warn!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    action = request.action.name(),
                    "non-SMTC transport validation failed"
                );
            }
            Err(error) => {
                last_error = Some(error.to_string());
                tracing::warn!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    %error,
                    action = request.action.name(),
                    "non-SMTC transport validation errored"
                );
            }
        }

        thread::sleep(request.config.retry_delay);
    }

    tracing::error!(
        source_id = %request.source.id,
        process_id,
        attempts = max_attempts,
        action = request.action.name(),
        last_error = last_error.as_deref(),
        "non-SMTC transport failed after retries"
    );
    emit_controller_event(
        event_sender,
        NonSmtcControllerEvent::ControllerError {
            source_id: request.source.id.clone(),
            process_id,
            attempts: max_attempts,
            error: last_error
                .clone()
                .unwrap_or_else(|| "transport command failed after retries".to_string()),
        },
    );

    NonSmtcTransportResult {
        source_id: request.source.id,
        process_id,
        success: false,
        action: request.action,
        attempts: max_attempts,
        last_error,
    }
}

fn process_identity_is_current(process: &crate::media_source::ProcessIdentity) -> bool {
    let current = resolve_process(process.process_id, process.executable_name.clone());
    process.creation_time == 0
        || current.creation_time == 0
        || process.creation_time == current.creation_time
}

fn failure_result(
    source_id: MediaSourceId,
    process_id: u32,
    action: NonSmtcTransportAction,
    attempts: u8,
    error: impl Into<String>,
    event_sender: Option<&Sender<NonSmtcControllerEvent>>,
) -> NonSmtcTransportResult {
    let error = error.into();
    tracing::error!(
        source_id = %source_id,
        process_id,
        action = action.name(),
        attempts,
        error = %error,
        "non-SMTC transport request failed"
    );
    emit_controller_event(
        event_sender,
        NonSmtcControllerEvent::ControllerError {
            source_id: source_id.clone(),
            process_id,
            attempts,
            error: error.clone(),
        },
    );
    NonSmtcTransportResult {
        source_id,
        process_id,
        success: false,
        action,
        attempts,
        last_error: Some(error),
    }
}

fn emit_controller_event(
    event_sender: Option<&Sender<NonSmtcControllerEvent>>,
    event: NonSmtcControllerEvent,
) {
    if let Some(sender) = event_sender {
        if let Err(error) = sender.send(event) {
            tracing::warn!(%error, "failed to emit non-SMTC controller event");
        }
    }
}
