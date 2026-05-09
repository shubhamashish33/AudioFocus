use windows::Win32::UI::WindowsAndMessaging::{
    LoadIconW, HICON, IDI_APPLICATION, IDI_ERROR, IDI_SHIELD,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayIconState {
    Active,
    Paused,
    Error,
}

pub fn load_state_icon(state: TrayIconState) -> HICON {
    let icon_id = match state {
        TrayIconState::Active => IDI_SHIELD,
        TrayIconState::Paused => IDI_APPLICATION,
        TrayIconState::Error => IDI_ERROR,
    };

    unsafe { LoadIconW(None, icon_id).unwrap_or_default() }
}
