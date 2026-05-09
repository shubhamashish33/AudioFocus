use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

pub struct EventStormProtector {
    window: Duration,
    max_events: usize,
    history: Mutex<VecDeque<Instant>>,
}

impl EventStormProtector {
    pub fn new(window: Duration, max_events: usize) -> Self {
        Self {
            window,
            max_events,
            history: Mutex::new(VecDeque::new()),
        }
    }

    pub fn check_and_record(&self) -> bool {
        let mut history = self.history.lock().unwrap();
        let now = Instant::now();
        
        while history.front().is_some_and(|&t| now.duration_since(t) > self.window) {
            history.pop_front();
        }
        
        if history.len() >= self.max_events {
            tracing::warn!(
                event_count = history.len(),
                window_ms = self.window.as_millis(),
                "Event storm detected; suppressing additional events"
            );
            return false;
        }
        
        history.push_back(now);
        true
    }
}
