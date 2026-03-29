//! OpenDAL-backed IO adapters for the mcapable project.

mod source;

use opendal::BlockingOperator;
use opendal::Operator;
use opendal::layers::BlockingLayer;
use opendal::services::{Gcs, Http, S3};
use url::Url;

pub use source::OpendalBytesSource;

/// Error returned by opendal helpers in this crate.
#[derive(Debug)]
pub enum Error {
    /// The provided URI is invalid or missing required parts.
    InvalidUri(String),
    /// The URI scheme is not supported by these helpers.
    UnsupportedScheme(String),
    /// The URI is missing a host/bucket component.
    MissingHost,
    /// The URI is missing an object path.
    MissingPath,
    /// An underlying OpenDAL error.
    Opendal(Box<opendal::Error>),
    /// URL parsing failed.
    Url(url::ParseError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidUri(message) => write!(f, "invalid uri: {message}"),
            Error::UnsupportedScheme(scheme) => write!(f, "unsupported uri scheme: {scheme}"),
            Error::MissingHost => write!(f, "missing host/bucket in uri"),
            Error::MissingPath => write!(f, "missing object path in uri"),
            Error::Opendal(err) => write!(f, "{err}"),
            Error::Url(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<opendal::Error> for Error {
    fn from(err: opendal::Error) -> Self {
        Error::Opendal(Box::new(err))
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error::Url(err)
    }
}

/// Result type for opendal helpers in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Parsed URI containing the operator and object path.
#[derive(Debug)]
pub struct ParsedUri {
    /// The operator configured for the URI scheme.
    pub operator: Operator,
    /// The object path relative to the operator root.
    pub path: String,
}

/// Parsed URI containing a blocking operator and object path.
#[derive(Debug)]
pub struct ParsedBlockingUri {
    /// The blocking operator configured for the URI scheme.
    pub operator: BlockingOperator,
    /// The object path relative to the operator root.
    pub path: String,
}

/// Configuration for building an S3 operator.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// Optional endpoint (useful for MinIO or custom endpoints).
    pub endpoint: Option<String>,
    /// Optional region.
    pub region: Option<String>,
    /// Optional access key id.
    pub access_key_id: Option<String>,
    /// Optional secret access key.
    pub secret_access_key: Option<String>,
    /// Optional root prefix.
    pub root: Option<String>,
}

impl S3Config {
    /// Create a new configuration for the given bucket.
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            endpoint: None,
            region: None,
            access_key_id: None,
            secret_access_key: None,
            root: None,
        }
    }
}

/// Configuration for building a GCS operator.
#[derive(Debug, Clone)]
pub struct GcsConfig {
    /// GCS bucket name.
    pub bucket: String,
    /// Optional endpoint override.
    pub endpoint: Option<String>,
    /// Optional root prefix.
    pub root: Option<String>,
    /// Optional base64-encoded credentials JSON.
    pub credential: Option<String>,
    /// Optional credentials file path.
    pub credential_path: Option<String>,
    /// Optional service account.
    pub service_account: Option<String>,
    /// Optional OAuth scope override.
    pub scope: Option<String>,
}

impl GcsConfig {
    /// Create a new configuration for the given bucket.
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            endpoint: None,
            root: None,
            credential: None,
            credential_path: None,
            service_account: None,
            scope: None,
        }
    }
}

/// Configuration for building an HTTP operator.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// HTTP endpoint such as `https://example.com`.
    pub endpoint: String,
    /// Optional root prefix.
    pub root: Option<String>,
    /// Optional username for basic auth.
    pub username: Option<String>,
    /// Optional password for basic auth.
    pub password: Option<String>,
    /// Optional bearer token for auth.
    pub token: Option<String>,
}

impl HttpConfig {
    /// Create a new HTTP config for the given endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            root: None,
            username: None,
            password: None,
            token: None,
        }
    }
}

/// Build an OpenDAL operator for S3.
pub fn s3_operator(config: &S3Config) -> Result<Operator> {
    let mut builder = S3::default().bucket(&config.bucket);
    if let Some(value) = &config.endpoint {
        builder = builder.endpoint(value);
    }
    if let Some(value) = &config.region {
        builder = builder.region(value);
    }
    if let Some(value) = &config.access_key_id {
        builder = builder.access_key_id(value);
    }
    if let Some(value) = &config.secret_access_key {
        builder = builder.secret_access_key(value);
    }
    if let Some(value) = &config.root {
        builder = builder.root(value);
    }
    Ok(Operator::new(builder)?.finish())
}

