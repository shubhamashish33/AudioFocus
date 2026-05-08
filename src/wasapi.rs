use std::collections::HashMap;

use windows::{
    core::PWSTR,
    Win32::{
        Media::Audio::{
            eConsole, eRender, AudioSessionState, AudioSessionStateActive,
            AudioSessionStateExpired, AudioSessionStateInactive, Endpoints::IAudioMeterInformation,
            IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL},
    },
};

use crate::{
    error::Result,
    events::{AudioSessionSnapshot, AudioSessionStateKind},
};

#[derive(Debug)]
pub struct WasapiSessionMonitor {
    session_manager: IAudioSessionManager2,
}

impl WasapiSessionMonitor {
    pub fn from_default_render_endpoint() -> Result<Self> {
        let device_enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device = unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
        let session_manager =
            unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)? };

        Ok(Self { session_manager })
    }

    pub fn snapshot_sessions(&self) -> Result<Vec<AudioSessionSnapshot>> {
        let enumerator = unsafe { self.session_manager.GetSessionEnumerator()? };
        let count = unsafe { enumerator.GetCount()? };
        let mut by_process = HashMap::<u32, ProcessSessionAccumulator>::new();

        for index in 0..count {
            let control = unsafe { enumerator.GetSession(index)? };
            if let Some(snapshot) = snapshot_session(&control)? {
                by_process
                    .entry(snapshot.process_id)
                    .and_modify(|existing| existing.merge(snapshot.clone()))
                    .or_insert_with(|| ProcessSessionAccumulator::from(snapshot));
            }
        }

        Ok(by_process
            .into_values()
            .map(ProcessSessionAccumulator::into_snapshot)
            .collect())
    }
}

#[derive(Clone, Debug)]
struct ProcessSessionAccumulator {
    process_id: u32,
    display_name: String,
    state: AudioSessionStateKind,
    peak: f32,
    session_count: usize,
}

impl ProcessSessionAccumulator {
    fn from(snapshot: AudioSessionSnapshot) -> Self {
        Self {
            process_id: snapshot.process_id,
            display_name: snapshot.display_name,
            state: snapshot.state,
            peak: snapshot.peak,
            session_count: 1,
        }
    }

    fn merge(&mut self, snapshot: AudioSessionSnapshot) {
        if self.display_name.is_empty() && !snapshot.display_name.is_empty() {
            self.display_name = snapshot.display_name;
        }

        self.state = strongest_state(&self.state, &snapshot.state);
        self.peak = self.peak.max(snapshot.peak);
        self.session_count += 1;
    }

    fn into_snapshot(self) -> AudioSessionSnapshot {
        AudioSessionSnapshot {
            process_id: self.process_id,
            display_name: self.display_name,
            state: self.state,
            peak: self.peak,
            session_count: self.session_count,
        }
    }
}

fn snapshot_session(control: &IAudioSessionControl) -> Result<Option<AudioSessionSnapshot>> {
    let control2 = control.cast::<IAudioSessionControl2>()?;
    let process_id = unsafe { control2.GetProcessId()? };
    if process_id == 0 {
        return Ok(None);
    }

    let display_name = read_display_name(control)?;
    let state = unsafe { control.GetState()? };
    let peak = read_peak(control).unwrap_or(0.0);

    Ok(Some(AudioSessionSnapshot {
        process_id,
        display_name,
        state: convert_state(state),
        peak,
        session_count: 1,
    }))
}

fn read_peak(control: &IAudioSessionControl) -> Result<f32> {
    let meter = control.cast::<IAudioMeterInformation>()?;
    Ok(unsafe { meter.GetPeakValue()? })
}

fn read_display_name(control: &IAudioSessionControl) -> Result<String> {
    let raw = unsafe { control.GetDisplayName()? };
    let display_name = pwstr_to_string(raw);
    unsafe {
        CoTaskMemFree(Some(raw.0.cast()));
    }
    Ok(display_name)
}

fn pwstr_to_string(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }

    unsafe { value.to_string().unwrap_or_default() }
}

fn convert_state(state: AudioSessionState) -> AudioSessionStateKind {
    if state == AudioSessionStateActive {
        AudioSessionStateKind::Active
    } else if state == AudioSessionStateInactive {
        AudioSessionStateKind::Inactive
    } else if state == AudioSessionStateExpired {
        AudioSessionStateKind::Expired
    } else {
        AudioSessionStateKind::Unknown(state.0)
    }
}

fn strongest_state(
    left: &AudioSessionStateKind,
    right: &AudioSessionStateKind,
) -> AudioSessionStateKind {
    match (left, right) {
        (AudioSessionStateKind::Active, _) | (_, AudioSessionStateKind::Active) => {
            AudioSessionStateKind::Active
        }
        (AudioSessionStateKind::Inactive, _) | (_, AudioSessionStateKind::Inactive) => {
            AudioSessionStateKind::Inactive
        }
        (AudioSessionStateKind::Expired, AudioSessionStateKind::Expired) => {
            AudioSessionStateKind::Expired
        }
        (AudioSessionStateKind::Unknown(value), _) => AudioSessionStateKind::Unknown(*value),
        (_, AudioSessionStateKind::Unknown(value)) => AudioSessionStateKind::Unknown(*value),
    }
}
