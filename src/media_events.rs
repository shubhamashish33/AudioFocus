use std::fmt;

use crate::media_source::MediaSource;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Unknown,
}

impl fmt::Display for PlaybackState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Playing => formatter.write_str("playing"),
            Self::Paused => formatter.write_str("paused"),
            Self::Stopped => formatter.write_str("stopped"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct MediaMetadata {
    pub title: String,
    pub artist: String,
    pub album_title: String,
    pub album_artist: String,
    pub subtitle: String,
    pub track_number: i32,
}

impl MediaMetadata {
    pub fn fingerprint(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.title,
            self.artist,
            self.album_title,
            self.album_artist,
            self.subtitle,
            self.track_number
        )
    }
}

#[derive(Clone, Debug)]
pub enum MediaEvent {
    MediaStarted {
        source: MediaSource,
        metadata: MediaMetadata,
    },
    MediaPaused {
        source: MediaSource,
        metadata: MediaMetadata,
    },
    MediaStopped {
        source: MediaSource,
        metadata: MediaMetadata,
    },
    MediaMetadataChanged {
        source: MediaSource,
        metadata: MediaMetadata,
    },
    ActiveSessionChanged {
        source: Option<MediaSource>,
    },
}

impl MediaEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MediaStarted { .. } => "MediaStarted",
            Self::MediaPaused { .. } => "MediaPaused",
            Self::MediaStopped { .. } => "MediaStopped",
            Self::MediaMetadataChanged { .. } => "MediaMetadataChanged",
            Self::ActiveSessionChanged { .. } => "ActiveSessionChanged",
        }
    }

    pub fn source(&self) -> Option<&MediaSource> {
        match self {
            Self::MediaStarted { source, .. }
            | Self::MediaPaused { source, .. }
            | Self::MediaStopped { source, .. }
            | Self::MediaMetadataChanged { source, .. } => Some(source),
            Self::ActiveSessionChanged { source } => source.as_ref(),
        }
    }
}
