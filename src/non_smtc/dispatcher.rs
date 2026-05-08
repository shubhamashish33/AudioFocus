use std::time::Duration;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::SystemServices::APPCOMMAND_MEDIA_PAUSE,
    UI::WindowsAndMessaging::{
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_ERRORONEXIT, WM_APPCOMMAND,
    },
};

use super::{window_discovery::WindowHandle, window_filtering::validate_window_for_process};

#[derive(Clone, Debug)]
pub struct DispatchResult {
    pub attempted: bool,
}

pub fn send_media_pause(
    handle: WindowHandle,
    process_id: u32,
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
    let command_lparam = LPARAM((APPCOMMAND_MEDIA_PAUSE.0 as isize) << 16);
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
        attempted,
        send_result = result,
        "sent WM_APPCOMMAND APPCOMMAND_MEDIA_PAUSE"
    );

    DispatchResult { attempted }
}
