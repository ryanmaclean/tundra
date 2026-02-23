use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, watch};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// ShutdownSignal — cooperative shutdown coordination
// ---------------------------------------------------------------------------

/// Broadcast-based shutdown coordinator.
///
/// Components register interest in shutdown by calling `subscribe()`, then
/// `select!` on the returned receiver alongside their main work loop.
///
/// The orchestrator triggers shutdown by calling `trigger()`, which:
/// 1. Sets the `is_shutting_down` flag (atomically)
/// 2. Broadcasts a signal to all subscribers
/// 3. Optionally waits for all components to confirm drain
///
/// ```ignore
/// let shutdown = ShutdownSignal::new();
/// let mut rx = shutdown.subscribe();
///
/// tokio::select! {
///     _ = rx.recv() => { /* graceful cleanup */ }
///     _ = do_work() => {}
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    /// Broadcast sender — triggers shutdown for all subscribers.
    trigger: broadcast::Sender<()>,
    /// Atomic flag for cheap polling.
    shutting_down: Arc<AtomicBool>,
    /// Watch channel for drain confirmation.
    drain_tx: Arc<watch::Sender<usize>>,
    drain_rx: watch::Receiver<usize>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        let (trigger, _) = broadcast::channel(1);
        let (drain_tx, drain_rx) = watch::channel(0);
        Self {
            trigger,
            shutting_down: Arc::new(AtomicBool::new(false)),
            drain_tx: Arc::new(drain_tx),
            drain_rx,
        }
    }

    /// Subscribe to the shutdown signal.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.trigger.subscribe()
    }

    /// Check if shutdown has been triggered (non-blocking).
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Relaxed)
    }

    /// Trigger shutdown for all subscribers.
    pub fn trigger(&self) {
        if self
            .shutting_down
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            info!("shutdown signal triggered");
            let _ = self.trigger.send(());
        } else {
            warn!("shutdown already triggered");
        }
    }

    /// Notify that a component has finished draining.
    pub fn confirm_drained(&self) {
        self.drain_tx.send_modify(|count| *count += 1);
    }

    /// Wait for `expected` components to confirm drain, with a timeout.
    pub async fn wait_for_drain(&mut self, expected: usize, timeout: Duration) -> DrainResult {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let current = *self.drain_rx.borrow();
            if current >= expected {
                info!(count = current, "all components drained");
                return DrainResult::Complete(current);
            }

            match tokio::time::timeout_at(deadline, self.drain_rx.changed()).await {
                Ok(Ok(())) => continue,
                Ok(Err(_)) => {
                    // Sender dropped
                    let current = *self.drain_rx.borrow();
                    return DrainResult::Complete(current);
                }
                Err(_) => {
                    let current = *self.drain_rx.borrow();
                    warn!(
                        current,
                        expected, "drain timeout — some components did not confirm"
                    );
                    return DrainResult::Timeout {
                        confirmed: current,
                        expected,
                    };
                }
            }
        }
    }

    /// Number of subscribers currently listening.
    pub fn subscriber_count(&self) -> usize {
        self.trigger.receiver_count()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DrainResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrainResult {
    /// All expected components confirmed drain.
    Complete(usize),
    /// Timeout expired before all components confirmed.
    Timeout { confirmed: usize, expected: usize },
}

impl DrainResult {
    pub fn is_complete(&self) -> bool {
        matches!(self, DrainResult::Complete(_))
    }
}

// ---------------------------------------------------------------------------
// ShutdownGuard — RAII guard that confirms drain on drop
// ---------------------------------------------------------------------------

/// RAII guard that calls `confirm_drained()` when dropped.
///
/// Give one to each component that needs to participate in graceful shutdown.
/// When the component finishes its drain, dropping the guard signals completion.
pub struct ShutdownGuard {
    signal: ShutdownSignal,
}

impl ShutdownGuard {
    pub fn new(signal: ShutdownSignal) -> Self {
        Self { signal }
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        self.signal.confirm_drained();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_signal_is_not_shutting_down() {
        let signal = ShutdownSignal::new();
        assert!(!signal.is_shutting_down());
    }

    #[test]
    fn trigger_sets_flag() {
        let signal = ShutdownSignal::new();
        signal.trigger();
        assert!(signal.is_shutting_down());
    }

    #[test]
    fn double_trigger_is_idempotent() {
        let signal = ShutdownSignal::new();
        signal.trigger();
        signal.trigger(); // no panic
        assert!(signal.is_shutting_down());
    }

    #[test]
    fn subscriber_count() {
        let signal = ShutdownSignal::new();
        assert_eq!(signal.subscriber_count(), 0);
        let _rx1 = signal.subscribe();
        assert_eq!(signal.subscriber_count(), 1);
        let _rx2 = signal.subscribe();
        assert_eq!(signal.subscriber_count(), 2);
        drop(_rx1);
        assert_eq!(signal.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn subscribe_receives_trigger() {
        let signal = ShutdownSignal::new();
        let mut rx = signal.subscribe();

        signal.trigger();

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn drain_completes_when_all_confirm() {
        let mut signal = ShutdownSignal::new();
        let guard1 = ShutdownGuard::new(signal.clone());
        let guard2 = ShutdownGuard::new(signal.clone());

        signal.trigger();

        // Simulate async drain
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            drop(guard1);
        });
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(guard2);
        });

        let result = signal.wait_for_drain(2, Duration::from_secs(1)).await;
        assert!(result.is_complete());
    }

    #[tokio::test]
    async fn drain_timeout_when_not_all_confirm() {
        let mut signal = ShutdownSignal::new();
        let _guard = ShutdownGuard::new(signal.clone());
        // Intentionally don't drop the guard

        signal.trigger();

        let result = signal.wait_for_drain(2, Duration::from_millis(50)).await;
        match result {
            DrainResult::Timeout {
                confirmed,
                expected,
            } => {
                assert_eq!(confirmed, 0);
                assert_eq!(expected, 2);
            }
            _ => panic!("expected timeout"),
        }
    }

    #[test]
    fn drain_result_is_complete() {
        assert!(DrainResult::Complete(3).is_complete());
        assert!(!DrainResult::Timeout {
            confirmed: 1,
            expected: 3
        }
        .is_complete());
    }

    #[test]
    fn clone_shares_state() {
        let signal = ShutdownSignal::new();
        let clone = signal.clone();

        signal.trigger();
        assert!(clone.is_shutting_down());
    }

    #[tokio::test]
    async fn guard_confirms_on_drop() {
        let mut signal = ShutdownSignal::new();
        {
            let _guard = ShutdownGuard::new(signal.clone());
            // guard dropped here
        }

        let result = signal.wait_for_drain(1, Duration::from_millis(100)).await;
        assert!(result.is_complete());
    }

    #[test]
    fn default_creates_new_signal() {
        let signal = ShutdownSignal::default();
        assert!(!signal.is_shutting_down());
    }
}
