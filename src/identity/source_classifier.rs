use crate::media_source::{MediaCapability, MediaSourceKind};

#[derive(Clone, Debug, Default)]
pub struct SourceClassifier;

impl SourceClassifier {
    pub fn new() -> Self {
        Self
    }

    pub fn classify(&self, executable_name: &str, kind: &MediaSourceKind) -> MediaCapability {
        if self.is_system_process(executable_name) || self.is_ignored_process(executable_name) {
            return MediaCapability::System;
        }

        match kind {
            MediaSourceKind::Browser(_) => MediaCapability::Browser,
            _ if self.is_streaming_app(executable_name) => MediaCapability::StreamingApp,
            _ if self.is_dedicated_player(executable_name) => MediaCapability::DedicatedPlayer,
            _ => MediaCapability::Unknown,
        }
    }

    pub fn should_ignore(&self, executable_name: &str) -> bool {
        self.is_system_process(executable_name) || self.is_ignored_process(executable_name)
    }

    fn is_streaming_app(&self, executable_name: &str) -> bool {
        let name = executable_name.to_ascii_lowercase();
        name.contains("spotify") || name.contains("netflix") || name.contains("deezer") || name.contains("tidal")
    }

    fn is_dedicated_player(&self, executable_name: &str) -> bool {
        let name = executable_name.to_ascii_lowercase();
        name.contains("vlc") || name.contains("foobar2000") || name.contains("wmplayer") || name.contains("music.ui")
    }

    fn is_system_process(&self, executable_name: &str) -> bool {
        let name = executable_name.to_ascii_lowercase();
        matches!(name.as_str(), "audiodg.exe" | "svchost.exe" | "system" | "idle" | "lsass.exe" | "csrss.exe" | "wininit.exe" | "services.exe")
    }

    fn is_ignored_process(&self, executable_name: &str) -> bool {
        let name = executable_name.to_ascii_lowercase();
        name.contains("update") || name.contains("helper") || name.contains("crashpad") || name.contains("telemetry") || name.contains("feedback")
    }
}
