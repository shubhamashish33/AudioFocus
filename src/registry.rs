use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::events::{AudioSessionEvent, AudioSessionSnapshot};
use crate::identity::IdentitySystem;
use crate::media_source::{MediaSource, MediaSourceId};

const STOP_CONFIRMATION_POLLS: u8 = 3;

#[derive(Debug)]
pub struct AudioSessionRegistry {
    identity_system: Arc<IdentitySystem>,
    sessions_by_id: HashMap<MediaSourceId, TrackedAudioSession>,
}

impl Default for AudioSessionRegistry {
    fn default() -> Self {
        Self::new(Arc::new(IdentitySystem::default()))
    }
}

impl AudioSessionRegistry {
    pub fn new(identity_system: Arc<IdentitySystem>) -> Self {
        Self {
            identity_system,
            sessions_by_id: HashMap::new(),
        }
    }

    pub fn reconcile(
        &mut self,
        snapshots: Vec<AudioSessionSnapshot>,
    ) -> Vec<(AudioSessionEvent, MediaSource)> {
        let mut next: HashMap<MediaSourceId, (MediaSource, AudioSessionSnapshot, u8)> =
            HashMap::with_capacity(snapshots.len());

        for snapshot in snapshots {
            if !snapshot.is_live() {
                continue;
            }

            if let Some(source) = self.identity_system.resolve_wasapi_session(&snapshot) {
                next.insert(source.id.clone(), (source, snapshot, 0));
            }
        }

        let previous_ids = self
            .sessions_by_id
            .keys()
            .cloned()
            .collect::<HashSet<MediaSourceId>>();
        let next_ids = next.keys().cloned().collect::<HashSet<MediaSourceId>>();

        let mut events: Vec<(AudioSessionEvent, MediaSource)> = Vec::new();

        for id in next_ids.difference(&previous_ids) {
            if let Some((source, snapshot, _)) = next.get(id) {
                events.push((
                    AudioSessionEvent::SessionStarted(snapshot.clone()),
                    source.clone(),
                ));
                if snapshot.is_active() && snapshot.is_audible() {
                    events.push((
                        AudioSessionEvent::SessionBecameActive(snapshot.clone()),
                        source.clone(),
                    ));
                }
            }
        }

        for id in previous_ids.intersection(&next_ids) {
            let previous = &self.sessions_by_id[id].snapshot;
            let (source, current, _) = &next[id];

            let was_playing = previous.is_active() && previous.is_audible();
            let is_playing = current.is_active() && current.is_audible();

            if !was_playing && is_playing {
                events.push((
                    AudioSessionEvent::SessionBecameActive(current.clone()),
                    source.clone(),
                ));
            } else if was_playing && !is_playing {
                events.push((
                    AudioSessionEvent::SessionBecameInactive(current.clone()),
                    source.clone(),
                ));
            }
        }

        for id in previous_ids.difference(&next_ids) {
            if let Some(tracked) = self.sessions_by_id.get(id) {
                let bumped = tracked.missing_polls.saturating_add(1);
                if bumped >= STOP_CONFIRMATION_POLLS {
                    events.push((
                        AudioSessionEvent::SessionStopped(tracked.snapshot.clone()),
                        tracked.source.clone(),
                    ));
                } else {
                    next.insert(
                        id.clone(),
                        (tracked.source.clone(), tracked.snapshot.clone(), bumped),
                    );
                }
            }
        }

        self.sessions_by_id = next
            .into_iter()
            .map(|(id, (source, snapshot, missing_polls))| {
                (
                    id,
                    TrackedAudioSession {
                        source,
                        snapshot,
                        missing_polls,
                    },
                )
            })
            .collect();

        events
    }

    pub fn is_empty(&self) -> bool {
        self.sessions_by_id.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sessions_by_id.len()
    }
}

#[derive(Clone, Debug)]
struct TrackedAudioSession {
    source: crate::media_source::MediaSource,
    snapshot: AudioSessionSnapshot,
    missing_polls: u8,
}
