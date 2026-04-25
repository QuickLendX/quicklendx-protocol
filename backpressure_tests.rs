#[cfg(test)]
mod tests {
    use crate::indexer::backpressure::{BackpressureController, IngestionStrategy};
    use crate::indexer::monitor::IndexerMonitor;
    use std::time::Duration;

    #[tokio::test]
    async fn test_normal_operation() {
        let controller = BackpressureController::default();
        let monitor = IndexerMonitor::new(controller);

        monitor.update_health(50, 100).await;
        let status = monitor.get_status().await;
        
        assert_eq!(status.strategy, IngestionStrategy::Normal);
        assert!(!monitor.is_paused().await);
    }

    #[tokio::test]
    async fn test_degradation_to_throttle() {
        let controller = BackpressureController::default();
        let monitor = IndexerMonitor::new(controller);

        // Exceed latency threshold (default 200ms)
        monitor.update_health(250, 100).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Throttled);

        // Exceed queue threshold (default 5000)
        monitor.update_health(50, 6000).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Throttled);
    }

    #[tokio::test]
    async fn test_degradation_to_pause() {
        let controller = BackpressureController::default();
        let monitor = IndexerMonitor::new(controller);

        // Critical latency (default 1000ms)
        monitor.update_health(1200, 100).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Paused);
        assert!(monitor.is_paused().await);

        // Critical queue (default 10000)
        monitor.update_health(50, 15000).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Paused);
    }

    #[tokio::test]
    async fn test_recovery_flow() {
        let controller = BackpressureController::default();
        let monitor = IndexerMonitor::new(controller);

        // Start paused
        monitor.update_health(2000, 20000).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Paused);

        // Recover to throttled
        monitor.update_health(300, 100).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Throttled);

        // Full recovery
        monitor.update_health(50, 100).await;
        assert_eq!(monitor.get_status().await.strategy, IngestionStrategy::Normal);
    }

    #[tokio::test]
    async fn test_wait_logic_timing() {
        let controller = BackpressureController::new(100, 500, 1000, 5000, 200);
        let monitor = IndexerMonitor::new(controller);

        monitor.update_health(150, 100).await; // Throttled
        
        let start = std::time::Instant::now();
        monitor.wait_if_needed().await;
        let elapsed = start.elapsed();

        // Should wait at least 200ms
        assert!(elapsed >= Duration::from_millis(200));
        assert!(elapsed < Duration::from_millis(250));
    }
}