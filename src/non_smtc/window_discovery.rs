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

    unsafe {
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(&mut context as *mut _ as isize));
    }

    context.candidates
}

struct EnumContext {
    process_id: u32,
    candidates: Vec<WindowCandidate>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut EnumContext);

    let mut process_id = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    if process_id == context.process_id {
        let handle = WindowHandle(hwnd.0 as isize);
        let title = get_window_title(hwnd);
        let class_name = get_window_class(hwnd);
        let rect = get_window_rect(hwnd);
        let z_order = context.candidates.len();

        context.candidates.push(WindowCandidate {
            handle,
            title,
            class_name,
            rect,
            z_order,
        });
    }

    BOOL(1)
}

fn get_window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
    } else {
        String::new()
    }
}

fn get_window_class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
    } else {
        String::new()
    }
}

fn get_window_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok().map(|_| rect)
}
