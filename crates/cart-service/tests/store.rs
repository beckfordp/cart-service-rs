// SPDX-License-Identifier: MIT

//! The Redis store on a throwaway Redis: carts stored as the Scala service
//! stores them (CART-F7, CART-F10), every write atomic, Redis down an error.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use cart_service::domain::{CartError, CartId, Quantity, Sku};
use cart_service::store::{CartStore, RedisCarts, StoreError};
use redis_cache::Cache;
use redis_cache::redis::cmd;
use service_testing::Redis;
use time::macros::datetime;
use uuid::Uuid;

const TTL: Duration = Duration::from_hours(24);

fn store(redis: &Redis) -> (RedisCarts, Arc<Cache>) {
    let cache = Arc::new(Cache::new(&redis.url(), Duration::from_millis(500)).unwrap());
    (RedisCarts::new(cache.clone(), TTL), cache)
}

fn id(n: u128) -> CartId {
    Uuid::from_u128(n).into()
}

fn sku(text: &str) -> Sku {
    Sku::parse(text).unwrap()
}

async fn hash(cache: &Cache, cart: CartId) -> Vec<String> {
    let mut command = cmd("HGETALL");
    command.arg(format!("cart:{cart}"));
    let mut fields: Vec<String> = cache.query(&command).await.unwrap();
    // Pairs sorted by field, to compare.
    let mut pairs: Vec<(String, String)> = Vec::new();
    while let (Some(value), Some(field)) = (fields.pop(), fields.pop()) {
        pairs.push((field, value));
    }
    pairs.sort();
    pairs.into_iter().flat_map(|(f, v)| [f, v]).collect()
}

async fn ttl(cache: &Cache, cart: CartId) -> i64 {
    let mut command = cmd("TTL");
    command.arg(format!("cart:{cart}"));
    cache.query(&command).await.unwrap()
}

#[tokio::test]
async fn a_cart_is_stored_as_the_scala_service_stores_it() {
    let redis = Redis::start();
    let (carts, cache) = store(&redis);
    let now = datetime!(2026-11-22 20:29:06.704453 UTC);
    let created = carts.create(id(1), now).await.unwrap();
    assert!(created.items.is_empty());
    assert_eq!(
        hash(&cache, id(1)).await,
        [
            "__created_at",
            "2026-11-22T20:29:06.704453Z",
            "__updated_at",
            "2026-11-22T20:29:06.704453Z"
        ]
    );
    assert!(ttl(&cache, id(1)).await > 86_000);
    let later = datetime!(2026-11-22 20:30:00 UTC);
    carts
        .add_item(
            id(1),
            &sku("SKU-MUG-001"),
            Quantity::parse(2).unwrap(),
            later,
        )
        .await
        .unwrap();
    assert_eq!(
        hash(&cache, id(1)).await,
        [
            "SKU-MUG-001",
            "2",
            "__created_at",
            "2026-11-22T20:29:06.704453Z",
            "__updated_at",
            "2026-11-22T20:30:00Z",
        ]
    );
    assert_eq!(
        carts.get(id(1)).await.unwrap(),
        Some(
            created
                .add_item(&sku("SKU-MUG-001"), Quantity::parse(2).unwrap(), later)
                .unwrap()
        )
    );
}

#[tokio::test]
async fn every_write_starts_the_ttl_again_and_a_read_does_not() {
    let redis = Redis::start();
    let (carts, cache) = store(&redis);
    let now = datetime!(2026-11-22 12:00:00 UTC);
    carts.create(id(1), now).await.unwrap();
    let mut expire = cmd("EXPIRE");
    expire.arg("cart:1").arg(100);
    let mut shorten = cmd("EXPIRE");
    shorten.arg(format!("cart:{}", id(1))).arg(100);
    let _: i64 = cache.query(&shorten).await.unwrap();
    carts.get(id(1)).await.unwrap();
    assert!(ttl(&cache, id(1)).await <= 100, "a read leaves it");
    carts
        .add_item(id(1), &sku("A"), Quantity::parse(1).unwrap(), now)
        .await
        .unwrap();
    assert!(ttl(&cache, id(1)).await > 86_000);
    let _: i64 = cache.query(&shorten).await.unwrap();
    carts.remove_item(id(1), &sku("A"), now).await.unwrap();
    assert!(ttl(&cache, id(1)).await > 86_000);
    let _ = expire;
}

