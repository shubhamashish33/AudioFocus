use std::collections::HashMap;
use std::sync::RwLock;
use crate::media_source::{MediaSource, MediaSourceId};

#[derive(Debug)]
pub struct SourceRegistry {
    sources: RwLock<HashMap<MediaSourceId, MediaSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
        }
    }

    pub fn upsert(&self, source: MediaSource) {
        if let Ok(mut sources) = self.sources.write() {
            sources.insert(source.id.clone(), source);
        }
    }

    pub fn get(&self, id: &MediaSourceId) -> Option<MediaSource> {
        self.sources.read().ok()?.get(id).cloned()
    }

    pub fn remove(&self, id: &MediaSourceId) -> Option<MediaSource> {
        self.sources.write().ok()?.remove(id)
    }

    pub fn find_by_pid(&self, pid: u32) -> Option<MediaSource> {
        let sources = self.sources.read().ok()?;
        sources.values().find(|source| {
            source.process.as_ref().map_or(false, |p| p.process_id == pid)
        }).cloned()
    }

    pub fn list(&self) -> Vec<MediaSource> {
        self.sources.read().map(|s| s.values().cloned().collect()).unwrap_or_default()
    }

    pub fn clear_stale(&self, active_processes: &HashMap<u32, u64>) -> Vec<MediaSourceId> {
        let mut removed = Vec::new();
        if let Ok(mut sources) = self.sources.write() {
            let stale_ids: Vec<MediaSourceId> = sources.iter()
                .filter(|(_, source)| {
                    if let Some(p) = &source.process {
                        match active_processes.get(&p.process_id) {
                            Some(&creation_time) => creation_time != p.creation_time,
                            None => true,
                        }
                    } else {
                        // Keep unresolved sources for a while? 
                        // Actually, if it has no process, it might be an unresolved SMTC source.
                        // We should probably have a separate TTL for those.
                        false 
                    }
                })
                .map(|(id, _)| id.clone())
                .collect();
            
            for id in stale_ids {
                sources.remove(&id);
                removed.push(id);
            }
        }
        removed
    }
}
