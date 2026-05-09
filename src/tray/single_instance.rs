use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, MUTEX_ALL_ACCESS};

pub struct SingleInstance {
    handle: Option<HANDLE>,
}

impl SingleInstance {
    pub fn new(name: &str) -> Option<Self> {
        let name_u16: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let pcwstr = PCWSTR(name_u16.as_ptr());

        // Try to open existing mutex
        let existing = unsafe { OpenMutexW(MUTEX_ALL_ACCESS, false, pcwstr) };
        if existing.is_ok() {
            let _ = unsafe { CloseHandle(existing.unwrap()) };
            return None;
        }

        // Create new mutex
        let handle = unsafe { CreateMutexW(None, true, pcwstr) };
        match handle {
            Ok(h) => Some(Self { handle: Some(h) }),
            Err(_) => None,
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = unsafe { CloseHandle(h) };
        }
    }
}
