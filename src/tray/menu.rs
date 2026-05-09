use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, HMENU, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED,
};

pub const IDM_TOGGLE_ACTIVE: u32 = 1001;
pub const IDM_RESTART: u32 = 1002;
pub const IDM_OPEN_LOGS: u32 = 1003;
pub const IDM_QUIT: u32 = 1004;

pub fn create_tray_menu(active: bool) -> HMENU {
    let menu = unsafe { CreatePopupMenu().unwrap() };

    let toggle_flags = if active {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };

    unsafe {
        let _ = AppendMenuW(menu, toggle_flags, IDM_TOGGLE_ACTIVE as usize, w!("Active"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, IDM_RESTART as usize, w!("Restart AudioFocus"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN_LOGS as usize, w!("Open Logs Folder"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT as usize, w!("Quit"));
    }

    menu
}
