use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

use crate::media_source::{MediaSource, MediaSourceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PauseOrigin {
    AudioFocus { generation_id: u64 },
    External,
}

#[derive(Clone, Debug)]
pub struct PlaybackRecord {
    pub source: MediaSource,
    pub last_started_at: Option<Instant>,
    pub last_paused_at: Option<Instant>,
    pub last_pause_origin: Option<PauseOrigin>,
    pub generation_id: u64,
}

impl PlaybackRecord {
    pub fn new(source: MediaSource) -> Self {
        Self {
            source,
            last_started_at: None,
            last_paused_at: None,
            last_pause_origin: None,
            generation_id: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PauseCommandRecord {
    pub paused_source: MediaSource,
    pub requested_by: MediaSource,
    pub requested_at: Instant,
    pub completed: bool,
    pub rollback_active_on_failure: bool,
}

#[derive(Clone, Debug)]
pub struct ArbitrationSnapshot {
    pub currently_active_source: Option<MediaSourceId>,
    pub previously_paused_sources: Vec<MediaSourceId>,
    pub event_generation_id: u64,
    pub playback_history_len: usize,
}

#[derive(Debug)]
pub struct ArbitrationState {
    pub currently_active_source: Option<MediaSourceId>,
    pub sources: HashMap<MediaSourceId, PlaybackRecord>,
    pub previously_paused_sources: HashMap<MediaSourceId, PauseCommandRecord>,
    pub pending_pauses: HashMap<u64, PauseCommandRecord>,
    pub playback_history: VecDeque<(u64, MediaSourceId, &'static str, Instant)>,
    pub event_generation_id: u64,
}

impl ArbitrationState {
    pub fn new() -> Self {
        Self {
            currently_active_source: None,
            sources: HashMap::new(),
            previously_paused_sources: HashMap::new(),
            pending_pauses: HashMap::new(),
            playback_history: VecDeque::with_capacity(256),
            event_generation_id: 0,
        }
    }

    pub fn next_generation(&mut self) -> u64 {
        self.event_generation_id = self.event_generation_id.saturating_add(1);
        self.event_generation_id
    }

    pub fn upsert_source(&mut self, source: MediaSource) -> &mut PlaybackRecord {
        self.sources
            .entry(source.id.clone())
            .and_modify(|record| record.source = source.clone())
            .or_insert_with(|| PlaybackRecord::new(source))
    }

    pub fn push_history(&mut self, source_id: MediaSourceId, event_name: &'static str) {
        self.playback_history
            .push_back((self.event_generation_id, source_id, event_name, Instant::now()));
        while self.playback_history.len() > 256 {
            self.playback_history.pop_front();
        }
    }

    pub fn snapshot(&self) -> ArbitrationSnapshot {
        ArbitrationSnapshot {
            currently_active_source: self.currently_active_source.clone(),
            previously_paused_sources: self.previously_paused_sources.keys().cloned().collect(),
            event_generation_id: self.event_generation_id,
            playback_history_len: self.playback_history.len(),
        }
    }
}

impl Default for ArbitrationState {
    fn default() -> Self {
        Self::new()
    }
}
