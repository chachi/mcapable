use bytes::Bytes;
use mcapable_core::source::{ArenaBytesSource, BytesCursor, BytesSource};
use mcapable_opendal::OpendalBytesSource;
use opendal::{Operator, Scheme};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSpec {
    Stdin,
    HttpUrl(String),
    Path(PathBuf),
    OpendalUrl {
        scheme: Scheme,
        cfg: Vec<(String, String)>,
        path: String,
    },
    OtherUrlScheme(String),
}

impl InputSpec {
    pub fn parse(s: &str) -> Self {
        if s == "-" {
            return Self::Stdin;
        }
        if let Some((scheme, _rest)) = s.split_once("://") {
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                return Self::HttpUrl(s.to_string());
            }
            if scheme.eq_ignore_ascii_case("file") {
                let path = s.trim_start_matches("file://");
                return Self::Path(PathBuf::from(path));
            }
            if scheme.eq_ignore_ascii_case("s3") {
                return parse_s3_url(s).unwrap_or_else(|| Self::OtherUrlScheme(s.to_string()));
            }
            if scheme.eq_ignore_ascii_case("gs") || scheme.eq_ignore_ascii_case("gcs") {
                return parse_gcs_url(s).unwrap_or_else(|| Self::OtherUrlScheme(s.to_string()));
            }
            if scheme.eq_ignore_ascii_case("azblob") {
                return parse_azblob_url(s).unwrap_or_else(|| Self::OtherUrlScheme(s.to_string()));
            }
            return Self::OtherUrlScheme(s.to_string());
        }
        Self::Path(PathBuf::from(s))
    }
}

pub fn open_source(spec: &InputSpec) -> std::io::Result<Box<dyn BytesSource>> {
    match spec {
        InputSpec::Stdin => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(Box::new(BytesCursor::new(Bytes::from(buf))))
        }
        InputSpec::HttpUrl(url) => {
            let response = ureq::get(url)
                .call()
                .map_err(|e| std::io::Error::other(format!("http request failed: {e}")))?;
            let mut reader = response.into_reader();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            Ok(Box::new(BytesCursor::new(Bytes::from(buf))))
        }
        InputSpec::Path(path) => open_file_source(path),
        InputSpec::OpendalUrl { scheme, cfg, path } => {
            let op = Operator::via_iter(*scheme, cfg.clone())
                .map_err(|e| std::io::Error::other(format!("opendal init failed: {e}")))?;
            Ok(Box::new(OpendalBytesSource::new(
                op.blocking(),
                path.clone(),
            )?))
        }
        InputSpec::OtherUrlScheme(url) => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("unsupported URL scheme: {url}"),
        )),
    }
}

fn open_file_source(path: &Path) -> std::io::Result<Box<dyn BytesSource>> {
    let file = File::open(path)?;
    Ok(Box::new(ArenaBytesSource::new(file)))
}

fn parse_s3_url(url: &str) -> Option<InputSpec> {
    let rest = url.strip_prefix("s3://")?;
    let (bucket, key) = rest.split_once('/')?;
    if bucket.is_empty() || key.is_empty() {
        return None;
    }

    let cfg = vec![
        ("bucket".to_string(), bucket.to_string()),
        ("region".to_string(), "auto".to_string()),
        ("root".to_string(), "/".to_string()),
    ];

    Some(InputSpec::OpendalUrl {
        scheme: Scheme::S3,
        cfg,
        path: key.to_string(),
    })
}

fn parse_gcs_url(url: &str) -> Option<InputSpec> {
    let rest = url
        .strip_prefix("gs://")
        .or_else(|| url.strip_prefix("gcs://"))?;
    let (bucket, key) = rest.split_once('/')?;
    if bucket.is_empty() || key.is_empty() {
        return None;
    }

    let cfg = vec![
        ("bucket".to_string(), bucket.to_string()),
        ("root".to_string(), "/".to_string()),
    ];

    Some(InputSpec::OpendalUrl {
        scheme: Scheme::Gcs,
        cfg,
        path: key.to_string(),
    })
}

fn parse_azblob_url(url: &str) -> Option<InputSpec> {
    let rest = url.strip_prefix("azblob://")?;
    let (container, path) = rest.split_once('/')?;
    if container.is_empty() || path.is_empty() {
        return None;
    }

    let cfg = vec![
        ("container".to_string(), container.to_string()),
        ("root".to_string(), "/".to_string()),
    ];

    Some(InputSpec::OpendalUrl {
        scheme: Scheme::Azblob,
        cfg,
        path: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::SeekFrom;

    #[test]
    fn parse_input_spec_handles_stdin() {
        assert_eq!(InputSpec::parse("-"), InputSpec::Stdin);
    }

    #[test]
    fn parse_input_spec_classifies_http_urls() {
        assert_eq!(
            InputSpec::parse("https://example.com/file.mcap"),
            InputSpec::HttpUrl("https://example.com/file.mcap".to_string())
        );
        assert_eq!(
            InputSpec::parse("http://example.com/file.mcap"),
            InputSpec::HttpUrl("http://example.com/file.mcap".to_string())
        );
    }

    #[test]
    fn parse_input_spec_classifies_unknown_url_schemes() {
        assert_eq!(
            InputSpec::parse("s3://bucket/key"),
            InputSpec::OpendalUrl {
                scheme: Scheme::S3,
                cfg: vec![
                    ("bucket".to_string(), "bucket".to_string()),
                    ("region".to_string(), "auto".to_string()),
                    ("root".to_string(), "/".to_string())
                ],
                path: "key".to_string()
            }
        );
    }

    #[test]
    fn parse_input_spec_classifies_gcs_urls() {
        assert_eq!(
            InputSpec::parse("gs://bucket/key"),
            InputSpec::OpendalUrl {
                scheme: Scheme::Gcs,
                cfg: vec![
                    ("bucket".to_string(), "bucket".to_string()),
                    ("root".to_string(), "/".to_string())
                ],
                path: "key".to_string()
            }
        );
        assert_eq!(
            InputSpec::parse("gcs://bucket/key"),
            InputSpec::OpendalUrl {
                scheme: Scheme::Gcs,
                cfg: vec![
                    ("bucket".to_string(), "bucket".to_string()),
                    ("root".to_string(), "/".to_string())
                ],
                path: "key".to_string()
            }
        );
    }

    #[test]
    fn parse_input_spec_classifies_azblob_urls() {
        assert_eq!(
            InputSpec::parse("azblob://container/path/to/file.mcap"),
            InputSpec::OpendalUrl {
                scheme: Scheme::Azblob,
                cfg: vec![
                    ("container".to_string(), "container".to_string()),
                    ("root".to_string(), "/".to_string())
                ],
                path: "path/to/file.mcap".to_string()
            }
        );
    }

    #[test]
    fn open_source_reads_from_files() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"abcdef").unwrap();

        let spec = InputSpec::Path(tmp.path().to_path_buf());
        let mut source = open_source(&spec).unwrap();

        let out = source.read_exact_bytes(3).unwrap();
        assert_eq!(out.as_ref(), b"abc");

        source.seek(SeekFrom::Start(4)).unwrap();
        let out = source.read_exact_bytes(2).unwrap();
        assert_eq!(out.as_ref(), b"ef");
    }

    #[test]
    fn open_source_rejects_unknown_url_schemes() {
        let spec = InputSpec::OtherUrlScheme("s3://bucket/key".to_string());
        let err = open_source(&spec).err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
    }
}
