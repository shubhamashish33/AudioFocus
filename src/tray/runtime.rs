use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::app::{AudioFocusMonitor, AudioFocusRuntime};
use crate::shutdown::ShutdownSignal;
use crate::tray::icons::TrayIconState;

pub struct RuntimeHost {
    monitor: AudioFocusMonitor,
    runtime: Mutex<Option<AudioFocusRuntime>>,
    shutdown: Mutex<Option<ShutdownSignal>>,
    error: Mutex<Option<String>>,
}

impl RuntimeHost {
    pub fn new() -> Self {
        Self {
            monitor: AudioFocusMonitor::new(Duration::from_millis(250)),
            runtime: Mutex::new(None),
            shutdown: Mutex::new(None),
            error: Mutex::new(None),
        }
    }

    pub fn start(&self) -> crate::error::Result<()> {
        let shutdown = ShutdownSignal::new();
        let runtime = self.monitor.start(shutdown.clone())?;

        let mut rt_lock = self.runtime.lock().unwrap();
        let mut sd_lock = self.shutdown.lock().unwrap();
        
        *rt_lock = Some(runtime);
        *sd_lock = Some(shutdown);
        
        tracing::info!("RuntimeHost started AudioFocus service");
        Ok(())
    }

    pub fn restart(&self) -> crate::error::Result<()> {
        tracing::info!("RuntimeHost restarting AudioFocus service");
        self.stop();
        self.start()
    }

    pub fn stop(&self) {
        if let Some(shutdown) = self.shutdown.lock().unwrap().take() {
            shutdown.request_shutdown();
        }
        if let Some(runtime) = self.runtime.lock().unwrap().take() {
            let _ = runtime.shutdown();
        }
        tracing::info!("RuntimeHost stopped AudioFocus service");
    }

    pub fn is_active(&self) -> bool {
        if let Some(runtime) = self.runtime.lock().unwrap().as_ref() {
            runtime.arbitration().is_enabled()
        } else {
            false
        }
    }

    pub fn toggle_active(&self) {
        if let Some(runtime) = self.runtime.lock().unwrap().as_ref() {
            let next = !runtime.arbitration().is_enabled();
            runtime.arbitration().set_enabled(next);
            tracing::info!(enabled = next, "Arbitration toggled via tray");
        }
    }

    pub fn state(&self) -> TrayIconState {
        if self.error.lock().unwrap().is_some() {
            TrayIconState::Error
        } else if self.is_active() {
            TrayIconState::Active
        } else {
            TrayIconState::Paused
        }
    }

    pub fn open_logs_folder(&self) {
        // Open the logs folder in explorer
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("logs");
        
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }

        let _ = std::process::Command::new("explorer")
            .arg(path)
            .spawn();
    }
}
