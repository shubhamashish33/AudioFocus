use windows::Win32::UI::WindowsAndMessaging::{
    LoadIconW, HICON, IDI_APPLICATION, IDI_ERROR,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayIconState {
    Active,
    Paused,
    Error,
}

pub fn load_state_icon(state: TrayIconState) -> HICON {
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default() };
    
    match state {
        // IDI_APPLICATION (32512) is the default resource ID for the icon set in build.rs
        // We use MAKEINTRESOURCEW(1) essentially.
        TrayIconState::Active | TrayIconState::Paused => {
            unsafe { 
                LoadIconW(instance, windows::core::PCWSTR(1 as *const u16)).unwrap_or_else(|_| {
                    // Fallback to generic if resource loading fails
                    LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
                })
            }
        },
        TrayIconState::Error => {
            unsafe { LoadIconW(None, IDI_ERROR).unwrap_or_default() }
        }
    }
}

pub fn free_icon(_icon: HICON) {
    // Icons loaded with LoadIcon do not need to be freed if they are from resources
}
