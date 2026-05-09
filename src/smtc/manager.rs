use std::{
    sync::{mpsc, Arc},
    thread,
};

use crate::{
    arbitration::ArbitrationHandle, error::AudioFocusError, identity::IdentitySystem,
    shutdown::ShutdownSignal,
};

use super::{
    controller::SmtcTransportController,
    watcher::{run_smtc_worker, SmtcWorkerMessage},
};

#[derive(Debug)]
pub struct SmtcRuntime {
    controller: SmtcTransportController,
    worker: Option<thread::JoinHandle<crate::error::Result<()>>>,
}

impl SmtcRuntime {
    pub fn start(
        shutdown: ShutdownSignal,
        identity_system: Arc<IdentitySystem>,
        arbitration: ArbitrationHandle,
    ) -> crate::error::Result<Self> {
        let (sender, receiver) = mpsc::channel::<SmtcWorkerMessage>();
        Self::start_with_channel(shutdown, identity_system, arbitration, sender, receiver)
    }

    pub fn start_with_channel(
        shutdown: ShutdownSignal,
        identity_system: Arc<IdentitySystem>,
        arbitration: ArbitrationHandle,
        sender: mpsc::Sender<SmtcWorkerMessage>,
        receiver: mpsc::Receiver<SmtcWorkerMessage>,
    ) -> crate::error::Result<Self> {
        let controller = SmtcTransportController::new(sender);
        let worker = thread::Builder::new()
            .name("smtc-session-monitor".to_string())
            .spawn(move || {
                run_smtc_worker(shutdown, sender, receiver, identity_system, arbitration)
            })
            .map_err(|error| AudioFocusError::Thread(error.to_string()))?;

        Ok(Self {
            controller,
            worker: Some(worker),
        })
    }

    pub fn controller(&self) -> SmtcTransportController {
        self.controller.clone()
    }

    pub fn join(mut self) -> crate::error::Result<()> {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| AudioFocusError::Thread("SMTC worker panicked".to_string()))?
        } else {
            Ok(())
        }
    }
}
