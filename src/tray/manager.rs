use std::sync::Arc;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NIF_ICON,
    NIF_INFO, NIF_MESSAGE, NIF_TIP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, TrackPopupMenu, CS_HREDRAW, CS_VREDRAW,
    GWLP_USERDATA, HMENU, HWND_MESSAGE, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WINDOW_EX_STYLE,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_RBUTTONUP, WM_TIMER, WM_USER,
    WNDCLASSW, WS_OVERLAPPED,
};

use crate::tray::icons::{free_icon, load_state_icon};
use crate::tray::menu::{
    create_tray_menu, IDM_OPEN_LOGS, IDM_QUIT, IDM_RESTART, IDM_TOGGLE_ACTIVE, IDM_TOGGLE_AUTO_RESUME,
};
use crate::tray::runtime::RuntimeHost;

const WM_TRAY_ICON: u32 = WM_USER + 1;
const ID_TIMER_MAINTENANCE: usize = 1;

pub struct TrayManager {
    runtime: Arc<RuntimeHost>,
}

impl TrayManager {
    pub fn new(runtime: Arc<RuntimeHost>) -> Self {
        Self { runtime }
    }

    pub fn run(&self) -> crate::error::Result<()> {
        let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? };
        let window_class = w!("AudioFocusTrayWindow");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };

        unsafe { RegisterClassW(&wc) };

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                window_class,
                w!("AudioFocus"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                HMENU::default(),
                instance,
                Some(self as *const _ as *const _),
            )
        }?;

        if hwnd.0.is_null() {
            return Err(crate::error::AudioFocusError::Win32(
                "Failed to create tray window".to_string(),
            ));
        }

        unsafe { SetTimer(hwnd, ID_TIMER_MAINTENANCE, 5000, None) };
        self.add_tray_icon(hwnd);
        
        // Startup Notification
        self.show_notification(hwnd, "AudioFocus Started", "Background audio orchestration is active.");

        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
        }

        let _ = unsafe { KillTimer(hwnd, ID_TIMER_MAINTENANCE) };
        self.remove_tray_icon(hwnd);
        Ok(())
    }

    pub fn show_notification(&self, hwnd: HWND, title: &str, message: &str) {
        let mut nid = self.create_nid(hwnd);
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        
        copy_u16_slice(&mut nid.szInfoTitle, title);
        copy_u16_slice(&mut nid.szInfo, message);

        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn add_tray_icon(&self, hwnd: HWND) {
        let mut nid = self.create_nid(hwnd);
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        
        let icon = load_state_icon(self.runtime.state());
        nid.hIcon = icon;
        
        copy_u16_slice(&mut nid.szTip, "AudioFocus");

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
        
        free_icon(icon);
    }

    fn update_tray_icon(&self, hwnd: HWND) {
        let mut nid = self.create_nid(hwnd);
        nid.uFlags = NIF_ICON;
        
        let icon = load_state_icon(self.runtime.state());
        nid.hIcon = icon;
        
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
        
        free_icon(icon);
    }

    fn remove_tray_icon(&self, hwnd: HWND) {
        let nid = self.create_nid(hwnd);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    fn create_nid(&self, hwnd: HWND) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == ID_TIMER_MAINTENANCE {
                if let Some(tm) = get_tray_manager(hwnd) {
                    tm.runtime.run_maintenance();
                    tm.update_tray_icon(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            let tray_manager = get_tray_manager(hwnd);
            match lparam.0 as u32 {
                WM_RBUTTONUP => {
                    if let Some(tm) = tray_manager {
                        tm.show_menu(hwnd);
                    }
                    LRESULT(0)
                }
                WM_LBUTTONDBLCLK => {
                    if let Some(tm) = tray_manager {
                        tm.runtime.toggle_active();
                        tm.update_tray_icon(hwnd);
                        let status = if tm.runtime.is_active() { "Enabled" } else { "Disabled" };
                        tm.show_notification(hwnd, "AudioFocus", &format!("Arbitration is now {}.", status));
                    }
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_COMMAND => {
            let tray_manager = get_tray_manager(hwnd);
            if let Some(tm) = tray_manager {
                match wparam.0 as u32 {
                    IDM_TOGGLE_ACTIVE => {
                        tm.runtime.toggle_active();
                        tm.update_tray_icon(hwnd);
                        let status = if tm.runtime.is_active() { "Enabled" } else { "Disabled" };
                        tm.show_notification(hwnd, "AudioFocus", &format!("Arbitration is now {}.", status));
                    }
                    IDM_TOGGLE_AUTO_RESUME => {
                        tm.runtime.toggle_auto_resume();
                        let status = if tm.runtime.is_auto_resume() { "Enabled" } else { "Disabled" };
                        tm.show_notification(hwnd, "AudioFocus", &format!("Auto-Resume is now {}.", status));
                    }
                    IDM_RESTART => {
                        tm.show_notification(hwnd, "AudioFocus", "Restarting services...");
                        let _ = tm.runtime.restart();
                        tm.update_tray_icon(hwnd);
                    }
                    IDM_OPEN_LOGS => {
                        tm.runtime.open_logs_folder();
                    }
                    IDM_QUIT => {
                        tm.show_notification(hwnd, "AudioFocus", "Shutting down...");
                        let _ = PostMessageW(hwnd, WM_DESTROY, WPARAM(0), LPARAM(0));
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        _ => {
            if msg == WM_CREATE {
                let create_struct = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
                let tm = (*create_struct).lpCreateParams as isize;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, tm);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

fn get_tray_manager(hwnd: HWND) -> Option<&'static TrayManager> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if ptr == 0 {
        None
    } else {
        unsafe { Some(&*(ptr as *const TrayManager)) }
    }
}

impl TrayManager {
    fn show_menu(&self, hwnd: HWND) {
        let mut pos = Default::default();
        unsafe { let _ = GetCursorPos(&mut pos); };

        let menu = create_tray_menu(self.runtime.is_active(), self.runtime.is_auto_resume());
        unsafe {
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_BOTTOMALIGN | TPM_LEFTALIGN,
                pos.x,
                pos.y,
                0,
                hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
        }
    }
}

fn copy_u16_slice(dest: &mut [u16], src: &str) {
    let src_u16: Vec<u16> = src.encode_utf16().collect();
    let len = src_u16.len().min(dest.len() - 1);
    dest[..len].copy_from_slice(&src_u16[..len]);
    dest[len] = 0;
}
