// SPDX-License-Identifier: MIT

//! The cart-service binary: the Rust port of gluon's Scala
//! cart-service (ADR 0029). Generated from `tools/rust-service-template`:
//! the `example` table and `GET /examples/{id}` show the shape and go when
//! the first real route arrives.
pub mod cache;
pub mod clock;
pub mod config;
pub mod db;
pub mod docs;
pub mod domain;
pub mod examples;
pub mod ids;
pub mod server;
pub mod store;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
