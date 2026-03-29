package mcapable

import (
	"encoding/base64"
	"testing"
)

const sampleBase64 = "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg=="

func sampleBytes(t *testing.T) []byte {
	data, err := base64.StdEncoding.DecodeString(sampleBase64)
	if err != nil {
		t.Fatalf("decode sample base64: %v", err)
	}
	return data
}

func TestReaderAndParsedStream(t *testing.T) {
	reader, err := NewReaderFromBytes(sampleBytes(t))
	if err != nil {
		t.Fatalf("reader: %v", err)
	}
	defer reader.Close()

	header, err := reader.Header()
	if err != nil {
		t.Fatalf("header: %v", err)
	}
	if header.Profile != "example" {
		t.Fatalf("unexpected profile %q", header.Profile)
	}
	if header.Library != "mcapable" {
		t.Fatalf("unexpected library %q", header.Library)
	}

	stream, err := reader.Messages()
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	msg, err := stream.Next()
	if err != nil {
		t.Fatalf("next message: %v", err)
	}
	if msg == nil {
		t.Fatal("expected first message")
	}
	if len(msg.Data) == 0 {
		t.Fatal("expected message payload")
	}

	reader, err = stream.IntoReader()
	if err != nil {
		t.Fatalf("into reader: %v", err)
	}
	defer reader.Close()

	stream, err = reader.Messages()
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	parsedStream, err := stream.Parsed()
	if err != nil {
		t.Fatalf("parsed: %v", err)
	}
	parsed, err := parsedStream.Next()
	if err != nil {
		t.Fatalf("parsed next: %v", err)
	}
	if parsed == nil {
		t.Fatal("expected parsed message")
	}
	if parsed.IsJSON {
		if _, ok := parsed.JSON.(map[string]any); !ok {
			t.Fatalf("expected map JSON, got %T", parsed.JSON)
		}
	} else if len(parsed.Bytes) == 0 {
		t.Fatal("expected parsed bytes")
	}
}
