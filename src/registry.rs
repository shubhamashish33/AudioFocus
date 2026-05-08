use std::collections::{HashMap, HashSet};

use crate::events::{AudioSessionEvent, AudioSessionSnapshot};

const STOP_CONFIRMATION_POLLS: u8 = 3;

#[derive(Debug, Default)]
pub struct AudioSessionRegistry {
    sessions_by_process: HashMap<u32, TrackedAudioSession>,
}

impl AudioSessionRegistry {
    pub fn reconcile(
        &mut self,
        mut snapshots: Vec<AudioSessionSnapshot>,
    ) -> Vec<AudioSessionEvent> {
        snapshots.retain(AudioSessionSnapshot::is_live);
        snapshots.sort_by_key(|snapshot| snapshot.process_id);

        let mut next = HashMap::with_capacity(snapshots.len());
        for snapshot in snapshots {
            next.insert(snapshot.process_id, snapshot);
        }

        let previous_processes = self
            .sessions_by_process
            .keys()
            .copied()
            .collect::<HashSet<u32>>();
        let next_processes = next.keys().copied().collect::<HashSet<u32>>();

        let mut events = Vec::new();

        for process_id in next_processes.difference(&previous_processes) {
            if let Some(snapshot) = next.get(process_id) {
                events.push(AudioSessionEvent::SessionStarted(snapshot.clone()));
                if snapshot.is_active() {
                    events.push(AudioSessionEvent::SessionBecameActive(snapshot.clone()));
                }
            }
        }

        for process_id in previous_processes.intersection(&next_processes) {
            let previous = &self.sessions_by_process[process_id].snapshot;
            let current = &next[process_id];

            if !previous.is_active() && current.is_active() {
                events.push(AudioSessionEvent::SessionBecameActive(current.clone()));
            } else if previous.is_active() && !current.is_active() {
                events.push(AudioSessionEvent::SessionBecameInactive(current.clone()));
            }
        }

        for process_id in previous_processes.difference(&next_processes) {
            if let Some(tracked) = self.sessions_by_process.get_mut(process_id) {
                tracked.missing_polls = tracked.missing_polls.saturating_add(1);
                if tracked.missing_polls >= STOP_CONFIRMATION_POLLS {
                    events.push(AudioSessionEvent::SessionStopped(tracked.snapshot.clone()));
                } else {
                    next.insert(*process_id, tracked.snapshot.clone());
                }
            }
        }

        events.sort_by_key(|event| event.snapshot().process_id);
        self.sessions_by_process = next
            .into_iter()
            .map(|(process_id, snapshot)| {
                (
                    process_id,
                    TrackedAudioSession {
                        snapshot,
                        missing_polls: 0,
                    },
                )
            })
            .collect();
        events
    }

    pub fn len(&self) -> usize {
        self.sessions_by_process.len()
    }
}

#[derive(Clone, Debug)]
struct TrackedAudioSession {
    snapshot: AudioSessionSnapshot,
    missing_polls: u8,
}
