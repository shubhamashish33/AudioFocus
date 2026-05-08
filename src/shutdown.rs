use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::error::Result;

#[derive(Clone, Debug)]
pub struct ShutdownSignal {
    requested: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn install_ctrlc_handler(&self) -> Result<()> {
        let requested = Arc::clone(&self.requested);
        ctrlc::set_handler(move || {
            requested.store(true, Ordering::SeqCst);
        })?;
        Ok(())
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}
