use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum BrowserFamily {
    Chrome,
    Edge,
    Brave,
    Firefox,
}

impl fmt::Display for BrowserFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chrome => formatter.write_str("chrome"),
            Self::Edge => formatter.write_str("edge"),
            Self::Brave => formatter.write_str("brave"),
            Self::Firefox => formatter.write_str("firefox"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MediaSourceKind {
    DesktopApp,
    Browser(BrowserFamily),
    StoreApp,
    Unknown,
}

impl fmt::Display for MediaSourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DesktopApp => formatter.write_str("desktop_app"),
            Self::Browser(family) => write!(formatter, "browser:{family}"),
            Self::StoreApp => formatter.write_str("store_app"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MediaSourceId(String);

impl MediaSourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MediaSourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessIdentity {
    pub process_id: u32,
    pub executable_path: Option<PathBuf>,
    pub executable_name: String,
    pub package_full_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSource {
    pub id: MediaSourceId,
    pub kind: MediaSourceKind,
    pub source_app_user_model_id: String,
    pub process: Option<ProcessIdentity>,
}

impl MediaSource {
    pub fn unresolved(source_app_user_model_id: String) -> Self {
        let normalized = normalize_component(&source_app_user_model_id);
        Self {
            id: MediaSourceId::new(format!("smtc:unresolved:{normalized}")),
            kind: MediaSourceKind::Unknown,
            source_app_user_model_id,
            process: None,
        }
    }
}

pub fn normalize_component(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
