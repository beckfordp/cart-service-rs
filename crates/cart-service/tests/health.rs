// SPDX-License-Identifier: MIT

//! The started binary: liveness, the JSON start line, and the exits (SIGTERM
//! ends it 0, bad config 1 listing every problem, a URL Redis can't use 1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use cart_service::config::PORT_VARIABLE;
use support::{Redis, Service};

#[test]
fn health_is_200_with_no_body() {
    let redis = Redis::start();
    let service = Service::on(&redis);
    assert_eq!(service.get("/health").unwrap(), (200, None, String::new()));
}

#[test]
fn the_start_is_a_json_log_line_and_sigterm_exits_0() {
    let redis = Redis::start();
    let mut service = Service::on(&redis);
    service.terminate();
    assert_eq!(service.wait_exit().code(), Some(0));
    let stdout = service.stdout();
    let start = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|line| line["message"] == "cart-service starting")
        .unwrap_or_else(|| panic!("no start line in {stdout}"));
    assert_eq!(start["logger_name"], "cart-service");
    assert_eq!(start["port"], service.port.to_string());
}

#[test]
fn bad_config_exits_1_listing_every_problem() {
    let mut service = Service::spawn(&[
        (PORT_VARIABLE, "http".to_owned()),
        ("OTEL_TRACES_SAMPLER_ARG", "lots".to_owned()),
    ]);
    assert_eq!(service.wait_exit().code(), Some(1));
    let stderr = service.stderr();
    assert!(stderr.contains(PORT_VARIABLE), "{stderr}");
    assert!(stderr.contains("OTEL_TRACES_SAMPLER_ARG"), "{stderr}");
}

#[test]
fn a_url_redis_cant_use_exits_1() {
    let mut service = Service::spawn(&[("REDIS_URI", "http://nope".to_owned())]);
    assert_eq!(service.wait_exit().code(), Some(1));
    assert!(
        service.stderr().contains("not a Redis URL"),
        "{}",
        service.stderr()
    );
}
