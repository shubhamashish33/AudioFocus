use crate::media_source::{MediaSourceId, MediaSourceKind, ProcessIdentity, normalize_component};
use crate::identity::source_classifier::SourceClassifier;

pub struct IdentityManager {
    classifier: SourceClassifier,
}

impl IdentityManager {
    pub fn new() -> Self {
        Self {
            classifier: SourceClassifier::new(),
        }
    }

    pub fn generate_id(&self, process: &ProcessIdentity, kind: &MediaSourceKind, aumid: &str) -> MediaSourceId {
        match kind {
            MediaSourceKind::Browser(family) => {
                // Requirement 6: treat browser as one logical source
                MediaSourceId::new(format!("browser:{}", family))
            }
            MediaSourceKind::StoreApp => {
                let name = process.package_full_name.as_deref().unwrap_or(aumid);
                MediaSourceId::new(format!("store:{}", normalize_component(name)))
            }
            _ => {
                // Requirement 8: Spotify handling. 
                // We use executable path for desktop apps to ensure stability.
                let path_or_name = process.executable_path.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| process.executable_name.clone());
                
                MediaSourceId::new(format!("process:{}", normalize_component(&path_or_name)))
            }
        }
    }
}
