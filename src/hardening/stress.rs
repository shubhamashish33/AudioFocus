use crate::arbitration::{ArbitrationHandle, ArbitrationEvent};
use crate::media_source::MediaSource;

pub struct StressTestHarness {
    arbitration: ArbitrationHandle,
}

impl StressTestHarness {
    pub fn new(arbitration: ArbitrationHandle) -> Self {
        Self { arbitration }
    }

    pub fn simulate_event_flood(&self, count: usize) {
        tracing::info!(count, "Starting simulated event flood stress test");
        let dummy_source = MediaSource::unresolved("stress-test-dummy".to_string());
        
        for i in 0..count {
            let _ = self.arbitration.submit(ArbitrationEvent::Media(
                crate::media_events::MediaEvent::MediaMetadataChanged {
                    source: dummy_source.clone(),
                    metadata: crate::media_events::MediaMetadata {
                        title: format!("Stress Test Event {}", i),
                        ..Default::default()
                    }
                }
            ));
        }
        tracing::info!("Simulated event flood completed");
    }
}
