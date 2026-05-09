use std::collections::HashMap;
use crate::identity::source_registry::SourceRegistry;
use crate::process::enumerate_processes;
use crate::media_source::MediaSourceId;

pub struct StaleSourceCollector {
    registry: std::sync::Arc<SourceRegistry>,
}

impl StaleSourceCollector {
    pub fn new(registry: std::sync::Arc<SourceRegistry>) -> Self {
        Self { registry }
    }

    pub fn collect(&self) -> Vec<MediaSourceId> {
        let processes = enumerate_processes();
        let active: HashMap<u32, u64> = processes.into_iter()
            .map(|p| (p.process_id, p.creation_time))
            .collect();
        
        let removed = self.registry.clear_stale(&active);
        
        for id in &removed {
            tracing::info!(source_id = %id, "cleaned up stale media source");
        }
        
        removed
    }
}
