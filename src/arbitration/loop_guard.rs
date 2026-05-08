use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::media_source::MediaSourceId;

#[derive(Debug)]
pub struct PauseLoopGuard {
    window: Duration,
    max_pauses_per_window: usize,
    pauses: HashMap<MediaSourceId, Vec<Instant>>,
}

impl PauseLoopGuard {
    pub fn new(window: Duration, max_pauses_per_window: usize) -> Self {
        Self {
            window,
            max_pauses_per_window,
            pauses: HashMap::new(),
        }
    }

    pub fn allow_pause(&mut self, source_id: &MediaSourceId) -> bool {
        let now = Instant::now();
        let entries = self.pauses.entry(source_id.clone()).or_default();
        entries.retain(|instant| now.duration_since(*instant) <= self.window);
        if entries.len() >= self.max_pauses_per_window {
            tracing::warn!(
                source_id = %source_id,
                window_ms = self.window.as_millis(),
                pause_count = entries.len(),
                "arbitration pause loop guard blocked pause command"
            );
            return false;
        }
        entries.push(now);
        true
    }
}
