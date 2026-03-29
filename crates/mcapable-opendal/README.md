## mcapable-opendal

OpenDAL-backed IO helpers for mcapable.

### URI Parsing

```rust
use mcapable_opendal::OpendalBytesSource;
use mcapable_core::reader::Builder as ReaderBuilder;

let source = OpendalBytesSource::from_uri("s3://my-bucket/path/to/file.mcap")?;
let reader = ReaderBuilder::new().build(source)?;
```

Supported schemes: `s3://`, `gcs://`/`gs://`, `http://`, `https://`.
