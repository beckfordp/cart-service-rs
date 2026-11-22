// SPDX-License-Identifier: MIT

//! Shared by the integration tests: gluon-rs `service-testing`'s throwaway
//! Redis and this service's binary under test.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::{Deref, DerefMut};

use cart_service::config::PORT_VARIABLE;

// Not every test file uses both.
#[allow(unused_imports)]
pub use service_testing::{Redis, free_port};

/// The cart-service binary, on a fresh port.
pub struct Service(service_testing::Service);

impl Service {
    /// Starts it with exactly `extra` (plus its port); does not wait.
    pub fn spawn(extra: &[(&str, String)]) -> Self {
        Self(service_testing::Service::spawn(
            env!("CARGO_BIN_EXE_cart-service"),
            PORT_VARIABLE,
            extra,
        ))
    }

    /// Starts it and waits until `/health` answers.
    pub fn start(extra: &[(&str, String)]) -> Self {
        Self(service_testing::Service::start(
            env!("CARGO_BIN_EXE_cart-service"),
            PORT_VARIABLE,
            extra,
        ))
    }

    /// Started against `redis`.
    pub fn on(redis: &Redis) -> Self {
        Self::start(&[("REDIS_URI", redis.url())])
    }
}

impl Deref for Service {
    type Target = service_testing::Service;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Service {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
