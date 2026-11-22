// SPDX-License-Identifier: MIT

//! The config contract: defaults, every override, every problem at once.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use cart_service::config::{Config, TracingConfig, from_env};

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn defaults() {
    assert_eq!(
        from_env(&HashMap::new()).unwrap(),
        Config {
            port: 8080,
            service_name: "cart-service".to_owned(),
            redis_uri: "redis://localhost:6379".to_owned(),
            ttl: Duration::from_hours(24),
            tracing: TracingConfig {
                otlp_endpoint: None,
                sampling_ratio: 1.0,
            },
        }
    );
}

#[test]
fn every_variable_overrides_its_default() {
    let config = from_env(&env(&[
        ("CART_SERVICE_PORT", "18080"),
        ("CART_SERVICE_NAME", "cart-service-rs"),
        ("REDIS_URI", "redis://127.0.0.1:16379"),
        ("CART_TTL_SECONDS", "60"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:4318"),
        ("OTEL_TRACES_SAMPLER_ARG", "0.25"),
    ]))
    .unwrap();
    assert_eq!(config.port, 18080);
    assert_eq!(config.service_name, "cart-service-rs");
    assert_eq!(config.redis_uri, "redis://127.0.0.1:16379");
    assert_eq!(config.ttl, Duration::from_secs(60));
    assert_eq!(config.tracing.sampling_ratio, 0.25);
}

#[test]
fn every_problem_is_reported() {
    let error = from_env(&env(&[
        ("CART_SERVICE_PORT", "http"),
        ("OTEL_TRACES_SAMPLER_ARG", "lots"),
        ("CART_TTL_SECONDS", "0"),
    ]))
    .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("CART_SERVICE_PORT"), "{text}");
    assert!(text.contains("OTEL_TRACES_SAMPLER_ARG"), "{text}");
    assert!(text.contains("CART_TTL_SECONDS"), "{text}");
}
