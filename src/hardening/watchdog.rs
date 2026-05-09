use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;

pub struct Watchdog {
    heartbeats: Arc<Mutex<HashMap<String, Instant>>>,
    timeout: Duration,
}

impl Watchdog {
    pub fn new(timeout: Duration) -> Self {
        Self {
            heartbeats: Arc::new(Mutex::new(HashMap::new())),
            timeout,
        }
    }

    pub fn heartbeat(&self, worker_name: &str) {
        if let Ok(mut heartbeats) = self.heartbeats.lock() {
            heartbeats.insert(worker_name.to_string(), Instant::now());
        }
    }

    pub fn check_health(&self) -> Vec<String> {
        let mut failed = Vec::new();
        let now = Instant::now();
        
        if let Ok(heartbeats) = self.heartbeats.lock() {
            for (name, last_seen) in heartbeats.iter() {
                if now.duration_since(*last_seen) > self.timeout {
                    failed.push(name.clone());
                }
            }
        }
        
        for name in &failed {
            tracing::error!(worker = %name, "Watchdog detected stalled worker thread");
        }
        
        failed
    }
}
