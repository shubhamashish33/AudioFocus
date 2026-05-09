use crate::media_source::{MediaSource, MediaSourceKind, SourceType};
use crate::identity::source_registry::SourceRegistry;

#[derive(Debug)]
pub struct SessionReconciler {
    registry: std::sync::Arc<SourceRegistry>,
}

impl SessionReconciler {
    pub fn new(registry: std::sync::Arc<SourceRegistry>) -> Self {
        Self {
            registry,
        }
    }

    pub fn reconcile_wasapi_session(&self, mut source: MediaSource) -> MediaSource {
        source.source_type = SourceType::NonSmtc;

        // Browser WASAPI is suppressed at the IdentitySystem layer (audio
        // service is per-process, not per-tab). Defensive guard so PID-based
        // merging never conflates two unrelated browser tabs if anything ever
        // bypasses that gate.
        if matches!(source.kind, MediaSourceKind::Browser(_)) {
            return source;
        }

        if let Some(existing) = self.registry.find_by_pid(source.process.as_ref().map(|p| p.process_id).unwrap_or(0)) {
            // Check creation time to prevent PID reuse issues
            if let (Some(existing_proc), Some(new_proc)) = (&existing.process, &source.process) {
                if existing_proc.creation_time == new_proc.creation_time {
                    // It's the same process!
                    let mut merged = existing.clone();
                    if merged.source_type == SourceType::Smtc {
                        merged.source_type = SourceType::Hybrid;
                    }
                    // Update ID to the stable one if it's different (though it should be the same if logic is consistent)
                    merged.id = existing.id;
                    return merged;
                }
            }
        }

        source
    }

    pub fn reconcile_smtc_session(&self, mut source: MediaSource) -> MediaSource {
        source.source_type = SourceType::Smtc;

        // Browser SMTC sources are per-tab, but every tab shares the same
        // browser PID. PID-based merging would pick an arbitrary sibling tab's
        // source and conflate them. Each browser tab stays its own source.
        if matches!(source.kind, MediaSourceKind::Browser(_)) {
            return source;
        }

        if let Some(pid) = source.process.as_ref().map(|p| p.process_id) {
            if let Some(existing) = self.registry.find_by_pid(pid) {
                if let (Some(existing_proc), Some(new_proc)) = (&existing.process, &source.process) {
                    if existing_proc.creation_time == new_proc.creation_time {
                        let mut merged = existing.clone();
                        if merged.source_type == SourceType::NonSmtc {
                            merged.source_type = SourceType::Hybrid;
                        }
                        return merged;
                    }
                }
            }
        }

        source
    }
}
