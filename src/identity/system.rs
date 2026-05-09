use std::sync::Arc;
use crate::media_source::{MediaSource, MediaSourceId, MediaSourceKind, ProcessIdentity};
use crate::events::AudioSessionSnapshot;
use super::*;

pub struct IdentitySystem {
    registry: Arc<SourceRegistry>,
    manager: IdentityManager,
    reconciler: SessionReconciler,
    inspector: ProcessInspector,
    classifier: SourceClassifier,
    collector: StaleSourceCollector,
}

impl IdentitySystem {
    pub fn new() -> Self {
        let registry = Arc::new(SourceRegistry::new());
        Self {
            registry: Arc::clone(&registry),
            manager: IdentityManager::new(),
            reconciler: SessionReconciler::new(Arc::clone(&registry)),
            inspector: ProcessInspector::new(),
            classifier: SourceClassifier::new(),
            collector: StaleSourceCollector::new(Arc::clone(&registry)),
        }
    }

    pub fn resolve_wasapi_session(&self, snapshot: &AudioSessionSnapshot) -> Option<MediaSource> {
        let process = self.inspector.inspect_process(snapshot.process_id);
        
        if self.classifier.should_ignore(&process.executable_name) {
            return None;
        }

        // Use the same browser detection logic as ProcessResolver
        let browser_family = crate::process::browser_family_for_exe(&process.executable_name);
        let kind = match browser_family {
            Some(family) => MediaSourceKind::Browser(family),
            None if process.package_full_name.is_some() => MediaSourceKind::StoreApp,
            None => MediaSourceKind::DesktopApp,
        };

        let aumid = process.package_full_name.clone().unwrap_or_else(|| process.executable_name.clone());
        
        let mut source = MediaSource {
            id: self.manager.generate_id(&process, &kind, &aumid),
            kind,
            source_type: crate::media_source::SourceType::NonSmtc,
            capability: self.classifier.classify(&process.executable_name, &kind),
            source_app_user_model_id: aumid,
            process: Some(process),
        };

        source = self.reconciler.reconcile_wasapi_session(source);
        self.registry.upsert(source.clone());
        
        tracing::debug!(
            source_id = %source.id,
            pid = snapshot.process_id,
            "resolved WASAPI audio session to media source"
        );
        
        Some(source)
    }

    pub fn resolve_smtc_source(&self, mut source: MediaSource) -> Option<MediaSource> {
        if let Some(process) = &source.process {
             if self.classifier.should_ignore(&process.executable_name) {
                 return None;
             }
             source.capability = self.classifier.classify(&process.executable_name, &source.kind);
             source.id = self.manager.generate_id(process, &source.kind, &source.source_app_user_model_id);
        }

        source = self.reconciler.reconcile_smtc_session(source);
        self.registry.upsert(source.clone());
        
        tracing::debug!(
            source_id = %source.id,
            "resolved SMTC session to media source"
        );
        
        Some(source)
    }

    pub fn cleanup_stale(&self) -> Vec<MediaSourceId> {
        self.collector.collect()
    }

    pub fn registry(&self) -> Arc<SourceRegistry> {
        Arc::clone(&self.registry)
    }
}
