use std::sync::mpsc::{self, Sender};

use crate::{error::AudioFocusError, media_source::MediaSourceId};

use super::watcher::SmtcWorkerMessage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportAction {
    Pause,
    Play,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportResult {
    pub accepted_by_session: bool,
}

#[derive(Clone, Debug)]
pub struct SmtcTransportController {
    sender: Sender<SmtcWorkerMessage>,
}

impl SmtcTransportController {
    pub(crate) fn new(sender: Sender<SmtcWorkerMessage>) -> Self {
        Self { sender }
    }

    pub fn pause(&self, source_id: MediaSourceId) -> crate::error::Result<TransportResult> {
        self.send(source_id, TransportAction::Pause)
    }

    pub fn play(&self, source_id: MediaSourceId) -> crate::error::Result<TransportResult> {
        self.send(source_id, TransportAction::Play)
    }

    fn send(
        &self,
        source_id: MediaSourceId,
        action: TransportAction,
    ) -> crate::error::Result<TransportResult> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.sender
            .send(SmtcWorkerMessage::TransportCommand {
                source_id,
                action,
                reply: reply_sender,
            })
            .map_err(|error| AudioFocusError::Smtc(error.to_string()))?;

        reply_receiver
            .recv()
            .map_err(|error| AudioFocusError::Smtc(error.to_string()))?
    }
}
