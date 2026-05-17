//! Writers for logging events and spans
//!
//! # Overview
//!
//! This module provides non-blocking writer support for [`tracing`](https://docs.rs/tracing/).
//! Events and spans are recorded in a dedicated logging thread to avoid blocking the application.
//!
//! # Non-Blocking Writer
//!
//! Use [`non_blocking()`] to wrap any [`std::io::Write`] implementation and obtain a
//! [`NonBlocking`] writer plus a [`non_blocking::WorkerGuard`].
//!
//! The guard must be kept alive for the duration of the program; dropping it flushes and shuts
//! down the background thread.
//!
//! ```rust
//! # fn doc() {
//! let (non_blocking, _guard) = zenrc_log::appender::non_blocking(std::io::stdout());
//! # let _ = non_blocking;
//! # }
//! ```
//!

use non_blocking::{NonBlocking, WorkerGuard};

use std::io::Write;

pub mod non_blocking;

// pub mod rolling;
pub mod builder;

mod worker;

pub(crate) mod sync;

/// Convenience function for creating a non-blocking, off-thread writer.
///
/// See the [`non_blocking` module's docs][non_blocking]'s for more details.
///
/// [non_blocking]: mod@non_blocking
///
/// # Examples
///
/// ``` rust
/// # fn docs() {
/// let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
/// let subscriber = tracing_subscriber::fmt().with_writer(non_blocking);
/// tracing::subscriber::with_default(subscriber.finish(), || {
///    tracing::event!(tracing::Level::INFO, "Hello");
/// });
/// # }
/// ```
pub fn non_blocking<T: Write + Send + 'static>(writer: T) -> (NonBlocking, WorkerGuard) {
    NonBlocking::new(writer)
}

#[derive(Debug)]
pub(crate) enum Msg {
    Line(Vec<u8>),
    Shutdown,
}
