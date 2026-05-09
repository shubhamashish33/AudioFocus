use std::sync::Arc;
use crate::media_source::{MediaSource, MediaSourceId, MediaSourceKind};
use crate::events::AudioSessionSnapshot;
use super::*;

#[derive(Debug)]
pub struct IdentitySystem {
    registry: Arc<SourceRegistry>,
    manager: IdentityManager,
    reconciler: SessionReconciler,
    inspector: ProcessInspector,
    classifier: SourceClassifier,
    collector: StaleSourceCollector,
}

impl Default for IdentitySystem {
    fn default() -> Self {
        Self::new()
    }
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

        // Phase 2 of tab identity: WASAPI cannot distinguish browser tabs (all
        // tabs share the audio-service process), and WM_APPCOMMAND on the
        // browser HWND can't target a specific tab anyway. Drop the event so
        // arbitration only sees per-tab SMTC sources for browser audio.
        if crate::process::browser_family_for_exe(&process.executable_name).is_some() {
            return None;
        }

        let kind = if process.package_full_name.is_some() {
            MediaSourceKind::StoreApp
        } else {
            MediaSourceKind::DesktopApp
        };

        let aumid = process.package_full_name.clone().unwrap_or_else(|| process.executable_name.clone());

        let mut source = MediaSource {
            id: self.manager.generate_id(&process, &kind, &aumid, None),
            capability: self.classifier.classify(&process.executable_name, &kind),
            kind,
            source_type: crate::media_source::SourceType::NonSmtc,
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

    pub fn resolve_smtc_source(
        &self,
        mut source: MediaSource,
        tab_key: Option<&str>,
    ) -> Option<MediaSource> {
        if let Some(process) = &source.process {
             if self.classifier.should_ignore(&process.executable_name) {
                 return None;
             }
             source.capability = self.classifier.classify(&process.executable_name, &source.kind);
             source.id = self.manager.generate_id(
                 process,
                 &source.kind,
                 &source.source_app_user_model_id,
                 tab_key,
             );
        } else if let Some(key) = tab_key {
            // No process resolved, but SMTC still gave us a per-tab key — keep
            // unresolved tabs distinct so they don't collide in the registry.
            let base = source.id.as_str().to_string();
            source.id = MediaSourceId::new(format!("{}:tab:{}", base, key));
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
