use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{
    com::MtaApartment,
    error::{AudioFocusError, Result},
    media_source::{MediaSource, MediaSourceId},
    shutdown::ShutdownSignal,
};

use super::{
    dispatcher::send_media_pause, validation::WasapiPlaybackValidator,
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
pub struct NonSmtcPauseRequest {
    pub source: MediaSource,
    pub config: RetryConfig,
}

#[derive(Clone, Debug)]
pub struct NonSmtcPauseResult {
    pub source_id: MediaSourceId,
    pub process_id: u32,
    pub success: bool,
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
    PauseValidated {
        source_id: MediaSourceId,
        process_id: u32,
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
    ) -> Result<Receiver<NonSmtcPauseResult>> {
        let (reply, receiver) = mpsc::channel();
        self.sender
            .send(ControllerMessage::Pause {
                request: NonSmtcPauseRequest { source, config },
                reply,
            })
            .map_err(|error| AudioFocusError::NonSmtc(error.to_string()))?;
        Ok(receiver)
    }
}

#[derive(Clone, Debug)]
enum ControllerMessage {
    Pause {
        request: NonSmtcPauseRequest,
        reply: Sender<NonSmtcPauseResult>,
    },
}

fn run_controller_worker(
    shutdown: ShutdownSignal,
    receiver: Receiver<ControllerMessage>,
    event_sender: Option<Sender<NonSmtcControllerEvent>>,
) -> Result<()> {
    while !shutdown.is_requested() {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ControllerMessage::Pause { request, reply }) => {
                let event_sender = event_sender.clone();
                thread::Builder::new()
                    .name("non-smtc-pause-attempt".to_string())
                    .spawn(move || {
                        let result = execute_pause_request(request, event_sender);
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

fn execute_pause_request(
    request: NonSmtcPauseRequest,
    event_sender: Option<Sender<NonSmtcControllerEvent>>,
) -> NonSmtcPauseResult {
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
                0,
                error.to_string(),
                event_sender.as_ref(),
            );
        }
    };

    execute_pause_request_inner(request, &mut validator, event_sender.as_ref())
}

fn execute_pause_request_inner(
    request: NonSmtcPauseRequest,
    validator: &mut WasapiPlaybackValidator,
    event_sender: Option<&Sender<NonSmtcControllerEvent>>,
) -> NonSmtcPauseResult {
    let Some(process) = request.source.process.as_ref() else {
        return failure_result(
            request.source.id,
            0,
            0,
            "media source has no process identity",
            event_sender,
        );
    };

    let process_id = process.process_id;
    let mut last_error = None;
    let max_attempts = request.config.retry_count.max(1);

    for attempt in 1..=max_attempts {
        tracing::info!(
            source_id = %request.source.id,
            process_id,
            attempt,
            max_attempts,
            "starting non-SMTC pause attempt"
        );

        let windows = enumerate_top_level_windows_for_process(process_id);
        let ranked = ranked_playback_windows(windows);
        let Some(target) = ranked.first() else {
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

        let dispatch = send_media_pause(
            target.candidate.handle,
            process_id,
            request.config.dispatch_timeout,
        );

        if !dispatch.attempted {
            last_error = Some("WM_APPCOMMAND dispatch was not attempted".to_string());
            thread::sleep(request.config.retry_delay);
            continue;
        }

        match validator.wait_until_inactive(
            process_id,
            request.config.validation_timeout,
            request.config.validation_interval,
        ) {
            Ok(true) => {
                tracing::info!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    hwnd = target.candidate.handle.0,
                    "non-SMTC pause validated by WASAPI inactivity"
                );
                emit_controller_event(
                    event_sender,
                    NonSmtcControllerEvent::PauseValidated {
                        source_id: request.source.id.clone(),
                        process_id,
                        attempts: attempt,
                    },
                );
                return NonSmtcPauseResult {
                    source_id: request.source.id,
                    process_id,
                    success: true,
                    attempts: attempt,
                    last_error: None,
                };
            }
            Ok(false) => {
                last_error = Some("WASAPI activity remained active after pause".to_string());
                tracing::warn!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    "non-SMTC pause validation failed"
                );
            }
            Err(error) => {
                last_error = Some(error.to_string());
                tracing::warn!(
                    source_id = %request.source.id,
                    process_id,
                    attempt,
                    %error,
                    "non-SMTC pause validation errored"
                );
            }
        }

        thread::sleep(request.config.retry_delay);
    }

    tracing::error!(
        source_id = %request.source.id,
        process_id,
        attempts = max_attempts,
        last_error = last_error.as_deref(),
        "non-SMTC pause failed after retries"
    );
    emit_controller_event(
        event_sender,
        NonSmtcControllerEvent::ControllerError {
            source_id: request.source.id.clone(),
            process_id,
            attempts: max_attempts,
            error: last_error
                .clone()
                .unwrap_or_else(|| "pause failed after retries".to_string()),
        },
    );

    NonSmtcPauseResult {
        source_id: request.source.id,
        process_id,
        success: false,
        attempts: max_attempts,
        last_error,
    }
}

fn failure_result(
    source_id: MediaSourceId,
    process_id: u32,
    attempts: u8,
    error: impl Into<String>,
    event_sender: Option<&Sender<NonSmtcControllerEvent>>,
) -> NonSmtcPauseResult {
    let error = error.into();
    tracing::error!(
        source_id = %source_id,
        process_id,
        attempts,
        error = %error,
        "non-SMTC pause request failed"
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
    NonSmtcPauseResult {
        source_id,
        process_id,
        success: false,
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
