#pragma once

#include <cstdint>
#include <string>
#include "cxx.h"

#include <vector>

static const char* MCAP_SAMPLE_BASE64 =
    "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg==";

inline rust::Vec<uint8_t> base64_decode(const std::string& input) {
  static const int kDecTable[256] = {
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
      52,53,54,55,56,57,58,59,60,61,-1,-1,-1, 0,-1,-1,
      -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
      15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
      -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
      41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
      -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
  };

  rust::Vec<uint8_t> output;
  int val = 0;
  int valb = -8;
  for (unsigned char c : input) {
    if (c == '=') break;
    int dec = kDecTable[c];
    if (dec < 0) continue;
    val = (val << 6) + dec;
    valb += 6;
    if (valb >= 0) {
      output.push_back(static_cast<uint8_t>((val >> valb) & 0xFF));
      valb -= 8;
    }
  }
  return output;
}

inline rust::Vec<uint8_t> sample_bytes() {
  return base64_decode(MCAP_SAMPLE_BASE64);
}
