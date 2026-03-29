//! ParsedStream implementation for converting messages to typed values.

mod builder;
#[cfg(feature = "parsers")]
mod defaults;
mod stream;

pub use builder::ParsedStreamBuilder;
pub use stream::ParsedStream;

use super::Stream;
use crate::types::Message;

impl<'a> Stream<'a, Message> {
    /// Create a parsed stream builder for converting messages to typed values.
    ///
    /// This allows you to register parser functions for different schema types
    /// and automatically parse message data during iteration.
    ///
    /// # Examples
    ///
    /// Parse JSON messages:
    /// ```ignore
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # use serde::{Deserialize, Serialize};
    /// # #[derive(Debug, Deserialize)]
    /// # struct SensorData { temperature: f64 }
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let parsed_stream = reader.messages()
    ///     .parsed::<SensorData>()
    ///     .parser(
    ///         |schema| schema.encoding == "json",
    ///         |data| serde_json::from_slice(data.as_ref())
    ///             .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
    ///     )
    ///     .build();
    ///
    /// for sensor_data in parsed_stream {
    ///     let data = sensor_data?;
    ///     println!("Temperature: {}", data.temperature);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Parse multiple formats:
    /// ```ignore
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # use bytes::Bytes;
    /// # fn parse_cbor(data: Bytes) -> Result<String, String> { Ok("parsed".to_string()) }
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let parsed_stream = reader.messages()
    ///     .filter_channel(|ch| ch.topic.starts_with("/sensors"))
    ///     .parsed::<String>()
    ///     .parser(
    ///         |schema| schema.encoding == "json",
    ///         |data| std::str::from_utf8(data.as_ref())
    ///             .map(|s| s.to_owned())
    ///             .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
    ///     )
    ///     .parser(
    ///         |schema| schema.encoding == "cbor",
    ///         |data| parse_cbor(data)
    ///             .map_err(|e| mcapable_core::Error::InvalidRecord(e))
    ///     )
    ///     .build();
    /// # Ok(())
    /// # }
    /// ```
    pub fn parsed<T>(self) -> ParsedStreamBuilder<'a, T> {
        ParsedStreamBuilder::new(self)
    }
}
