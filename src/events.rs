use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioSessionStateKind {
    Active,
    Inactive,
    Expired,
    Unknown(i32),
}

impl AudioSessionStateKind {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_expired(&self) -> bool {
        matches!(self, Self::Expired)
    }
}

impl fmt::Display for AudioSessionStateKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => formatter.write_str("active"),
            Self::Inactive => formatter.write_str("inactive"),
            Self::Expired => formatter.write_str("expired"),
            Self::Unknown(value) => write!(formatter, "unknown({value})"),
        }
    }
}

pub const AUDIO_ACTIVITY_THRESHOLD: f32 = 0.001;

#[derive(Clone, Debug)]
pub struct AudioSessionSnapshot {
    pub process_id: u32,
    pub display_name: String,
    pub state: AudioSessionStateKind,
    pub peak: f32,
    pub session_count: usize,
}

impl AudioSessionSnapshot {
    pub fn is_live(&self) -> bool {
        !self.state.is_expired()
    }

    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    pub fn is_audible(&self) -> bool {
        self.peak > AUDIO_ACTIVITY_THRESHOLD
    }
}

#[derive(Clone, Debug)]
pub enum AudioSessionEvent {
    SessionStarted(AudioSessionSnapshot),
    SessionStopped(AudioSessionSnapshot),
    SessionBecameActive(AudioSessionSnapshot),
    SessionBecameInactive(AudioSessionSnapshot),
}

impl AudioSessionEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionStarted(_) => "SessionStarted",
            Self::SessionStopped(_) => "SessionStopped",
            Self::SessionBecameActive(_) => "SessionBecameActive",
            Self::SessionBecameInactive(_) => "SessionBecameInactive",
        }
    }

    pub fn snapshot(&self) -> &AudioSessionSnapshot {
        match self {
            Self::SessionStarted(snapshot)
            | Self::SessionStopped(snapshot)
            | Self::SessionBecameActive(snapshot)
            | Self::SessionBecameInactive(snapshot) => snapshot,
        }
    }
}
