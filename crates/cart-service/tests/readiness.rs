// SPDX-License-Identifier: MIT

//! `/health/ready` follows Redis: down, then back; liveness never does.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::thread::sleep;
use std::time::{Duration, Instant};

use support::{Redis, Service};

fn ready_within(service: &Service, seconds: u64, want: u16) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let got = service.get("/health/ready").map(|(status, _, _)| status);
        if got == Some(want) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "not {want} within {seconds} s (last {got:?})"
        );
        sleep(Duration::from_millis(100));
    }
}

#[test]
fn readiness_follows_redis() {
    let redis = Redis::start();
    let service = Service::on(&redis);
    ready_within(&service, 5, 200);
    redis.stop();
    ready_within(&service, 5, 503);
    assert_eq!(
        service.get("/health").unwrap().0,
        200,
        "liveness doesn't depend on Redis"
    );
    redis.start_again();
    ready_within(&service, 10, 200);
}
