use super::super::{ParserFn, ParserPredicate, Stream};
use super::ParsedStream;
#[cfg(feature = "parsers")]
use super::defaults::DefaultParsers;
use crate::error::Result;
use crate::types::{Message, Schema};
use bytes::Bytes;
use std::marker::PhantomData;

/// Builder for creating a ParsedStream.
///
/// Allows registering multiple parser functions, each associated with a
/// schema predicate. When iterating, the first matching parser will be used.
pub struct ParsedStreamBuilder<'a, T> {
    /// The underlying message stream.
    message_stream: Stream<'a, Message>,
    /// Pairs of (parser predicate, parser function).
    parsers: Vec<(ParserPredicate<'a>, ParserFn<'a, T>)>,
    /// Default parser configuration for well-known encodings.
    #[cfg(feature = "parsers")]
    default_parsers: Option<DefaultParsers<'a, T>>,
    /// Phantom data for output type.
    _phantom: PhantomData<T>,
}

impl<'a, T> ParsedStreamBuilder<'a, T> {
    /// Create a new builder from a message stream.
    pub(super) fn new(message_stream: Stream<'a, Message>) -> Self {
        Self {
            message_stream,
            parsers: Vec::new(),
            #[cfg(feature = "parsers")]
            default_parsers: None,
            _phantom: PhantomData,
        }
    }

    /// Register a parser function for schemas matching the predicate.
    ///
    /// When iterating over messages, the first parser whose predicate returns
    /// true for the message's schema will be used to parse the data.
    ///
    /// # Arguments
    ///
    /// * `schema_predicate` - Function that returns true if this parser should be used
    /// * `parser` - Function that converts `Bytes` to `T`
    ///
    /// # Examples
    ///
    /// Match by encoding:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let builder = reader.messages()
    ///     .parsed::<String>()
    ///     .parser(
    ///         |schema| schema.encoding == "json",
    ///         |data| String::from_utf8(data)
    ///             .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
    ///     );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Match by schema name:
    /// ```no_run
    /// # use mcapable_core::reader;
    /// # use std::fs::File;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let file = File::open("data.mcap")?;
    /// # let mut reader = reader::Builder::new().build(file)?;
    /// let builder = reader.messages()
    /// # use bytes::Bytes;
    ///     .parsed::<Bytes>()
    ///     .parser(
    ///         |schema| schema.name == "sensor_data",
    ///         |data| Ok(data) // Pass through
    ///     );
    /// # Ok(())
    /// # }
    /// ```
    pub fn parser<F, P>(mut self, schema_predicate: F, parser: P) -> Self
    where
        F: Fn(&Schema) -> bool + 'a,
        P: Fn(Bytes) -> Result<T> + 'a,
    {
        self.parsers.push((
            Box::new(move |_channel, schema| match schema {
                Some(schema) => schema_predicate(schema),
                None => false,
            }),
            Box::new(parser),
        ));
        self
    }

    /// Register a parser function for channels/schemas matching the predicate.
    ///
    /// This can be used to select parsers based on the channel's message encoding
    /// or other metadata.
    pub fn parser_channel<F, P>(mut self, predicate: F, parser: P) -> Self
    where
        F: Fn(&crate::types::Channel, Option<&Schema>) -> bool + 'a,
        P: Fn(Bytes) -> Result<T> + 'a,
    {
        self.parsers.push((Box::new(predicate), Box::new(parser)));
        self
    }

    /// Register a parser function for a specific message encoding.
    pub fn parser_message_encoding<P>(self, encoding: impl Into<String>, parser: P) -> Self
    where
        P: Fn(Bytes) -> Result<T> + 'a,
    {
        let encoding = encoding.into();
        self.parser_channel(
            move |channel, _schema| channel.message_encoding.as_ref() == encoding,
            parser,
        )
    }

    /// Register a parser function for a specific schema encoding.
    pub fn parser_schema_encoding<P>(self, encoding: impl Into<String>, parser: P) -> Self
    where
        P: Fn(Bytes) -> Result<T> + 'a,
    {
        let encoding = encoding.into();
        self.parser_channel(
            move |_channel, schema| {
                schema
                    .map(|schema| schema.encoding.as_ref() == encoding)
                    .unwrap_or(false)
            },
            parser,
        )
    }

    /// Register default parsers for well-known encodings.
    ///
    /// This enables built-in parsing for `json`, `cbor`, and `msgpack` message
    /// encodings and `jsonschema` schemas, and passes through bytes for other
    /// registry-listed schema encodings.
    #[cfg(feature = "parsers")]
    pub fn default_parsers<F, G>(self, json_mapper: F, bytes_mapper: G) -> Self
    where
        F: Fn(serde_json::Value) -> Result<T> + 'a,
        G: Fn(Bytes) -> Result<T> + 'a,
    {
        self.default_parsers_impl(json_mapper, bytes_mapper)
    }

    #[cfg(feature = "parsers")]
    fn default_parsers_impl<F, G>(mut self, json_mapper: F, bytes_mapper: G) -> Self
    where
        F: Fn(serde_json::Value) -> Result<T> + 'a,
        G: Fn(Bytes) -> Result<T> + 'a,
    {
        self.default_parsers = Some(DefaultParsers::new(json_mapper, bytes_mapper));
        self
    }

    /// Build the ParsedStream.
    ///
    /// Returns a ParsedStream that will use the registered parsers to convert
    /// message data to typed values during iteration.
    pub fn build(self) -> ParsedStream<'a, T> {
        ParsedStream {
            message_stream: self.message_stream,
            parsers: self.parsers,
            #[cfg(feature = "parsers")]
            default_parsers: self.default_parsers,
            _phantom: PhantomData,
        }
    }
}
