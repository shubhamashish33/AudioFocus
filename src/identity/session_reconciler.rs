use crate::media_source::{MediaSource, SourceType};
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
