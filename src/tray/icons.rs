use windows::Win32::UI::WindowsAndMessaging::{
    LoadIconW, HICON, IDI_APPLICATION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayIconState {
    Active,
    Paused,
}

pub fn load_state_icon(_state: TrayIconState) -> HICON {
    let instance = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default()
    };

    // The bundled .ico is registered as resource id 1 by build.rs; fall back
    // to a system icon if resource loading fails. The integer-to-pointer
    // cast is the MAKEINTRESOURCEW idiom — Win32 interprets the low bits as
    // a resource id when the high bits are zero, so the "dangling pointer"
    // clippy flags is intentional and required by the API.
    #[allow(clippy::manual_dangling_ptr)]
    let resource = windows::core::PCWSTR(1 as *const u16);

    unsafe {
        LoadIconW(instance, resource)
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}

pub fn free_icon(_icon: HICON) {
    // LoadIconW returns shared icons that must not be destroyed.
}
