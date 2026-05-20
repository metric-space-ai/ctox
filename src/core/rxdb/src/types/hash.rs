//! Hash function trait.
//!
//! Upstream `HashFunction = (input: string) => Promise<string>`.
//! Translated as a `Send + Sync` async-returning trait so plugins can swap
//! the implementation (e.g. Web Crypto vs. our `sha2`-based default).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type HashOutput<'a> = Pin<Box<dyn Future<Output = String> + Send + 'a>>;

pub trait HashFunction: Send + Sync {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a>;
}

/// Shared, cheap-to-clone handle to a hash function.
pub type SharedHashFunction = Arc<dyn HashFunction>;
