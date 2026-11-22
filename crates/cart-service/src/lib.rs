// SPDX-License-Identifier: MIT

//! The cart-service binary: the Rust port of gluon's Scala cart-service
//! (ADR 0029). Generated from `tools/rust-service-template`, cut to the RED
//! skeleton (all-Rust Stage A): health, and readiness following Redis — the
//! store the carts live in. The carts routes and the docs arrive with the port.

pub mod config;
pub mod server;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
