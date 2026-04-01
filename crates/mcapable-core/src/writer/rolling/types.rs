use std::time::{Duration, SystemTime};

/// Snapshot of rolling writer state, passed to triggers after each message write.
#[derive(Debug, Clone)]
pub struct TriggerState {
    /// Estimated current file size including buffered (unflushed) chunk data.
    ///
    /// This is a conservative overestimate: buffered chunk data is counted at its
    /// uncompressed size, so the actual file size will typically be smaller when
    /// compression is enabled.
    pub estimated_file_size: u64,

    /// Number of messages written to the current file.
    pub message_count: u64,

    /// Wall-clock duration since the current file was opened.
    pub wall_elapsed: Duration,

    /// Log-time span in the current file (`log_time_end - log_time_start`), in nanoseconds.
    ///
    /// Zero if no messages have been written, or if all messages share the same log_time.
    pub log_time_span: u64,

    /// The `log_time` of the message that was just written.
    pub current_log_time: u64,
}

/// Information about a split event, passed to the sink factory and callbacks.
#[derive(Debug, Clone)]
pub struct SplitContext {
    /// Zero-based index of the file about to be created.
    pub file_index: usize,

    /// The name of the trigger that fired, or `None` for the initial file.
    pub trigger_name: Option<String>,

    /// Wall-clock time of the split.
    pub split_time: SystemTime,

    /// Log-time range `(start, end)` of the file that just closed, if it had messages.
    pub prev_log_time_range: Option<(u64, u64)>,
}

/// Information about a file that has been finalized and closed.
#[derive(Debug, Clone)]
pub struct ClosedFileContext {
    /// Zero-based index of the closed file.
    pub file_index: usize,

    /// Final byte size of the closed file.
    pub file_size: u64,

    /// Number of messages written to the closed file.
    pub message_count: u64,

    /// Log-time range `(start, end)` of messages in the closed file, if any.
    pub log_time_range: Option<(u64, u64)>,
}
