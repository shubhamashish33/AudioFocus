use windows::Win32::{
    Foundation::{BOOL, HWND, LPARAM, RECT},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowHandle(pub isize);

impl WindowHandle {
    pub fn hwnd(self) -> HWND {
        HWND(self.0 as *mut core::ffi::c_void)
    }
}

#[derive(Clone, Debug)]
pub struct WindowCandidate {
    pub handle: WindowHandle,
    pub process_id: u32,
    pub title: String,
    pub class_name: String,
    pub rect: Option<RECT>,
    pub z_order: usize,
}

pub fn enumerate_top_level_windows_for_process(process_id: u32) -> Vec<WindowCandidate> {
    let mut context = EnumContext {
        process_id,
        candidates: Vec::new(),
    };

    let lparam = LPARAM((&mut context as *mut EnumContext) as isize);
    if let Err(error) = unsafe { EnumWindows(Some(enum_windows_proc), lparam) } {
        tracing::warn!(%error, process_id, "failed to enumerate top-level windows");
    }

    context.candidates
}

struct EnumContext {
    process_id: u32,
    candidates: Vec<WindowCandidate>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut EnumContext);
    let mut window_process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut window_process_id));
    }

    if window_process_id == context.process_id {
        let z_order = context.candidates.len();
        context.candidates.push(WindowCandidate {
            handle: WindowHandle(hwnd.0 as isize),
            process_id: window_process_id,
            title: window_text(hwnd),
            class_name: class_name(hwnd),
            rect: window_rect(hwnd),
            z_order,
        });
    }

    BOOL(1)
}

fn window_text(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if length <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..length as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if length <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..length as usize])
}

fn window_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some(rect)
}
