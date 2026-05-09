use std::panic;
use std::thread;

pub fn spawn_safe<F, T>(name: String, f: F) -> thread::JoinHandle<crate::error::Result<T>>
where
    F: FnOnce() -> crate::error::Result<T> + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name.clone())
        .spawn(move || {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(f));
            match result {
                Ok(inner_res) => inner_res,
                Err(panic_payload) => {
                    let message = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };
                    
                    tracing::error!(worker = %name, panic = %message, "Panic detected in worker thread");
                    Err(crate::error::AudioFocusError::Thread(format!("Thread '{}' panicked: {}", name, message)))
                }
            }
        })
        .expect("Failed to spawn safe thread")
}
