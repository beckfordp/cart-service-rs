// SPDX-License-Identifier: MIT

//! The cart-service binary: the Rust port of gluon's Scala cart-service
//! (ADR 0029). Generated from `tools/rust-service-template`, carts in Redis
//! as the Scala service stores them, its five routes, health, readiness on
//! Redis, and the docs.

pub mod config;
pub mod docs;
pub mod domain;
pub mod routes;
pub mod server;
pub mod store;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
