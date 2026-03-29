use super::super::{ParserFn, ParserPredicate, Stream};
#[cfg(feature = "parsers")]
use super::defaults::DefaultParsers;
use crate::error::Result;
use crate::types::Message;
use std::marker::PhantomData;

/// A stream that parses message data into typed values.
///
/// Created via `Stream<Message>::parsed()` builder. Iterates over messages
/// and automatically parses their data using registered parser functions.
///
/// # Type Parameters
///
/// - `'a` - Lifetime of the borrow from the Reader
/// - `T` - Output type that messages are parsed into
///
/// # Examples
///
/// ```ignore
/// use mcapable_core::reader;
/// use std::fs::File;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct Telemetry {
///     speed: f64,
///     altitude: f64,
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::open("data.mcap")?;
/// let reader = reader::Builder::new().build(file)?;
///
/// let parsed_stream = reader.messages()
///     .filter_channel(|ch| ch.topic == "/telemetry")
///     .parsed::<Telemetry>()
///     .parser(
///         |schema| schema.encoding == "json",
///         |data| serde_json::from_slice(data.as_ref())
///             .map_err(|e| mcapable_core::Error::InvalidRecord(e.to_string()))
///     )
///     .build();
///
/// for telemetry in parsed_stream {
///     let t = telemetry?;
///     println!("Speed: {}, Altitude: {}", t.speed, t.altitude);
/// }
/// # Ok(())
/// # }
/// ```
pub struct ParsedStream<'a, T> {
    /// The underlying message stream.
    pub(super) message_stream: Stream<'a, Message>,
    /// Pairs of (parser predicate, parser function).
    pub(super) parsers: Vec<(ParserPredicate<'a>, ParserFn<'a, T>)>,
    /// Default parser configuration for well-known encodings.
    #[cfg(feature = "parsers")]
    pub(super) default_parsers: Option<DefaultParsers<'a, T>>,
    /// Phantom data for output type.
    pub(super) _phantom: PhantomData<T>,
}

impl<'a, T> Iterator for ParsedStream<'a, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let message = self.message_stream.next()?;
        let message = match message {
            Ok(message) => message,
            Err(err) => return Some(Err(err)),
        };

        let channel = match self.message_stream.reader.get_channel(message.channel_id) {
            Some(channel) => channel,
            None => {
                return Some(Err(crate::error::Error::InvalidRecord(format!(
                    "Channel {} not found",
                    message.channel_id
                ))));
            }
        };

        // Look up schema (if channel has one)
        let schema = if channel.schema_id == 0 {
            None
        } else {
            self.message_stream.reader.get_schema(channel.schema_id)
        };

        // Find matching parser
        let parser = self
            .parsers
            .iter()
            .find(|(pred, _)| pred(&channel, schema.as_ref()))
            .map(|(_, parser)| parser);

        if let Some(parser) = parser {
            return Some(parser(message.data_bytes()));
        }

        #[cfg(feature = "parsers")]
        {
            if let Some(default_parsers) = &mut self.default_parsers {
                return Some(default_parsers.parse(
                    &channel,
                    schema.as_ref(),
                    message.data_bytes(),
                ));
            }
        }

        Some(Err(crate::error::Error::InvalidRecord(format!(
            "No parser registered for channel {}",
            channel.id
        ))))
    }
}
