//! `Reporter` trait — how parsers report progress and warnings back to
//! their caller, without taking a dependency on `tracing`, `log`, or
//! Tauri.
//!
//! The application provides a concrete `Reporter` that bridges to
//! whichever channel makes sense (Tauri events for the UI, stderr for
//! the test harness, a `Vec` in unit tests).

/// Sink for parser progress and warnings.
///
/// All methods take `&self` so the implementation can buffer or count
/// internally with interior mutability. `Send + Sync` so a parser can
/// be spawned across threads.
pub trait Reporter: Send + Sync {
    /// Called once before the first `progress` call. `total_estimate`
    /// is `Some` only when the parser can cheaply estimate work — for
    /// Dumper-7 we can count files cheaply but not symbols.
    fn started(&self, total_estimate: Option<u64>);

    /// Called periodically as the parser makes progress. `current` is
    /// monotonically increasing in the same units as the
    /// `total_estimate`. `label` is a short user-friendly message
    /// (`"parsing FortniteGame.hpp"`).
    fn progress(&self, current: u64, label: &str);

    /// Called when the parser encounters something it can recover from
    /// (a malformed symbol, an unrecognized line) but wants to surface.
    fn warn(&self, message: &str);

    /// Called once after the parser finishes (success or failure).
    fn finished(&self);
}

/// `Reporter` that discards everything. Useful for tests and one-off
/// scripts where progress reporting just adds noise.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullReporter;

impl Reporter for NullReporter {
    fn started(&self, _total_estimate: Option<u64>) {}
    fn progress(&self, _current: u64, _label: &str) {}
    fn warn(&self, _message: &str) {}
    fn finished(&self) {}
}

/// `Reporter` that buffers warnings into a `Mutex<Vec<String>>`. Used by
/// the ingest pipeline so it can surface warnings to `IngestReport`.
#[derive(Debug, Default)]
pub struct CollectingReporter {
    warnings: std::sync::Mutex<Vec<String>>,
    progress_calls: std::sync::Mutex<u64>,
}

impl CollectingReporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_warnings(&self) -> Vec<String> {
        let mut g = self.warnings.lock().expect("reporter mutex poisoned");
        std::mem::take(&mut *g)
    }

    pub fn progress_calls(&self) -> u64 {
        *self.progress_calls.lock().expect("reporter mutex poisoned")
    }
}

impl Reporter for CollectingReporter {
    fn started(&self, _total_estimate: Option<u64>) {}

    fn progress(&self, _current: u64, _label: &str) {
        let mut g = self.progress_calls.lock().expect("reporter mutex poisoned");
        *g += 1;
    }

    fn warn(&self, message: &str) {
        let mut g = self.warnings.lock().expect("reporter mutex poisoned");
        g.push(message.to_string());
    }

    fn finished(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_reporter_does_nothing_observable() {
        let r = NullReporter;
        r.started(Some(100));
        r.progress(10, "hi");
        r.warn("ignored");
        r.finished();
    }

    #[test]
    fn collecting_reporter_buffers_warnings() {
        let r = CollectingReporter::new();
        r.warn("a");
        r.warn("b");
        r.progress(1, "x");
        assert_eq!(r.progress_calls(), 1);
        assert_eq!(r.take_warnings(), vec!["a".to_string(), "b".to_string()]);
        // Drained on take.
        assert!(r.take_warnings().is_empty());
    }
}
