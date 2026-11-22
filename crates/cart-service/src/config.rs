// SPDX-License-Identifier: MIT

//! The config contract: the Scala service's environment variables and
//! defaults (its `application.conf`). Pure: an environment map in, a
//! [`Config`] or every problem out.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::time::Duration;

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
    /// `CART_TTL_SECONDS`: how long a cart lives after its last write.
    pub ttl: Duration,
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
    let ttl = match env.get("CART_TTL_SECONDS").map(|raw| raw.parse::<u64>()) {
        None => 86_400,
        Some(Ok(seconds)) if seconds > 0 => seconds,
        Some(_) => {
            problems.push("CART_TTL_SECONDS: not a positive whole number of seconds".to_owned());
            0
        }
    };
    if !problems.is_empty() {
        return Err(ConfigError { problems });
    }
    Ok(Config {
        port: http_port,
        service_name: text("CART_SERVICE_NAME", "cart-service"),
        redis_uri: text("REDIS_URI", "redis://localhost:6379"),
        ttl: Duration::from_secs(ttl),
        tracing,
    })
}
