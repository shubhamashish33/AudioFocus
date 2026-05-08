mod app;
mod com;
mod error;
mod events;
mod logging;
mod media_events;
mod media_source;
mod process;
mod registry;
mod shutdown;
mod smtc;
mod wasapi;

use std::time::Duration;

use crate::app::AudioFocusMonitor;
use crate::error::Result;
use crate::shutdown::ShutdownSignal;

fn main() -> Result<()> {
    let _logging = logging::init()?;
    tracing::info!(
        app = "AudioFocus",
        phase = 2,
        "starting AudioFocus WASAPI and SMTC monitors"
    );

    let shutdown = ShutdownSignal::new();
    shutdown.install_ctrlc_handler()?;

    let monitor = AudioFocusMonitor::new(Duration::from_millis(250));
    let result = monitor.run(shutdown);

    match &result {
        Ok(()) => tracing::info!("AudioFocus monitor stopped cleanly"),
        Err(error) => tracing::error!(%error, "AudioFocus monitor stopped with an error"),
    }

    result
}
