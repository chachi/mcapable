use std::collections::VecDeque;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, BufWriter, Seek, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::SystemTime;

use super::types::{ClosedFileContext, SplitContext};

/// A factory that produces new sinks for each MCAP file in a rolling sequence.
///
/// The factory is called each time the rolling writer needs to start a new file.
/// It receives a [`SplitContext`] describing why and when the split occurred.
pub trait SinkFactory {
    /// The sink type produced by this factory.
    type Sink: Write + Seek;

    /// Create a new sink for writing the next MCAP file.
    fn create_sink(&mut self, context: &SplitContext) -> io::Result<Self::Sink>;

    /// Called after a file has been finalized and closed.
    ///
    /// Use this for post-processing such as compression or upload.
    /// The default implementation does nothing.
    fn on_file_closed(&mut self, _context: &ClosedFileContext) {}
}

/// Shared file-rotation state for filesystem-based factories.
struct FileRotationBase {
    directory: PathBuf,
    prefix: String,
    max_files: Option<usize>,
    created_files: VecDeque<PathBuf>,
}

impl FileRotationBase {
    fn new(directory: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            prefix: prefix.into(),
            max_files: None,
            created_files: VecDeque::new(),
        }
    }

    fn enforce_max_files(&mut self) -> io::Result<()> {
        if let Some(max) = self.max_files {
            while self.created_files.len() > max {
                if let Some(old_path) = self.created_files.pop_front() {
                    match fs::remove_file(&old_path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        Ok(())
    }

    fn create_sink(&mut self, path: PathBuf) -> io::Result<BufWriter<File>> {
        fs::create_dir_all(&self.directory)?;
        let file = File::create(&path)?;
        self.created_files.push_back(path);
        self.enforce_max_files()?;
        Ok(BufWriter::new(file))
    }
}

/// A sink factory that creates sequentially numbered files in a directory.
///
/// Produces files like `prefix_000000.mcap`, `prefix_000001.mcap`, etc.
///
/// ```no_run
/// use mcapable_core::writer::rolling::SequentialFiles;
///
/// let factory = SequentialFiles::new("./output", "recording")
///     .max_files(10);
/// ```
pub struct SequentialFiles {
    base: FileRotationBase,
}

impl SequentialFiles {
    /// Create a factory that writes files to `directory` with the given `prefix`.
    pub fn new(directory: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        Self {
            base: FileRotationBase::new(directory, prefix),
        }
    }

    /// Set the maximum number of files to keep. When exceeded, the oldest file is deleted.
    ///
    /// Files that have already been removed externally are silently skipped.
    /// Other deletion errors (e.g. permission denied) are propagated through
    /// [`SinkFactory::create_sink`].
    pub fn max_files(mut self, max: usize) -> Self {
        self.base.max_files = Some(max);
        self
    }

    fn path_for_index(&self, index: usize) -> PathBuf {
        self.base
            .directory
            .join(format!("{}_{:06}.mcap", self.base.prefix, index))
    }
}

impl SinkFactory for SequentialFiles {
    type Sink = BufWriter<File>;

    fn create_sink(&mut self, context: &SplitContext) -> io::Result<Self::Sink> {
        let path = self.path_for_index(context.file_index);
        self.base.create_sink(path)
    }
}

/// A sink factory that names files using UTC timestamps.
///
/// Produces files like `prefix_20260330T143022Z.mcap`.
///
/// ```no_run
/// use mcapable_core::writer::rolling::TimestampFiles;
///
/// let factory = TimestampFiles::new("./output", "recording")
///     .max_files(10);
/// ```
pub struct TimestampFiles {
    base: FileRotationBase,
}

impl TimestampFiles {
    /// Create a factory that writes timestamp-named files to `directory` with the given `prefix`.
    pub fn new(directory: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        Self {
            base: FileRotationBase::new(directory, prefix),
        }
    }

    /// Set the maximum number of files to keep. When exceeded, the oldest file is deleted.
    ///
    /// Files that have already been removed externally are silently skipped.
    /// Other deletion errors (e.g. permission denied) are propagated through
    /// [`SinkFactory::create_sink`].
    pub fn max_files(mut self, max: usize) -> Self {
        self.base.max_files = Some(max);
        self
    }
}

impl SinkFactory for TimestampFiles {
    type Sink = BufWriter<File>;

    fn create_sink(&mut self, context: &SplitContext) -> io::Result<Self::Sink> {
        let timestamp = format_utc_timestamp(context.split_time);
        let path = self
            .base
            .directory
            .join(format!("{}_{}.mcap", self.base.prefix, timestamp));
        self.base.create_sink(path)
    }
}

/// A sink factory backed by a user-provided closure.
///
/// ```no_run
/// use mcapable_core::writer::rolling::FnSinkFactory;
/// use std::fs::File;
/// use std::io::BufWriter;
///
/// let factory = FnSinkFactory::new(|ctx| {
///     let path = format!("recording_{:04}.mcap", ctx.file_index);
///     Ok(BufWriter::new(File::create(path)?))
/// });
/// ```
pub struct FnSinkFactory<F, W>
where
    F: FnMut(&SplitContext) -> io::Result<W>,
    W: Write + Seek,
{
    create_fn: F,
    _phantom: PhantomData<W>,
}

impl<F, W> FnSinkFactory<F, W>
where
    F: FnMut(&SplitContext) -> io::Result<W>,
    W: Write + Seek,
{
    /// Create a sink factory from a closure.
    pub fn new(create_fn: F) -> Self {
        Self {
            create_fn,
            _phantom: PhantomData,
        }
    }
}

impl<F, W> SinkFactory for FnSinkFactory<F, W>
where
    F: FnMut(&SplitContext) -> io::Result<W>,
    W: Write + Seek,
{
    type Sink = W;

    fn create_sink(&mut self, context: &SplitContext) -> io::Result<Self::Sink> {
        (self.create_fn)(context)
    }
}

fn format_utc_timestamp(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();

    let time_of_day = total_secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from unix timestamp using Howard Hinnant's algorithm.
    // Reference: http://howardhinnant.github.io/date_algorithms.html#civil_from_days
    let z = (total_secs / 86400) as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    let mut buf = String::with_capacity(16);
    let _ = write!(
        buf,
        "{y:04}{m:02}{d:02}T{hours:02}{minutes:02}{seconds:02}Z"
    );
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;

    fn split_context(file_index: usize) -> SplitContext {
        SplitContext {
            file_index,
            trigger_name: None,
            split_time: SystemTime::now(),
            prev_log_time_range: None,
        }
    }

    #[test]
    fn sequential_path_format() {
        let factory = SequentialFiles::new("/tmp/test", "rec");
        assert_eq!(
            factory.path_for_index(0),
            PathBuf::from("/tmp/test/rec_000000.mcap")
        );
        assert_eq!(
            factory.path_for_index(42),
            PathBuf::from("/tmp/test/rec_000042.mcap")
        );
    }

    #[test]
    fn utc_timestamp_format() {
        // Unix epoch
        let ts = format_utc_timestamp(SystemTime::UNIX_EPOCH);
        assert_eq!(ts, "19700101T000000Z");

        // Known timestamp: 2026-03-31 18:30:22 UTC = 1774981822
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1774981822);
        let ts = format_utc_timestamp(time);
        assert_eq!(ts, "20260331T183022Z");
    }

    #[test]
    fn sequential_files_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut factory = SequentialFiles::new(dir.path(), "test");

        let mut sink = factory.create_sink(&split_context(0)).unwrap();
        sink.write_all(b"hello").unwrap();
        drop(sink);

        let mut sink = factory.create_sink(&split_context(1)).unwrap();
        sink.write_all(b"world").unwrap();
        drop(sink);

        assert!(dir.path().join("test_000000.mcap").exists());
        assert!(dir.path().join("test_000001.mcap").exists());
    }

    #[test]
    fn sequential_files_max_files_evicts() {
        let dir = tempfile::tempdir().unwrap();
        let mut factory = SequentialFiles::new(dir.path(), "rec").max_files(2);

        for i in 0..3 {
            let mut sink = factory.create_sink(&split_context(i)).unwrap();
            sink.write_all(b"data").unwrap();
            drop(sink);
        }

        assert!(
            !dir.path().join("rec_000000.mcap").exists(),
            "oldest evicted"
        );
        assert!(dir.path().join("rec_000001.mcap").exists());
        assert!(dir.path().join("rec_000002.mcap").exists());
    }

    #[test]
    fn sequential_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("nested").join("deep");
        let mut factory = SequentialFiles::new(&subdir, "x");

        let sink = factory.create_sink(&split_context(0)).unwrap();
        drop(sink);

        assert!(subdir.join("x_000000.mcap").exists());
    }

    #[test]
    fn timestamp_files_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut factory = TimestampFiles::new(dir.path(), "log");

        let ctx = SplitContext {
            file_index: 0,
            trigger_name: None,
            split_time: SystemTime::UNIX_EPOCH + Duration::from_secs(1774981822),
            prev_log_time_range: None,
        };
        let mut sink = factory.create_sink(&ctx).unwrap();
        sink.write_all(b"data").unwrap();
        drop(sink);

        assert!(dir.path().join("log_20260331T183022Z.mcap").exists());
    }

    #[test]
    fn timestamp_files_max_files_evicts() {
        let dir = tempfile::tempdir().unwrap();
        let mut factory = TimestampFiles::new(dir.path(), "log").max_files(2);

        let base_secs = 1774981822u64;
        for i in 0..3u64 {
            let ctx = SplitContext {
                file_index: i as usize,
                trigger_name: None,
                split_time: SystemTime::UNIX_EPOCH + Duration::from_secs(base_secs + i),
                prev_log_time_range: None,
            };
            let mut sink = factory.create_sink(&ctx).unwrap();
            sink.write_all(b"data").unwrap();
            drop(sink);
        }

        // First file should be evicted
        assert!(!dir.path().join("log_20260331T183022Z.mcap").exists());
        assert!(dir.path().join("log_20260331T183023Z.mcap").exists());
        assert!(dir.path().join("log_20260331T183024Z.mcap").exists());
    }

    #[test]
    fn enforce_max_files_ignores_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let mut factory = SequentialFiles::new(dir.path(), "x").max_files(1);

        // Create first file
        let mut sink = factory.create_sink(&split_context(0)).unwrap();
        sink.write_all(b"a").unwrap();
        drop(sink);

        // Manually remove it so enforce_max_files encounters NotFound
        fs::remove_file(dir.path().join("x_000000.mcap")).unwrap();

        // Creating a second file should succeed (NotFound is ignored)
        let sink = factory.create_sink(&split_context(1)).unwrap();
        drop(sink);

        assert!(dir.path().join("x_000001.mcap").exists());
    }

    #[test]
    fn fn_sink_factory_direct() {
        let mut received_index = None;
        let mut factory = FnSinkFactory::new(|ctx: &SplitContext| {
            received_index = Some(ctx.file_index);
            Ok(Cursor::new(Vec::new()))
        });

        let _sink = factory.create_sink(&split_context(42)).unwrap();
        assert_eq!(received_index, Some(42));
    }
}