#[tokio::test]
async fn adds_increment_up_to_999_and_a_missing_cart_is_not_found() {
    let redis = Redis::start();
    let (carts, _) = store(&redis);
    let now = datetime!(2026-11-22 12:00:00 UTC);
    carts.create(id(1), now).await.unwrap();
    let q = |n| Quantity::parse(n).unwrap();
    carts.add_item(id(1), &sku("A"), q(995), now).await.unwrap();
    let cart = carts.add_item(id(1), &sku("A"), q(4), now).await.unwrap();
    assert_eq!(cart.items["A"], 999);
    assert_eq!(
        carts.add_item(id(1), &sku("A"), q(1), now).await,
        Err(StoreError::Refused(CartError::InvalidQuantity))
    );
    assert_eq!(
        carts.get(id(1)).await.unwrap().unwrap().items["A"],
        999,
        "unchanged"
    );
    assert_eq!(
        carts.add_item(id(2), &sku("A"), q(1), now).await,
        Err(StoreError::Refused(CartError::NotFound))
    );
    assert_eq!(
        carts.remove_item(id(2), &sku("A"), now).await,
        Err(StoreError::Refused(CartError::NotFound))
    );
    assert_eq!(carts.get(id(2)).await.unwrap(), None);
    let mut exists = cmd("EXISTS");
    exists.arg(format!("cart:{}", id(2)));
    let (_, cache) = store(&redis);
    assert_eq!(
        cache.query::<i64>(&exists).await.unwrap(),
        0,
        "no cart resurrected"
    );
}

#[tokio::test]
async fn removing_and_deleting() {
    let redis = Redis::start();
    let (carts, _) = store(&redis);
    let now = datetime!(2026-11-22 12:00:00 UTC);
    carts.create(id(1), now).await.unwrap();
    carts
        .add_item(id(1), &sku("A"), Quantity::parse(3).unwrap(), now)
        .await
        .unwrap();
    assert!(
        carts
            .remove_item(id(1), &sku("A"), now)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        carts
            .remove_item(id(1), &sku("B"), now)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(carts.delete(id(1)).await, Ok(()));
    assert_eq!(
        carts.delete(id(1)).await,
        Err(StoreError::Refused(CartError::NotFound))
    );
    assert_eq!(carts.get(id(1)).await.unwrap(), None);
}

#[tokio::test]
async fn a_corrupt_cart_is_an_error_not_a_missing_one() {
    let redis = Redis::start();
    let (carts, cache) = store(&redis);
    for fields in [["__created_at", "yesterday"], ["SKU", "many"]] {
        let mut set = cmd("HSET");
        set.arg(format!("cart:{}", id(9)))
            .arg("__created_at")
            .arg("2026-11-22T12:00:00Z")
            .arg("__updated_at")
            .arg("2026-11-22T12:00:00Z")
            .arg(fields[0])
            .arg(fields[1]);
        let _: i64 = cache.query(&set).await.unwrap();
        assert!(
            matches!(carts.get(id(9)).await, Err(StoreError::Corrupt(_))),
            "{fields:?}"
        );
        let mut del = cmd("DEL");
        del.arg(format!("cart:{}", id(9)));
        let _: i64 = cache.query(&del).await.unwrap();
    }
}

#[tokio::test]
async fn redis_down_is_an_error_and_the_ping_follows_it() {
    let redis = Redis::start();
    let (carts, _) = store(&redis);
    assert!(carts.ping().await);
    redis.stop();
    assert!(!carts.ping().await);
    assert!(matches!(
        carts.get(id(1)).await,
        Err(StoreError::Unavailable(_))
    ));
    let now = datetime!(2026-11-22 12:00:00 UTC);
    assert!(matches!(
        carts.create(id(1), now).await,
        Err(StoreError::Unavailable(_))
    ));
}
