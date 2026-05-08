use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::media_source::MediaSourceId;

#[derive(Debug)]
pub struct DebounceCoordinator {
    window: Duration,
    last_seen: HashMap<(MediaSourceId, &'static str), Instant>,
}

impl DebounceCoordinator {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last_seen: HashMap::new(),
        }
    }

    pub fn should_drop(&mut self, source_id: &MediaSourceId, event_name: &'static str) -> bool {
        let key = (source_id.clone(), event_name);
        let now = Instant::now();
        if self
            .last_seen
            .get(&key)
            .is_some_and(|last_seen| now.duration_since(*last_seen) < self.window)
        {
            tracing::info!(
                source_id = %source_id,
                event = event_name,
                debounce_ms = self.window.as_millis(),
                "arbitration debounce dropped duplicate event"
            );
            return true;
        }
        self.last_seen.insert(key, now);
        false
    }
}
