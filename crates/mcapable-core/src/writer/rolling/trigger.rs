use std::time::Duration;

use super::types::TriggerState;

/// A trigger that decides when to split to a new file.
///
/// Triggers are checked after each message write. Multiple triggers can be
/// composed with [`AnyTrigger`] (first to fire wins).
pub trait SplitTrigger {
    /// Return `true` if the current file should be finalized and a new one started.
    fn should_split(&mut self, state: &TriggerState) -> bool;

    /// Called when a split occurs. Resets any internal state for the new file.
    fn reset(&mut self);

    /// Human-readable name for logging and diagnostics.
    fn name(&self) -> &str;
}

/// Split when the estimated file size exceeds a threshold.
///
/// The estimate includes buffered chunk data that has not yet been flushed to disk,
/// so files will respect the size limit even with large chunk buffers.
///
/// ```
/// use mcapable_core::writer::rolling::MaxSize;
///
/// let trigger = MaxSize::new(100_000_000); // 100 MB
/// ```
pub struct MaxSize {
    max_bytes: u64,
}

impl MaxSize {
    /// Create a trigger that fires when the estimated file size exceeds `max_bytes`.
    pub fn new(max_bytes: u64) -> Self {
        Self { max_bytes }
    }
}

impl SplitTrigger for MaxSize {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        state.estimated_file_size > self.max_bytes
    }

    fn reset(&mut self) {}

    fn name(&self) -> &str {
        "MaxSize"
    }
}

/// Split after a wall-clock duration has elapsed since the file was opened.
///
/// ```
/// use mcapable_core::writer::rolling::WallDuration;
/// use std::time::Duration;
///
/// let trigger = WallDuration::new(Duration::from_secs(300)); // 5 minutes
/// ```
pub struct WallDuration {
    max_duration: Duration,
}

impl WallDuration {
    /// Create a trigger that fires when wall-clock time since file open exceeds `max_duration`.
    pub fn new(max_duration: Duration) -> Self {
        Self { max_duration }
    }
}

impl SplitTrigger for WallDuration {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        state.wall_elapsed > self.max_duration
    }

    fn reset(&mut self) {}

    fn name(&self) -> &str {
        "WallDuration"
    }
}

/// Split when the log-time span within the current file exceeds a threshold.
///
/// The span is `log_time_end - log_time_start` in nanoseconds.
///
/// ```
/// use mcapable_core::writer::rolling::LogDuration;
///
/// let trigger = LogDuration::new(5_000_000_000); // 5 seconds of log time
/// ```
pub struct LogDuration {
    max_duration_ns: u64,
}

impl LogDuration {
    /// Create a trigger that fires when the log-time span exceeds `max_duration_ns` nanoseconds.
    pub fn new(max_duration_ns: u64) -> Self {
        Self { max_duration_ns }
    }
}

impl SplitTrigger for LogDuration {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        state.log_time_span > self.max_duration_ns
    }

    fn reset(&mut self) {}

    fn name(&self) -> &str {
        "LogDuration"
    }
}

/// Split after a fixed number of messages have been written to the current file.
///
/// ```
/// use mcapable_core::writer::rolling::MessageCount;
///
/// let trigger = MessageCount::new(100_000); // 100k messages per file
/// ```
pub struct MessageCount {
    max_messages: u64,
}

impl MessageCount {
    /// Create a trigger that fires when the message count reaches `max_messages`.
    pub fn new(max_messages: u64) -> Self {
        Self { max_messages }
    }
}

impl SplitTrigger for MessageCount {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        state.message_count >= self.max_messages
    }

    fn reset(&mut self) {}

    fn name(&self) -> &str {
        "MessageCount"
    }
}

/// Split based on a user-provided closure.
///
/// ```
/// use mcapable_core::writer::rolling::FnTrigger;
///
/// let trigger = FnTrigger::new("custom", |state| {
///     state.message_count >= 50_000 && state.estimated_file_size > 10_000_000
/// });
/// ```
pub struct FnTrigger<F> {
    name: String,
    func: F,
}

impl<F: FnMut(&TriggerState) -> bool> FnTrigger<F> {
    /// Create a trigger that fires when `func` returns `true`.
    pub fn new(name: impl Into<String>, func: F) -> Self {
        Self {
            name: name.into(),
            func,
        }
    }
}

impl<F: FnMut(&TriggerState) -> bool> SplitTrigger for FnTrigger<F> {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        (self.func)(state)
    }

    fn reset(&mut self) {}

    fn name(&self) -> &str {
        &self.name
    }
}

