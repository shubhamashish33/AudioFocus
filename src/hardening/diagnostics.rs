use std::sync::Arc;
use crate::identity::SourceRegistry;

pub struct DiagnosticsCollector {
    registry: Arc<SourceRegistry>,
}

impl DiagnosticsCollector {
    pub fn new(registry: Arc<SourceRegistry>) -> Self {
        Self { registry }
    }

    pub fn collect_snapshot(&self) -> String {
        let sources = self.registry.list();
        let mut report = String::new();
        report.push_str("--- AudioFocus Runtime Diagnostics ---\n");
        report.push_str(&format!("Tracked Sources: {}\n", sources.len()));
        
        for source in sources {
            report.push_str(&format!("- [{}] {} (Type: {}, Capability: {})\n", 
                source.id, 
                source.source_app_user_model_id,
                source.source_type,
                source.capability
            ));
        }
        
        report
    }
}
