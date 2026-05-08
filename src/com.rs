use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::error::Result;

#[derive(Debug)]
pub struct MtaApartment;

impl MtaApartment {
    pub fn initialize() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)?;
        }
        Ok(Self)
    }
}

impl Drop for MtaApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
