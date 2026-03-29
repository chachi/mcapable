//! Main conformance test suite.
//!
//! This module runs all conformance tests against the mcapable implementation.

// Test helpers need to be accessible to all test modules
#[path = "helpers/mod.rs"]
mod helpers;

// Conformance test modules
mod conformance;
