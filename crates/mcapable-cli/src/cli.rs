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
    Version {
        /// Print the mcapable-core library version instead.
        #[arg(short = 'l', long)]
        library: bool,
    },

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

        /// Faster approximation using message indexes (skips chunk decompression).
        #[arg(long)]
        approximate: bool,
    },

    /// Retrieve records from an MCAP file.
    Get {
        #[command(subcommand)]
        command: cmd::GetCommand,
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

        /// Start time (nanoseconds or RFC3339, e.g. 2024-01-01T00:00:00Z).
        #[arg(long)]
        start: Option<String>,

        /// End time (nanoseconds or RFC3339, e.g. 2024-01-01T00:00:00Z).
        #[arg(long)]
        end: Option<String>,

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

        /// Include topics matching regex (repeatable).
        #[arg(long = "include-topic-regex", short = 'y')]
        include_topic_regex: Vec<String>,

        /// Exclude topics matching regex (repeatable).
        #[arg(long = "exclude-topic-regex", short = 'n')]
        exclude_topic_regex: Vec<String>,

        /// For matching topics, include the last message before --start time (repeatable regex).
        #[arg(long = "last-per-channel-topic-regex", short = 'l')]
        last_per_channel_topic_regex: Vec<String>,

        /// Include metadata records in output.
        #[arg(long, default_value = "false", action = clap::ArgAction::Set)]
        include_metadata: bool,

        /// Include attachment records in output.
        #[arg(long, default_value = "false", action = clap::ArgAction::Set)]
        include_attachments: bool,

        #[command(flatten)]
        output_options: cmd::OutputOptions,
    },

    /// Concatenate multiple MCAP files into one.
    Merge {
        /// Output file path (or - for stdout).
        #[arg(value_name = "output")]
        output: String,

        /// Input files to merge.
        #[arg(value_name = "files", required = true)]
        inputs: Vec<String>,

        /// Channel coalescing behavior: auto, force, or none.
        #[arg(long, default_value = "auto", value_parser = ["auto", "force", "none"])]
        coalesce_channels: String,

        /// Allow duplicate-named metadata records in the output.
        #[arg(long)]
        allow_duplicate_metadata: bool,

        #[command(flatten)]
        output_options: cmd::OutputOptions,
    },

    /// Cat messages to stdout.
    Cat {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// Topic patterns to include (comma-separated).
        #[arg(long, value_delimiter = ',')]
        topics: Vec<String>,

        /// Start time (nanoseconds or RFC3339, e.g. 2024-01-01T00:00:00Z).
        #[arg(long)]
        start: Option<String>,

        /// End time (nanoseconds or RFC3339, e.g. 2024-01-01T00:00:00Z).
        #[arg(long)]
        end: Option<String>,

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

        /// Output messages as JSON (one JSON object per line).
        #[arg(long)]
        json: bool,
    },

    /// Sort messages by log time.
    Sort {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        #[command(flatten)]
        output_options: cmd::OutputOptions,
    },

    /// Compress chunks in an MCAP file.
    Compress {
        /// File input path (or - for stdin).
        #[arg(value_name = "file")]
        input: Option<String>,

        /// File output path (or - for stdout).
        #[arg(value_name = "output")]
        output: Option<String>,

        #[command(flatten)]
        output_options: cmd::OutputOptions,
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
        #[arg(long, default_value = "4194304")]
        chunk_size: usize,

        /// Include CRC checksums in chunks.
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        include_crc: bool,
    },

    /// Convert MCAP file (change chunking or compression).
    ///
    /// Unlike other output commands, `convert` uses optional flags instead of
    /// `OutputOptions` because its defaults are "preserve original" rather than
    /// fixed values: omitting `--compression` preserves the input compression,
    /// and omitting `--chunk-size` defaults to 1 MiB.
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

        /// Include CRC checksums in chunks.
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        include_crc: bool,
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
        #[command(subcommand)]
        command: cmd::AddCommand,
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

        #[command(flatten)]
        output_options: cmd::OutputOptions,
    },
}

pub fn main_entry() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Version { library } => cmd::version::run(library),
        Command::Completion { shell } => cmd::completion::run(shell),
        Command::Info { input } => cmd::info::run(input),
        Command::List { command } => cmd::list::dispatch(command),
        Command::Du { input, approximate } => cmd::du::run(input, approximate),
        Command::Get { command } => cmd::get::dispatch(command),
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
            include_topic_regex,
            exclude_topic_regex,
            last_per_channel_topic_regex,
            include_metadata,
            include_attachments,
            output_options,
        } => cmd::filter::run(cmd::filter::FilterOptions {
            input,
            output,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
            include_topic_regex,
            exclude_topic_regex,
            last_per_channel_topic_regex,
            include_metadata,
            include_attachments,
            output_options,
        }),
        Command::Merge {
            output,
            inputs,
            coalesce_channels,
            allow_duplicate_metadata,
            output_options,
        } => cmd::merge::run(
            output,
            inputs,
            output_options,
            coalesce_channels,
            allow_duplicate_metadata,
        ),
        Command::Cat {
            input,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
            json,
        } => cmd::cat::run(cmd::cat::CatOptions {
            input,
            topics,
            start,
            end,
            start_secs,
            start_nsecs,
            end_secs,
            end_nsecs,
            json,
        }),
        Command::Sort {
            input,
            output,
            output_options,
        } => cmd::sort::run(input, output, output_options),
        Command::Compress {
            input,
            output,
            output_options,
        } => cmd::compress::run(input, output, output_options),
        Command::Decompress {
            input,
            output,
            chunk_size,
            include_crc,
        } => cmd::decompress::run(input, output, chunk_size, include_crc),
        Command::Convert {
            input,
            output,
            compression,
            chunk_size,
            include_crc,
        } => cmd::convert::run(input, output, compression, chunk_size, include_crc),
        Command::Doctor { input, verbose } => cmd::doctor::run(input, verbose),
        Command::Add { command } => cmd::add::dispatch(command),
        Command::Generate {
            output,
            count,
            compression,
            chunked,
        } => cmd::generate::run(output, count, compression, chunked),
        Command::Recover {
            input,
            output,
            output_options,
        } => cmd::recover::run(input, output, output_options),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
