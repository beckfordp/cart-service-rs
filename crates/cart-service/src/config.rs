// SPDX-License-Identifier: MIT

//! The config contract: the Scala service's environment variables and
//! defaults (its `application.conf`). Pure: an environment map in, a
//! [`Config`] or every problem out.

use std::collections::HashMap;
use std::hash::BuildHasher;

pub use service_basics::config::ConfigError;
use service_basics::config::port;
pub use telemetry::config::TracingConfig;
use telemetry::config::tracing_from_env;

/// The HTTP port's variable.
pub const PORT_VARIABLE: &str = "CART_SERVICE_PORT";

/// Everything the service reads from its environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `CART_SERVICE_PORT`: the HTTP API.
    pub port: u16,
    /// `CART_SERVICE_NAME`: `service.name` on spans, the logger name.
    pub service_name: String,
    /// `REDIS_URI`: where the carts live.
    pub redis_uri: String,
    /// `OTEL_*`.
    pub tracing: TracingConfig,
}

/// Reads the config from an environment map.
///
/// # Errors
/// [`ConfigError`] listing every invalid variable.
pub fn from_env<S: BuildHasher>(env: &HashMap<String, String, S>) -> Result<Config, ConfigError> {
    let mut problems = Vec::new();
    let text = |name: &str, default: &str| env.get(name).map_or(default, String::as_str).to_owned();
    let http_port = port(env, PORT_VARIABLE, 8080, &mut problems);
    let tracing = tracing_from_env(env, &mut problems);
    if !problems.is_empty() {
        return Err(ConfigError { problems });
    }
    Ok(Config {
        port: http_port,
        service_name: text("CART_SERVICE_NAME", "cart-service"),
        redis_uri: text("REDIS_URI", "redis://localhost:6379"),
        tracing,
    })
}
