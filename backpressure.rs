use std::time::Duration;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum IngestionStrategy {
    /// Full speed ingestion
    Normal,
    /// Slowed ingestion with artificial delays
    Throttled,
    /// Stopped ingestion until health improves
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackpressureStatus {
    pub strategy: IngestionStrategy,
    pub db_latency_ms: u64,
    pub queue_depth: usize,
}

pub struct BackpressureController {
    degraded_latency_ms: u64,
    critical_latency_ms: u64,
    degraded_queue_size: usize,
    critical_queue_size: usize,
    throttle_delay_ms: u64,
}

impl BackpressureController {
    pub fn new(
        degraded_latency: u64,
        critical_latency: u64,
        degraded_queue: usize,
        critical_queue: usize,
        throttle_delay: u64,
    ) -> Self {
        Self {
            degraded_latency_ms: degraded_latency,
            critical_latency_ms: critical_latency,
            degraded_queue_size: degraded_queue,
            critical_queue_size: critical_queue,
            throttle_delay_ms: throttle_delay,
        }
    }

    pub fn calculate_status(&self, latency: u64, queue_depth: usize) -> BackpressureStatus {
        let strategy = if latency >= self.critical_latency_ms || queue_depth >= self.critical_queue_size {
            IngestionStrategy::Paused
        } else if latency >= self.degraded_latency_ms || queue_depth >= self.degraded_queue_size {
            IngestionStrategy::Throttled
        } else {
            IngestionStrategy::Normal
        };

        BackpressureStatus {
            strategy,
            db_latency_ms: latency,
            queue_depth,
        }
    }

    pub fn get_delay(&self, strategy: IngestionStrategy) -> Duration {
        match strategy {
            IngestionStrategy::Normal => Duration::from_millis(0),
            IngestionStrategy::Throttled => Duration::from_millis(self.throttle_delay_ms),
            IngestionStrategy::Paused => Duration::from_secs(5), // Re-check interval when paused
        }
    }
}

impl Default for BackpressureController {
    fn default() -> Self {
        Self::new(200, 1000, 5000, 10000, 500)
    }
}