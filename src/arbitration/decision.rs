use crate::media_source::MediaSource;

use super::state::ArbitrationState;

#[derive(Clone, Debug)]
pub enum ArbitrationDecision {
    Noop {
        reason: &'static str,
    },
    Promote {
        source: MediaSource,
    },
    Switch {
        from: MediaSource,
        to: MediaSource,
    },
    RejectChallenger {
        challenger: MediaSource,
        active: MediaSource,
    },
}

pub fn decide_started(
    state: &ArbitrationState,
    source: &MediaSource,
    simultaneous_conflict: bool,
) -> ArbitrationDecision {
    match state.currently_active_source.as_ref() {
        None => ArbitrationDecision::Promote {
            source: source.clone(),
        },
        Some(active_id) if active_id == &source.id => ArbitrationDecision::Noop {
            reason: "source already owns playback",
        },
        Some(active_id) => {
            let Some(active_record) = state.sources.get(active_id) else {
                return ArbitrationDecision::Promote {
                    source: source.clone(),
                };
            };

            if simultaneous_conflict
                && deterministic_winner(&active_record.source, source).id != source.id
            {
                ArbitrationDecision::RejectChallenger {
                    challenger: source.clone(),
                    active: active_record.source.clone(),
                }
            } else {
                ArbitrationDecision::Switch {
                    from: active_record.source.clone(),
                    to: source.clone(),
                }
            }
        }
    }
}

fn deterministic_winner<'a>(left: &'a MediaSource, right: &'a MediaSource) -> &'a MediaSource {
    // If events are effectively simultaneous, the lexicographically stable source id wins.
    // Repeated runs then make the same arbitration choice regardless of callback ordering.
    if right.id.as_str() < left.id.as_str() {
        right
    } else {
        left
    }
}
