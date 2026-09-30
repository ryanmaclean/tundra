//! Outbound screening of agent task output before it is persisted or emitted.
//!
//! Thin glue between the executor and [`at_harness::output_guard`].

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::Task;
use chrono::Utc;

/// `EventPayload::event_type` published when task output was redacted.
pub const OUTPUT_REDACTED_EVENT: &str = "output_redacted";

/// Redact credentials and blocking prompt-injection spans from `output`.
/// When anything was redacted, logs a warning and publishes an
/// [`OUTPUT_REDACTED_EVENT`] event naming the detectors (never the secrets).
pub fn redact_task_output(bus: &EventBus, task: &Task, output: String) -> String {
    let (redacted, report) = at_harness::output_guard::guard(&output);
    if !report.needs_redaction() {
        return output;
    }
    tracing::warn!(
        task_id = %task.id,
        verdict = ?report.verdict,
        patterns = ?report.pattern_ids(),
        "redacted agent task output"
    );
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: OUTPUT_REDACTED_EVENT.to_string(),
        agent_id: None,
        bead_id: Some(task.bead_id),
        message: format!("Task {}: output redacted ({})", task.id, report.summary()),
        timestamp: Utc::now(),
    }));
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{TaskCategory, TaskComplexity, TaskPriority};
    use uuid::Uuid;

    fn task() -> Task {
        Task::new(
            "t",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        )
    }

    #[test]
    fn clean_output_passes_through_without_event() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        let out = redact_task_output(&bus, &task(), "all tests passed".into());
        assert_eq!(out, "all tests passed");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn secret_is_redacted_and_event_published() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        let key = format!("ghp_{}", "Q7vL4nR8sT1yU6hD0jF5cGaB3xK9mW2pZe8Y");
        let out = redact_task_output(&bus, &task(), format!("pushed with {key}"));
        assert_eq!(out, "pushed with [REDACTED:github_pat_classic]");
        let msg = rx.try_recv().expect("event published");
        match &*msg {
            BridgeMessage::Event(e) => {
                assert_eq!(e.event_type, OUTPUT_REDACTED_EVENT);
                assert!(e.message.contains("github_pat_classic"));
                assert!(!e.message.contains(&key));
            }
            other => panic!("unexpected message {other:?}"),
        }
    }
}
