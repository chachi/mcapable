use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[path = "cli/cmd/mod.rs"]
pub mod cmd;
#[path = "cli/input.rs"]
pub mod input;

#[derive(Debug, Parser)]
#[command(name = "mcapable")]
#[command(version)]
#[command(about = "MCAP CLI powered by the mcapable crate")]
pub struct Cli {
    /// Path to a config file (currently ignored; accepted for CLI compatibility).
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Write a pprof profile to the given file (currently ignored).
    #[arg(long = "pprof-profile", global = true)]
    pub pprof_profile: Option<PathBuf>,

    /// Require that messages have a monotonic log time.
    #[arg(long = "strict-message-order", global = true)]
    pub strict_message_order: bool,

    /// Enable verbose output.
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the mcapable CLI version.
    Version,

    /// Generate shell completion scripts.
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Report statistics about an MCAP file.
    Info {
        #[arg(value_name = "file")]
        input: Option<String>,
    },

    /// List records in an MCAP file.
    List {
        #[command(subcommand)]
        command: cmd::ListCommand,
    },

    /// Print disk usage stats for an MCAP file.
    Du {
        #[arg(value_name = "file")]
        input: Option<String>,
    },

    /// Retrieve a specific message by index.
    Get {
        /// Index of the message to retrieve.
        #[arg(value_name = "index")]
        index: usize,

        /// Path to the MCAP file.
        #[arg(value_name = "file")]
        input: Option<String>,
    },

    /// Filter messages by topic and/or time range.
    Filter {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Topic patterns to include (comma-separated).
        #[arg(long, value_delimiter = ',')]
        topics: Vec<String>,

        /// Start time in nanoseconds.
        #[arg(long)]
        start: Option<u64>,

        /// End time in nanoseconds.
        #[arg(long)]
        end: Option<u64>,

        /// Start time in seconds (alternative to --start).
        #[arg(long)]
        start_secs: Option<u64>,

        /// Start time nanoseconds part.
        #[arg(long)]
        start_nsecs: Option<u32>,

        /// End time in seconds (alternative to --end).
        #[arg(long)]
        end_secs: Option<u64>,

        /// End time nanoseconds part.
        #[arg(long)]
        end_nsecs: Option<u32>,
    },

    /// Concatenate multiple MCAP files into one.
    Merge {
        /// Output file path (or - for stdout).
        #[arg(value_name = "output")]
        output: String,

        /// Input files to merge.
        #[arg(value_name = "files", required = true)]
        inputs: Vec<String>,
    },

    /// Cat messages to stdout.
    Cat {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Topic patterns to include (comma-separated).
        #[arg(long, value_delimiter = ',')]
        topics: Vec<String>,

        /// Start time in nanoseconds.
        #[arg(long)]
        start: Option<u64>,

        /// End time in nanoseconds.
        #[arg(long)]
        end: Option<u64>,

        /// Start time in seconds (alternative to --start).
        #[arg(long)]
        start_secs: Option<u64>,

        /// Start time nanoseconds part.
        #[arg(long)]
        start_nsecs: Option<u32>,

        /// End time in seconds (alternative to --end).
        #[arg(long)]
        end_secs: Option<u64>,

        /// End time nanoseconds part.
        #[arg(long)]
        end_nsecs: Option<u32>,
    },

    /// Sort messages by log time.
    Sort {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,
    },

    /// Compress chunks in an MCAP file.
    Compress {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Compression algorithm to use.
        #[arg(long, default_value = "zstd", value_parser = ["zstd", "lz4", "none"])]
        compression: String,

        /// Target chunk size in bytes.
        #[arg(long, default_value = "1048576")]
        chunk_size: usize,
    },

    /// Decompress chunks in an MCAP file.
    Decompress {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Target chunk size in bytes.
        #[arg(long, default_value = "1048576")]
        chunk_size: usize,
    },

    /// Convert MCAP file (change chunking or compression).
    Convert {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Compression algorithm to use.
        #[arg(long, value_parser = ["zstd", "lz4", "none"])]
        compression: Option<String>,

        /// Target chunk size in bytes (0 = unchunked).
        #[arg(long)]
        chunk_size: Option<usize>,
    },

    /// Validate and check MCAP file integrity.
    Doctor {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Enable verbose output showing all validated records.
        #[arg(short = 'v', long)]
        verbose: bool,
    },

    /// Add metadata or attachments to an MCAP file.
    Add {
        /// File input path.
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        /// Metadata key-value pairs (key=value).
        #[arg(long, value_delimiter = ',')]
        metadata: Vec<String>,

        /// Path to attachment file.
        #[arg(long)]
        attachment: Option<PathBuf>,

        /// Name for the attachment (defaults to filename).
        #[arg(long)]
        attachment_name: Option<String>,

        /// Content type for the attachment.
        #[arg(long)]
        attachment_type: Option<String>,
    },

    /// Generate sample MCAP files for testing.
    Generate {
        /// Output file path (or - for stdout).
        #[arg(value_name = "output")]
        output: String,

        /// Number of messages to generate.
        #[arg(long, default_value = "100")]
        count: usize,

        /// Compression to use.
        #[arg(long, value_parser = ["zstd", "lz4", "none"])]
        compression: Option<String>,

        /// Whether to generate chunked output.
        #[arg(long)]
        chunked: bool,
    },

    /// Recover data from a truncated or corrupted MCAP file.
    Recover {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,
    },
}

pub fn main_entry() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Version => cmd::version::run(),
        Command::Completion { shell } => cmd::completion::run(shell),
        Command::Info { input } => cmd::info::run(input),
        Command::List { command } => cmd::list::dispatch(command),
        Command::Du { input } => cmd::du::run(input),
        Command::Get { index, input } => cmd::get::run(index, input),
        Command::Filter {
            input,
            output,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
        } => cmd::filter::run(
            input,
            output,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
        ),
        Command::Merge { output, inputs } => cmd::merge::run(output, inputs),
        Command::Cat {
            input,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
        } => cmd::cat::run(
            input,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
        ),
        Command::Sort { input, output } => cmd::sort::run(input, output),
        Command::Compress {
            input,
            output,
            compression,
            chunk_size,
        } => cmd::compress::run(input, output, compression, chunk_size),
        Command::Decompress {
            input,
            output,
            chunk_size,
        } => cmd::decompress::run(input, output, chunk_size),
        Command::Convert {
            input,
            output,
            compression,
            chunk_size,
        } => cmd::convert::run(input, output, compression, chunk_size),
        Command::Doctor { input, verbose } => cmd::doctor::run(input, verbose),
        Command::Add {
            input,
            output,
            metadata,
            attachment,
            attachment_name,
            attachment_type,
        } => cmd::add::run(
            input,
            output,
            metadata,
            attachment,
            attachment_name,
            attachment_type,
        ),
        Command::Generate {
            output,
            count,
            compression,
            chunked,
        } => cmd::generate::run(output, count, compression, chunked),
        Command::Recover { input, output } => cmd::recover::run(input, output),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