/// Compose multiple triggers: split when ANY child trigger fires.
///
/// ```
/// use mcapable_core::writer::rolling::{AnyTrigger, MaxSize, MessageCount};
///
/// let trigger = AnyTrigger::new()
///     .or(MaxSize::new(100_000_000))
///     .or(MessageCount::new(500_000));
/// ```
pub struct AnyTrigger {
    triggers: Vec<Box<dyn SplitTrigger>>,
    last_fired: Option<String>,
}

impl AnyTrigger {
    /// Create an empty composite trigger.
    pub fn new() -> Self {
        Self {
            triggers: Vec::new(),
            last_fired: None,
        }
    }

    /// Add a child trigger. The composite fires when any child fires.
    pub fn or<T: SplitTrigger + 'static>(mut self, trigger: T) -> Self {
        self.triggers.push(Box::new(trigger));
        self
    }
}

impl Default for AnyTrigger {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitTrigger for AnyTrigger {
    fn should_split(&mut self, state: &TriggerState) -> bool {
        for trigger in &mut self.triggers {
            if trigger.should_split(state) {
                self.last_fired = Some(trigger.name().to_string());
                return true;
            }
        }
        false
    }

    fn reset(&mut self) {
        self.last_fired = None;
        for trigger in &mut self.triggers {
            trigger.reset();
        }
    }

    fn name(&self) -> &str {
        self.last_fired.as_deref().unwrap_or("AnyTrigger")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn state(overrides: impl FnOnce(&mut TriggerState)) -> TriggerState {
        let mut s = TriggerState {
            estimated_file_size: 0,
            message_count: 0,
            wall_elapsed: Duration::ZERO,
            log_time_span: 0,
            current_log_time: 0,
        };
        overrides(&mut s);
        s
    }

    #[test]
    fn max_size_trigger() {
        let mut t = MaxSize::new(100);
        assert!(!t.should_split(&state(|s| s.estimated_file_size = 99)));
        assert!(!t.should_split(&state(|s| s.estimated_file_size = 100)));
        assert!(t.should_split(&state(|s| s.estimated_file_size = 101)));
    }

    #[test]
    fn wall_duration_trigger() {
        let mut t = WallDuration::new(Duration::from_secs(10));
        assert!(!t.should_split(&state(|s| s.wall_elapsed = Duration::from_secs(9))));
        assert!(!t.should_split(&state(|s| s.wall_elapsed = Duration::from_secs(10))));
        assert!(t.should_split(&state(|s| s.wall_elapsed = Duration::from_secs(11))));
    }

    #[test]
    fn log_duration_trigger() {
        let mut t = LogDuration::new(1_000_000_000);
        assert!(!t.should_split(&state(|s| s.log_time_span = 999_999_999)));
        assert!(!t.should_split(&state(|s| s.log_time_span = 1_000_000_000)));
        assert!(t.should_split(&state(|s| s.log_time_span = 1_000_000_001)));
    }

    #[test]
    fn message_count_trigger() {
        let mut t = MessageCount::new(100);
        assert!(!t.should_split(&state(|s| s.message_count = 99)));
        assert!(t.should_split(&state(|s| s.message_count = 100)));
        assert!(t.should_split(&state(|s| s.message_count = 101)));
    }

    #[test]
    fn fn_trigger() {
        let mut t = FnTrigger::new("custom", |s: &TriggerState| s.message_count > 5);
        assert!(!t.should_split(&state(|s| s.message_count = 5)));
        assert!(t.should_split(&state(|s| s.message_count = 6)));
        assert_eq!(t.name(), "custom");
    }

    #[test]
    fn any_trigger_first_to_fire() {
        let mut t = AnyTrigger::new()
            .or(MaxSize::new(1000))
            .or(MessageCount::new(5));

        // Neither fires
        assert!(!t.should_split(&state(|s| {
            s.estimated_file_size = 500;
            s.message_count = 3;
        })));

        // MessageCount fires first
        assert!(t.should_split(&state(|s| {
            s.estimated_file_size = 500;
            s.message_count = 5;
        })));
        assert_eq!(t.name(), "MessageCount");

        // Reset
        t.reset();

        // MaxSize fires
        assert!(t.should_split(&state(|s| {
            s.estimated_file_size = 1001;
            s.message_count = 1;
        })));
        assert_eq!(t.name(), "MaxSize");
    }

    #[test]
    fn any_trigger_empty() {
        let mut t = AnyTrigger::new();
        assert!(!t.should_split(&state(|_| {})));
        assert_eq!(t.name(), "AnyTrigger");
    }
}
