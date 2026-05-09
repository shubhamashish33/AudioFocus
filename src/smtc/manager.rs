use std::{sync::{mpsc, Arc}, thread};

use crate::{error::AudioFocusError, identity::IdentitySystem, shutdown::ShutdownSignal};

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
    pub fn start(shutdown: ShutdownSignal, identity_system: Arc<IdentitySystem>) -> crate::error::Result<Self> {
        let (sender, receiver) = mpsc::channel::<SmtcWorkerMessage>();
        let controller = SmtcTransportController::new(sender.clone());
        let worker = thread::Builder::new()
            .name("smtc-session-monitor".to_string())
            .spawn(move || run_smtc_worker(shutdown, sender, receiver, identity_system))
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
