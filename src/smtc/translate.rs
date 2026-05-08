use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionMediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as SmtcPlaybackStatus,
};

use crate::media_events::{MediaMetadata, PlaybackState};

pub fn playback_state_from_smtc(status: SmtcPlaybackStatus) -> PlaybackState {
    match status.0 {
        4 => PlaybackState::Playing,
        5 => PlaybackState::Paused,
        1 => PlaybackState::Stopped,
        _ => PlaybackState::Unknown,
    }
}

pub fn metadata_from_smtc(
    properties: &GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> MediaMetadata {
    MediaMetadata {
        title: hstring(properties.Title()),
        artist: hstring(properties.Artist()),
        album_title: hstring(properties.AlbumTitle()),
        album_artist: hstring(properties.AlbumArtist()),
        subtitle: hstring(properties.Subtitle()),
        track_number: properties.TrackNumber().unwrap_or_default(),
    }
}

fn hstring(value: windows::core::Result<windows::core::HSTRING>) -> String {
    value
        .map(|value| value.to_string_lossy())
        .unwrap_or_default()
}
