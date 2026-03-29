//! Example demonstrating the parsed message stream.
//!
//! Shows how to:
//! - Register parsers for different schema types
//! - Parse JSON messages automatically
//! - Handle multiple encoding formats
//! - Combine parsing with filtering

mod common_bytes;
mod common_file;

use bytes::Bytes;
use mcapable::Error;
use mcapable::reader;
use serde_json::Value as JsonValue;
use std::fs::File;

fn open_reader(path: &str) -> Result<reader::Reader<File>, Error> {
    let file = File::open(path)?;
    reader::Builder::new().build(file)
}

// JSON parser using serde_json
fn parse_json_value(data: Bytes) -> Result<JsonValue, Error> {
    serde_json::from_slice(data.as_ref()).map_err(|e| Error::InvalidRecord(e.to_string()))
}

fn parse_text_value(data: Bytes) -> Result<String, Error> {
    std::str::from_utf8(data.as_ref())
        .map(|text| text.to_owned())
        .map_err(|e| Error::InvalidRecord(e.to_string()))
}

enum ParsedData {
    Json(JsonValue),
    Text(String),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Parsed Message Stream Example ===\n");

    let file = common_file::write_sample_file()?;
    let path = file.path().to_string_lossy().to_string();
    let mut reader = open_reader(&path)?;

    // Example 1: Parse JSON messages
    println!("Example 1: Parsing JSON data");
    println!("=====================================");

    let parsed_stream = reader
        .messages()?
        .filter_channel(|ch| ch.topic == "/example")
        .parsed::<JsonValue>()
        .parser_message_encoding("json", parse_json_value)
        .build();

    for (i, value) in parsed_stream.enumerate().take(10) {
        let data = value?;
        println!("Message {}: {}", i, data);
    }

    // Example 2: Parse multiple formats
    println!("\nExample 2: Multiple parsers for different encodings");
    println!("====================================================");

    let mut reader = open_reader(&path)?;

    let parsed_stream = reader
        .messages()?
        .parsed::<ParsedData>()
        // Parser for JSON message encodings
        .parser_message_encoding("json", |data| parse_json_value(data).map(ParsedData::Json))
        // Parser for text message encodings
        .parser_message_encoding("text", |data| parse_text_value(data).map(ParsedData::Text))
        .parser_message_encoding("utf8", |data| parse_text_value(data).map(ParsedData::Text))
        // Parser for specific schema names
        .parser(
            |schema| schema.name == "log_message",
            |data| parse_text_value(data).map(ParsedData::Text),
        )
        .build();

    for (i, parsed) in parsed_stream.enumerate().take(5) {
        match parsed? {
            ParsedData::Json(value) => println!("Message {}: {}", i, value),
            ParsedData::Text(text) => println!("Message {}: {}", i, text),
        }
    }

    // Example 3: Raw data pass-through
    println!("\nExample 3: Pass-through parser for raw data");
    println!("============================================");

    let mut reader = open_reader(&path)?;

    let parsed_stream = reader
        .messages()?
        .filter_channel(|ch| matches!(ch.id, 1..=3)) // Only specific channels
        .parsed::<Bytes>()
        .parser(
            |_schema| true, // Match all schemas
            Ok,             // Pass through raw bytes
        )
        .build();

    for (i, data) in parsed_stream.enumerate().take(5) {
        let bytes = data?;
        println!("Message {}: {} bytes", i, bytes.len());
    }

    Ok(())
}