/// Build a blocking OpenDAL operator for S3.
pub fn s3_blocking_operator(config: &S3Config) -> Result<BlockingOperator> {
    blocking_operator(s3_operator(config)?)
}

/// Build an OpenDAL operator for GCS.
pub fn gcs_operator(config: &GcsConfig) -> Result<Operator> {
    let mut builder = Gcs::default().bucket(&config.bucket);
    if let Some(value) = &config.endpoint {
        builder = builder.endpoint(value);
    }
    if let Some(value) = &config.root {
        builder = builder.root(value);
    }
    if let Some(value) = &config.credential {
        builder = builder.credential(value);
    }
    if let Some(value) = &config.credential_path {
        builder = builder.credential_path(value);
    }
    if let Some(value) = &config.service_account {
        builder = builder.service_account(value);
    }
    if let Some(value) = &config.scope {
        builder = builder.scope(value);
    }
    Ok(Operator::new(builder)?.finish())
}

/// Build a blocking OpenDAL operator for GCS.
pub fn gcs_blocking_operator(config: &GcsConfig) -> Result<BlockingOperator> {
    blocking_operator(gcs_operator(config)?)
}

/// Build an OpenDAL operator for HTTP (byte-range read support).
pub fn http_operator(config: &HttpConfig) -> Result<Operator> {
    let mut builder = Http::default().endpoint(&config.endpoint);
    if let Some(value) = &config.root {
        builder = builder.root(value);
    }
    if let Some(value) = &config.username {
        builder = builder.username(value);
    }
    if let Some(value) = &config.password {
        builder = builder.password(value);
    }
    if let Some(value) = &config.token {
        builder = builder.token(value);
    }
    Ok(Operator::new(builder)?.finish())
}

/// Build a blocking OpenDAL operator for HTTP (byte-range read support).
pub fn http_blocking_operator(config: &HttpConfig) -> Result<BlockingOperator> {
    blocking_operator(http_operator(config)?)
}

/// Convert an operator into a blocking operator using the blocking layer.
pub fn blocking_operator(operator: Operator) -> Result<BlockingOperator> {
    let operator = operator.layer(BlockingLayer::create()?);
    Ok(operator.blocking())
}

/// Parse a URI into an operator and object path.
///
/// Supported schemes: `s3`, `gcs`, `gs`, `http`, `https`.
pub fn parse_uri(uri: &str) -> Result<ParsedUri> {
    let url = Url::parse(uri)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::InvalidUri(
            "query/fragment components are not supported".to_string(),
        ));
    }
    let scheme = url.scheme();
    match scheme {
        "s3" => {
            let bucket = url.host_str().ok_or(Error::MissingHost)?;
            let path = url.path().trim_start_matches('/').to_string();
            if path.is_empty() {
                return Err(Error::MissingPath);
            }
            let operator = s3_operator(&S3Config::new(bucket))?;
            Ok(ParsedUri { operator, path })
        }
        "gcs" | "gs" => {
            let bucket = url.host_str().ok_or(Error::MissingHost)?;
            let path = url.path().trim_start_matches('/').to_string();
            if path.is_empty() {
                return Err(Error::MissingPath);
            }
            let operator = gcs_operator(&GcsConfig::new(bucket))?;
            Ok(ParsedUri { operator, path })
        }
        "http" | "https" => {
            let host = url.host_str().ok_or(Error::MissingHost)?;
            let mut endpoint = format!("{}://{}", scheme, host);
            if let Some(port) = url.port() {
                endpoint.push_str(&format!(":{port}"));
            }
            let path = url.path().trim_start_matches('/').to_string();
            if path.is_empty() {
                return Err(Error::MissingPath);
            }
            let operator = http_operator(&HttpConfig::new(endpoint))?;
            Ok(ParsedUri { operator, path })
        }
        _ => Err(Error::UnsupportedScheme(scheme.to_string())),
    }
}

/// Parse a URI into a blocking operator and object path.
///
/// Supported schemes: `s3`, `gcs`, `gs`, `http`, `https`.
pub fn parse_uri_blocking(uri: &str) -> Result<ParsedBlockingUri> {
    let parsed = parse_uri(uri)?;
    let operator = blocking_operator(parsed.operator)?;
    Ok(ParsedBlockingUri {
        operator,
        path: parsed.path,
    })
}
