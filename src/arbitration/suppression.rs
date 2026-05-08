use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::media_source::MediaSourceId;

#[derive(Clone, Debug)]
pub struct SuppressionEntry {
    pub generation_id: u64,
    pub expires_at: Instant,
}

#[derive(Debug)]
pub struct SuppressionWindows {
    window: Duration,
    entries: HashMap<MediaSourceId, SuppressionEntry>,
}

impl SuppressionWindows {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            entries: HashMap::new(),
        }
    }

    pub fn suppress_pause_event(&mut self, source_id: MediaSourceId, generation_id: u64) {
        self.entries.insert(
            source_id,
            SuppressionEntry {
                generation_id,
                expires_at: Instant::now() + self.window,
            },
        );
    }

    pub fn consume_if_suppressed(&mut self, source_id: &MediaSourceId) -> Option<u64> {
        self.prune_expired();
        self.entries.remove(source_id).map(|entry| entry.generation_id)
    }

    fn prune_expired(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| entry.expires_at > now);
    }
}
