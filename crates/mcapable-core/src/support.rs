#[cfg(feature = "std")]
mod imp {
    pub use rapidhash::{RapidHashMap as HashMap, RapidHashSet as HashSet};
    pub use std::borrow::Borrow;
    pub use std::error;
    pub use std::fmt;
    pub use std::format;
    pub use std::ops;
    pub use std::result::Result;
    pub use std::str;
    pub use std::string::{String, ToString};
    pub use std::sync::Arc;
    pub use std::vec::Vec;
}

#[cfg(not(feature = "std"))]
mod imp {
    pub use alloc::format;
    pub use alloc::string::{String, ToString};
    pub use alloc::sync::Arc;
    pub use alloc::vec::Vec;
    pub use core::borrow::Borrow;
    pub use core::error;
    pub use core::fmt;
    pub use core::ops;
    pub use core::result::Result;
    pub use core::str;
    pub type HashMap<K, V> = hashbrown::HashMap<K, V, rapidhash::fast::RandomState>;
    pub type HashSet<T> = hashbrown::HashSet<T, rapidhash::fast::RandomState>;
}

pub use imp::*;
