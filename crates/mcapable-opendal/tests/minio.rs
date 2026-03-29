use aws_config::BehaviorVersion;
use aws_config::meta::region::RegionProviderChain;
use aws_credential_types::Credentials;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_sdk_s3::primitives::ByteStream;
use base64::Engine;
use mcapable_core::reader::Builder as ReaderBuilder;
use mcapable_opendal::OpendalBytesSource;
use opendal::Operator;
use opendal::layers::BlockingLayer;
use opendal::services::S3;
use std::time::Duration;
use testcontainers::GenericImage;
use testcontainers::RunnableImage;
use testcontainers::core::WaitFor;
use testcontainers::runners::SyncRunner;
use tokio::time::sleep;

const SAMPLE_BASE64: &str = "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg==";

fn sample_bytes() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(SAMPLE_BASE64)
        .expect("valid base64")
}

#[test]
fn opendal_minio_reader_roundtrip() {
    if std::env::var("MCAPABLE_MINIO_TESTS").is_err() {
        eprintln!("skipping minio system test: set MCAPABLE_MINIO_TESTS=1 to enable");
        return;
    }

    let image = GenericImage::new("minio/minio", "RELEASE.2024-11-07T00-52-20Z")
        .with_env_var("MINIO_ROOT_USER", "minioadmin")
        .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
        .with_exposed_port(9000)
        .with_wait_for(WaitFor::seconds(1));
    let image =
        RunnableImage::from(image).with_args(vec!["server".to_string(), "/data".to_string()]);
    let node = image.start();
    let port = node.get_host_port_ipv4(9000);
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "mcapable-tests";

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let _guard = rt.enter();
    rt.block_on(async {
        let region_provider = RegionProviderChain::first_try("us-east-1");
        let credentials = Credentials::new("minioadmin", "minioadmin", None, None, "minio");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .credentials_provider(credentials)
            .endpoint_url(endpoint.clone())
            .load()
            .await;
        let s3_config = S3ConfigBuilder::from(&config)
            .force_path_style(true)
            .build();
        let client = S3Client::from_conf(s3_config);
        let mut last_err = None;
        for _ in 0..10 {
            match client.create_bucket().bucket(bucket).send().await {
                Ok(_) => {
                    last_err = None;
                    break;
                }
                Err(err) => {
                    last_err = Some(err);
                    sleep(Duration::from_millis(500)).await;
                }
            }
        }
        if let Some(err) = last_err {
            panic!("create bucket: {err}");
        }

        client
            .put_object()
            .bucket(bucket)
            .key("sample.mcap")
            .body(ByteStream::from(sample_bytes()))
            .send()
            .await
            .expect("put object");
    });

    let op = Operator::new(
        S3::default()
            .bucket(bucket)
            .endpoint(&endpoint)
            .region("us-east-1")
            .access_key_id("minioadmin")
            .secret_access_key("minioadmin"),
    )
    .expect("create operator")
    .layer(BlockingLayer::create().expect("blocking layer"))
    .finish()
    .blocking();

    let source = OpendalBytesSource::new(op, "sample.mcap").expect("bytes source");
    let mut reader = ReaderBuilder::new().build(source).expect("build reader");

    let header = reader.header().expect("header");
    assert_eq!(header.profile.as_ref(), "example");

    let stream = reader.messages().expect("message stream");
    let mut count = 0;
    for msg in stream {
        msg.expect("message");
        count += 1;
    }
    assert!(count > 0);
}
