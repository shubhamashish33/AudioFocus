use crate::media_source::{normalize_component, MediaSourceId, MediaSourceKind, ProcessIdentity};

#[derive(Debug)]
pub struct IdentityManager {
}

impl Default for IdentityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityManager {
    pub fn new() -> Self {
        Self {
        }
    }

    /// Build a stable `MediaSourceId` for a media source.
    ///
    /// `tab_key`, when present, only takes effect for `MediaSourceKind::Browser`
    /// — it lets SMTC give each browser tab its own source. Non-browser kinds
    /// ignore it.
    pub fn generate_id(
        &self,
        process: &ProcessIdentity,
        kind: &MediaSourceKind,
        aumid: &str,
        tab_key: Option<&str>,
    ) -> MediaSourceId {
        match kind {
            MediaSourceKind::Browser(family) => match tab_key {
                Some(key) => MediaSourceId::new(format!("browser:{}:tab:{}", family, key)),
                None => MediaSourceId::new(format!("browser:{}", family)),
            },
            MediaSourceKind::StoreApp => {
                let name = process.package_full_name.as_deref().unwrap_or(aumid);
                MediaSourceId::new(format!("store:{}", normalize_component(name)))
            }
            _ => {
                // Desktop apps: use executable path for stability across PID reuse.
                let path_or_name = process.executable_path.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| process.executable_name.clone());

                MediaSourceId::new(format!("process:{}", normalize_component(&path_or_name)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_source::BrowserFamily;
    use std::path::PathBuf;

    fn chrome_process() -> ProcessIdentity {
        ProcessIdentity {
            process_id: 1234,
            creation_time: 0,
            executable_path: Some(PathBuf::from("C:/Program Files/Google/Chrome/chrome.exe")),
            executable_name: "chrome.exe".to_string(),
            package_full_name: None,
        }
    }

    #[test]
    fn browser_with_tab_key_includes_tab() {
        let mgr = IdentityManager::new();
        let id = mgr.generate_id(
            &chrome_process(),
            &MediaSourceKind::Browser(BrowserFamily::Chrome),
            "youtube",
            Some("abc"),
        );
        assert_eq!(id.as_str(), "browser:chrome:tab:abc");
    }

    #[test]
    fn browser_without_tab_key_collapses_to_one_id_per_family() {
        let mgr = IdentityManager::new();
        let id = mgr.generate_id(
            &chrome_process(),
            &MediaSourceKind::Browser(BrowserFamily::Chrome),
            "youtube",
            None,
        );
        assert_eq!(id.as_str(), "browser:chrome");
    }

    #[test]
    fn distinct_tab_keys_produce_distinct_ids() {
        let mgr = IdentityManager::new();
        let id_a = mgr.generate_id(
            &chrome_process(),
            &MediaSourceKind::Browser(BrowserFamily::Chrome),
            "x",
            Some("aaa"),
        );
        let id_b = mgr.generate_id(
            &chrome_process(),
            &MediaSourceKind::Browser(BrowserFamily::Chrome),
            "x",
            Some("bbb"),
        );
        assert_ne!(id_a.as_str(), id_b.as_str());
    }

    #[test]
    fn non_browser_kinds_ignore_tab_key() {
        let mgr = IdentityManager::new();
        let mut process = chrome_process();
        process.executable_name = "spotify.exe".to_string();
        process.executable_path = Some(PathBuf::from("C:/Spotify/Spotify.exe"));
        let id_a = mgr.generate_id(&process, &MediaSourceKind::DesktopApp, "Spotify", Some("aaa"));
        let id_b = mgr.generate_id(&process, &MediaSourceKind::DesktopApp, "Spotify", Some("bbb"));
        assert_eq!(id_a.as_str(), id_b.as_str());
        assert!(id_a.as_str().starts_with("process:"));
    }
}
