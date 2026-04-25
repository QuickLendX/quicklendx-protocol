use std::sync::Arc;
use tokio::sync::RwLock;
use crate::indexer::backpressure::{BackpressureController, BackpressureStatus, IngestionStrategy};

pub struct IndexerMonitor {
    controller: BackpressureController,
    current_status: Arc<RwLock<BackpressureStatus>>,
}

impl IndexerMonitor {
    pub fn new(controller: BackpressureController) -> Self {
        Self {
            controller,
            current_status: Arc::new(RwLock::new(BackpressureStatus {
                strategy: IngestionStrategy::Normal,
                db_latency_ms: 0,
                queue_depth: 0,
            })),
        }
    }

    /// Updates the current health metrics and recalculates strategy
    pub async fn update_health(&self, db_latency: u64, queue_depth: usize) {
        let new_status = self.controller.calculate_status(db_latency, queue_depth);
        let mut status = self.current_status.write().await;
        *status = new_status;
    }

    /// Returns a copy of the current status for monitoring
    pub async fn get_status(&self) -> BackpressureStatus {
        self.current_status.read().await.clone()
    }

    /// Applies backpressure delay if the current strategy warrants it
    pub async fn wait_if_needed(&self) {
        let strategy = self.current_status.read().await.strategy;
        let delay = self.controller.get_delay(strategy);
        
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
    
    /// Helper to check if ingestion is paused
    pub async fn is_paused(&self) -> bool {
        self.current_status.read().await.strategy == IngestionStrategy::Paused
    }
}