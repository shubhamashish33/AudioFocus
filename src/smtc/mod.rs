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
    source_app_user_model_id: String,
}

impl SmtcSessionKey {
    pub fn from_source(source: &MediaSource) -> Self {
        Self {
            source_id: source.id.clone(),
            source_app_user_model_id: source.source_app_user_model_id.clone(),
        }
    }

    pub fn source_id(&self) -> &MediaSourceId {
        &self.source_id
    }

    pub fn source_app_user_model_id(&self) -> &str {
        &self.source_app_user_model_id
    }
}

impl fmt::Display for SmtcSessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}",
            self.source_id, self.source_app_user_model_id
        )
    }
}
