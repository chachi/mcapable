#ifndef MCAPABLE_C_EXAMPLE_COMMON_H
#define MCAPABLE_C_EXAMPLE_COMMON_H

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
  uint8_t* data;
  size_t len;
} SampleBuffer;

static const char* MCAP_SAMPLE_BASE64 =
    "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg==";

static SampleBuffer sample_bytes(void) {
  int table[256];
  for (int i = 0; i < 256; ++i) table[i] = -1;
  for (int i = 'A'; i <= 'Z'; ++i) table[i] = i - 'A';
  for (int i = 'a'; i <= 'z'; ++i) table[i] = i - 'a' + 26;
  for (int i = '0'; i <= '9'; ++i) table[i] = i - '0' + 52;
  table[(int)'+'] = 62;
  table[(int)'/'] = 63;

  size_t input_len = strlen(MCAP_SAMPLE_BASE64);
  uint8_t* out = (uint8_t*)malloc(input_len);
  if (!out) {
    SampleBuffer empty = {0};
    return empty;
  }

  int val = 0;
  int valb = -8;
  size_t out_len = 0;
  for (size_t i = 0; i < input_len; ++i) {
    unsigned char c = (unsigned char)MCAP_SAMPLE_BASE64[i];
    if (c == '=') break;
    int dec = table[c];
    if (dec < 0) continue;
    val = (val << 6) + dec;
    valb += 6;
    if (valb >= 0) {
      out[out_len++] = (uint8_t)((val >> valb) & 0xFF);
      valb -= 8;
    }
  }

  SampleBuffer buf = {out, out_len};
  return buf;
}

static void free_sample_bytes(SampleBuffer buf) {
  if (buf.data) {
    free(buf.data);
  }
}

#endif
