#![windows_subsystem = "windows"]

mod arbitration;
mod app;
mod com;
mod error;
mod events;
mod identity;
mod logging;
mod media_events;
mod media_source;
mod non_smtc;
mod process;
mod registry;
mod shutdown;
mod smtc;
mod tray;
mod wasapi;

use std::sync::Arc;
use crate::error::Result;
use crate::tray::{RuntimeHost, SingleInstance, TrayManager};

fn main() -> Result<()> {
    let _logging = logging::init()?;
    
    // Single instance protection
    let _instance = match SingleInstance::new("AudioFocus_Global_Mutex") {
        Some(instance) => instance,
        None => {
            tracing::warn!("Another instance of AudioFocus is already running. Trying to focus it.");
            unsafe {
                let window_class = windows::core::w!("AudioFocusTrayWindow");
                let hwnd = windows::Win32::UI::WindowsAndMessaging::FindWindowW(window_class, None);
                if hwnd.0 != 0 {
                    windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
            }
            return Ok(());
        }
    };

    tracing::info!(
        app = "AudioFocus",
        phase = 6,
        "starting AudioFocus Tray Application"
    );

    let runtime = Arc::new(RuntimeHost::new());
    if let Err(error) = runtime.start() {
        tracing::error!(%error, "Failed to start AudioFocus runtime");
        // We still continue to show the tray with an error state if possible
    }

    let tray_manager = TrayManager::new(Arc::clone(&runtime));
    let result = tray_manager.run();

    runtime.stop();

    match &result {
        Ok(()) => tracing::info!("AudioFocus tray application stopped cleanly"),
        Err(error) => tracing::error!(%error, "AudioFocus tray application stopped with an error"),
    }

    result
}
