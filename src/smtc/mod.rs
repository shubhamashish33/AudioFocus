mod controller;
mod manager;
mod translate;
mod watcher;

use std::fmt;

use crate::media_source::{MediaSource, MediaSourceId};

pub use controller::{SmtcTransportController, TransportAction, TransportResult};
pub use manager::SmtcRuntime;
pub use watcher::SmtcWorkerMessage;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SmtcSessionKey {
    source_id: MediaSourceId,
    session_ptr: isize,
}

impl SmtcSessionKey {
    pub fn from_session(source: &MediaSource, session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession) -> Self {
        use windows::core::Interface;
        Self {
            source_id: source.id.clone(),
            session_ptr: session.as_raw() as isize,
        }
    }

    pub fn source_id(&self) -> &MediaSourceId {
        &self.source_id
    }
}

impl fmt::Display for SmtcSessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{:p}",
            self.source_id, self.session_ptr as *const ()
        )
    }
}
