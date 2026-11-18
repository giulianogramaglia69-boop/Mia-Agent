//! Observability hook: records every loop event for inspection.
//!
//! ```rust
//! use mia_runtime::RecordingHook;
//!
//! let hook = RecordingHook::new();
//! assert_eq!(hook.len(), 0);
//! ```

use std::sync::Mutex;

use harness_core::{Event, Hook, HookOutcome, World};

/// A [`Hook`] that records a short summary of every event the loop
/// emits. The runtime exposes it so tests and the UI can show what
/// the agent actually did, without a tracing subscriber.
pub struct RecordingHook {
    events: Mutex<Vec<String>>,
}

impl Default for RecordingHook {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingHook {
    /// An empty recorder.
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// Recorded event summaries, in order.
    pub fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    /// How many events have been recorded.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Whether any event has been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Hook for RecordingHook {
    fn name(&self) -> &str {
        "mia-recorder"
    }

    fn matches(&self, _ev: &Event<'_>) -> bool {
        true
    }

    fn fire(&self, ev: &Event<'_>, _world: &mut World) -> HookOutcome {
        let summary: String = format!("{ev:?}").chars().take(500).collect();
        self.events.lock().unwrap().push(summary);
        HookOutcome::Allow
    }
}
