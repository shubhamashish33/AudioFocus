use std::{
    thread,
    time::{Duration, Instant},
};

use crate::{error::Result, events::AudioSessionSnapshot, wasapi::WasapiSessionMonitor};

use super::dispatcher::NonSmtcTransportAction;

const ACTIVE_PEAK_THRESHOLD: f32 = 0.001;

pub struct WasapiPlaybackValidator {
    monitor: WasapiSessionMonitor,
}

impl WasapiPlaybackValidator {
    pub fn new() -> Result<Self> {
        Ok(Self {
            monitor: WasapiSessionMonitor::from_default_render_endpoint()?,
        })
    }

    pub fn wait_for_transport_state(
        &mut self,
        process_id: u32,
        action: NonSmtcTransportAction,
        timeout: Duration,
        observation_interval: Duration,
    ) -> Result<bool> {
        let expected_audible = expected_audibility(action);
        let started = Instant::now();
        while started.elapsed() < timeout {
            if self.is_process_audible(process_id)? == expected_audible {
                return Ok(true);
            }
            thread::sleep(observation_interval);
        }

        Ok(self.is_process_audible(process_id)? == expected_audible)
    }

    fn is_process_audible(&self, process_id: u32) -> Result<bool> {
        let snapshots = self.monitor.snapshot_sessions()?;
        Ok(snapshots
            .iter()
            .filter(|snapshot| snapshot.process_id == process_id)
            .any(active_audio_snapshot))
    }
}

fn expected_audibility(action: NonSmtcTransportAction) -> bool {
    matches!(action, NonSmtcTransportAction::Play)
}

fn active_audio_snapshot(snapshot: &AudioSessionSnapshot) -> bool {
    snapshot.is_active() && snapshot.peak > ACTIVE_PEAK_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_waits_for_silence() {
        assert!(!expected_audibility(NonSmtcTransportAction::Pause));
    }

    #[test]
    fn play_waits_for_audibility() {
        assert!(expected_audibility(NonSmtcTransportAction::Play));
    }
}
