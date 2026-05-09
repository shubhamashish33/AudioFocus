#![windows_subsystem = "windows"]

use std::sync::Arc;
use audiofocus::error::Result;
use audiofocus::logging;
use audiofocus::tray::{RuntimeHost, SingleInstance, TrayManager};

fn main() -> Result<()> {
    let _logging = logging::init()?;
    
    // Single instance protection
    let _instance = match SingleInstance::new("AudioFocus_Global_Mutex") {
        Some(instance) => instance,
        None => {
            tracing::warn!("Another instance of AudioFocus is already running. Trying to focus it.");
            unsafe {
                let window_class = windows::core::w!("AudioFocusTrayWindow");
                if let Ok(hwnd) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(window_class, None) {
                    if hwnd.0 != std::ptr::null_mut() {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                    }
                }
            }
            return Ok(());
        }
    };

    tracing::info!(
        app = "AudioFocus",
        phase = 7,
        "starting AudioFocus Hardened Runtime"
    );

    let runtime = Arc::new(RuntimeHost::new());
    if let Err(error) = runtime.start() {
        tracing::error!(%error, "Failed to start AudioFocus runtime");
    }

    let tray_manager = TrayManager::new(Arc::clone(&runtime));
    let result = tray_manager.run();

    runtime.stop();

    match &result {
        Ok(()) => tracing::info!("AudioFocus application stopped cleanly"),
        Err(error) => tracing::error!(%error, "AudioFocus application stopped with an error"),
    }

    result
}
