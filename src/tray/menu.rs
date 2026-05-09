use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, HMENU, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED,
};

pub const IDM_TOGGLE_ACTIVE: u32 = 1001;
pub const IDM_TOGGLE_AUTO_RESUME: u32 = 1002;
pub const IDM_RESTART: u32 = 1003;
pub const IDM_OPEN_LOGS: u32 = 1004;
pub const IDM_QUIT: u32 = 1005;

pub fn create_tray_menu(active: bool, auto_resume: bool) -> HMENU {
    let menu = unsafe { CreatePopupMenu().unwrap() };

    let active_flags = if active {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    let resume_flags = if auto_resume {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    unsafe {
        let _ = AppendMenuW(menu, active_flags, IDM_TOGGLE_ACTIVE as usize, w!("Active"));
        let _ = AppendMenuW(menu, resume_flags, IDM_TOGGLE_AUTO_RESUME as usize, w!("Auto-Resume Recently Paused"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, IDM_RESTART as usize, w!("Restart AudioFocus"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN_LOGS as usize, w!("Open Logs Folder"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT as usize, w!("Quit"));
    }

    menu
}
