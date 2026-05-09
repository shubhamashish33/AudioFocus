use std::time::Instant;

use crate::media_source::{MediaSource, MediaSourceId};

use super::state::{ArbitrationState, PauseCommandRecord, PauseOrigin};

pub fn promote_active(state: &mut ArbitrationState, source: MediaSource, generation_id: u64) {
    let record = state.upsert_source(source.clone());
    record.last_started_at = Some(Instant::now());
    record.generation_id = generation_id;
    state.currently_active_source = Some(source.id.clone());
    state.push_history(source.id, "MediaStarted");
}

pub fn mark_paused_by_audiofocus(
    state: &mut ArbitrationState,
    paused_source: MediaSource,
    requested_by: MediaSource,
    generation_id: u64,
    rollback_active_on_failure: bool,
) {
    let record = PauseCommandRecord {
        paused_source: paused_source.clone(),
        requested_by,
        requested_at: Instant::now(),
        completed: false,
        rollback_active_on_failure,
    };
    state.pending_pauses.insert(generation_id, record.clone());
    state
        .previously_paused_sources
        .insert(paused_source.id.clone(), record);
}

pub fn mark_pause_observed(
    state: &mut ArbitrationState,
    source_id: &MediaSourceId,
    origin: PauseOrigin,
) {
    if let Some(record) = state.sources.get_mut(source_id) {
        record.last_paused_at = Some(Instant::now());
        record.last_pause_origin = Some(origin);
    }
    if state.currently_active_source.as_ref() == Some(source_id) {
        state.currently_active_source = None;
    }
    state.push_history(source_id.clone(), "MediaPaused");
}

pub fn remove_source(state: &mut ArbitrationState, source_id: &MediaSourceId, event_name: &'static str) {
    state.sources.remove(source_id);
    state.previously_paused_sources.remove(source_id);
    state.pending_pauses
        .retain(|_, pause| &pause.paused_source.id != source_id && &pause.requested_by.id != source_id);
    if state.currently_active_source.as_ref() == Some(source_id) {
        state.currently_active_source = None;
    }
    state.push_history(source_id.clone(), event_name);
}
