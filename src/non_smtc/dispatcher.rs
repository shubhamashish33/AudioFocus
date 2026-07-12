use std::time::Duration;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::SystemServices::{APPCOMMAND_MEDIA_PAUSE, APPCOMMAND_MEDIA_PLAY},
    UI::WindowsAndMessaging::{
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_ERRORONEXIT, WM_APPCOMMAND,
    },
};

use super::{window_discovery::WindowHandle, window_filtering::validate_window_for_process};

#[derive(Clone, Debug)]
pub struct DispatchResult {
    pub attempted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonSmtcTransportAction {
    Pause,
    Play,
}

impl NonSmtcTransportAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Play => "play",
        }
    }

    fn app_command(self) -> u32 {
        match self {
            Self::Pause => APPCOMMAND_MEDIA_PAUSE.0,
            Self::Play => APPCOMMAND_MEDIA_PLAY.0,
        }
    }
}

pub fn send_media_command(
    handle: WindowHandle,
    process_id: u32,
    action: NonSmtcTransportAction,
    timeout: Duration,
) -> DispatchResult {
    if !validate_window_for_process(handle, process_id) {
        tracing::warn!(
            hwnd = handle.0,
            process_id,
            "candidate HWND became invalid before WM_APPCOMMAND dispatch"
        );
        return DispatchResult { attempted: false };
    }

    let hwnd = handle.hwnd();
    let command_lparam = LPARAM((action.app_command() as isize) << 16);
    let wparam = WPARAM(hwnd.0 as usize);
    let mut result = 0usize;
    let response = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_APPCOMMAND,
            wparam,
            command_lparam,
            SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
            timeout.as_millis().min(u32::MAX as u128) as u32,
            Some(&mut result),
        )
    };

    let attempted = response.0 != 0;
    tracing::info!(
        hwnd = handle.0,
        process_id,
        action = action.name(),
        attempted,
        send_result = result,
        "sent WM_APPCOMMAND media transport command"
    );

    DispatchResult { attempted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_actions_use_explicit_idempotent_commands() {
        assert_eq!(
            NonSmtcTransportAction::Pause.app_command(),
            APPCOMMAND_MEDIA_PAUSE.0
        );
        assert_eq!(
            NonSmtcTransportAction::Play.app_command(),
            APPCOMMAND_MEDIA_PLAY.0
        );
    }
}
