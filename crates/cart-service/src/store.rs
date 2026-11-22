// SPDX-License-Identifier: MIT

//! Where the carts live: Redis, as the Scala `CartStore` keeps them — one
//! hash per cart at `cart:<id>`, its lines as sku → quantity beside
//! `__created_at`/`__updated_at` (Java instants), with a TTL every write
//! starts again. Every write is one Lua script (the Scala service's), so
//! Redis runs it atomically: no partial write, no TTL-less key, no cart
//! resurrected mid-operation. Either implementation reads the other's carts
//! (CART-F10). Handlers reach it through [`CartStore`], so routes are
//! testable over a store in memory.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use redis_cache::redis::cmd;
use redis_cache::{Cache, CacheError};
use service_basics::jvm::java_instant;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::domain::{Cart, CartError, CartId, MAX_QUANTITY, Quantity, Sku};

const CREATED_AT: &str = "__created_at";
const UPDATED_AT: &str = "__updated_at";

/// Why a store call didn't give a cart.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum StoreError {
    /// The cart doesn't exist, or the line would pass 999.
    #[error(transparent)]
    Refused(#[from] CartError),
    /// Redis didn't answer (a 500, never a missing cart).
    #[error("{0}")]
    Unavailable(String),
    /// A stored cart that doesn't parse (a 500, never a missing cart).
    #[error("cart is corrupt in Redis: {0}")]
    Corrupt(String),
}

impl From<CacheError> for StoreError {
    fn from(error: CacheError) -> Self {
        Self::Unavailable(error.to_string())
    }
}

/// What the routes need from the carts.
pub trait CartStore: Clone + Send + Sync + 'static {
    /// Stores a new empty cart `id`, made `now`.
    fn create(
        &self,
        id: CartId,
        now: OffsetDateTime,
    ) -> impl Future<Output = Result<Cart, StoreError>> + Send;

    /// The cart, if it exists.
    fn get(&self, id: CartId) -> impl Future<Output = Result<Option<Cart>, StoreError>> + Send;

    /// Adds `quantity` of `sku` (refused past 999 or for a missing cart).
    fn add_item(
        &self,
        id: CartId,
        sku: &Sku,
        quantity: Quantity,
        now: OffsetDateTime,
    ) -> impl Future<Output = Result<Cart, StoreError>> + Send;

    /// Drops `sku`'s line (refused for a missing cart).
    fn remove_item(
        &self,
        id: CartId,
        sku: &Sku,
        now: OffsetDateTime,
    ) -> impl Future<Output = Result<Cart, StoreError>> + Send;

    /// Deletes the cart (refused when it was already absent).
    fn delete(&self, id: CartId) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Whether the store answers (readiness).
    fn ping(&self) -> impl Future<Output = bool> + Send;
}

/// The carts in Redis.
#[derive(Debug, Clone)]
pub struct RedisCarts {
    redis: Arc<Cache>,
    ttl_seconds: u64,
}

/// Creates the hash and its TTL together. KEYS[1] = the cart; ARGV[1],
/// ARGV[2] = the timestamp fields, ARGV[3] = now, ARGV[4] = TTL seconds.
const CREATE: &str = "redis.call('HSET', KEYS[1], ARGV[1], ARGV[3], ARGV[2], ARGV[3])
redis.call('EXPIRE', KEYS[1], ARGV[4])
return {'ok'}";

/// A write to an existing cart, as the Scala `mutationScript`: `{'missing'}`,
/// `{'limit'}` or `{'ok', field1, value1, ...}` (the hash after the write).
/// ARGV[1] = the updatedAt field, ARGV[2] = now, ARGV[3] = TTL seconds, then
/// the write's own arguments.
fn mutation(write: &str) -> String {
    format!(
        "if redis.call('EXISTS', KEYS[1]) == 0 then return {{'missing'}} end
{write}
redis.call('HSET', KEYS[1], ARGV[1], ARGV[2])
redis.call('EXPIRE', KEYS[1], ARGV[3])
local fields = redis.call('HGETALL', KEYS[1])
table.insert(fields, 1, 'ok')
return fields"
    )
}

/// ARGV[4] = sku, ARGV[5] = units to add, ARGV[6] = the line limit (999).
const ADD_ITEM: &str =
    "local total = tonumber(redis.call('HGET', KEYS[1], ARGV[4]) or '0') + tonumber(ARGV[5])
if total > tonumber(ARGV[6]) then return {'limit'} end
redis.call('HSET', KEYS[1], ARGV[4], tostring(total))";

/// ARGV[4] = sku.
const REMOVE_ITEM: &str = "redis.call('HDEL', KEYS[1], ARGV[4])";

fn key(id: CartId) -> String {
    format!("cart:{id}")
}

impl RedisCarts {
    /// Over `redis`, each cart living `ttl` after its last write.
    #[must_use]
    pub fn new(redis: Arc<Cache>, ttl: Duration) -> Self {
        Self {
            redis,
            ttl_seconds: ttl.as_secs().max(1),
        }
    }

    async fn mutate(
        &self,
        id: CartId,
        now: OffsetDateTime,
        write: &str,
        args: &[String],
    ) -> Result<Cart, StoreError> {
        let mut command = cmd("EVAL");
        command
            .arg(mutation(write))
            .arg(1)
            .arg(key(id))
            .arg(UPDATED_AT)
            .arg(java_instant(now))
            .arg(self.ttl_seconds)
            .arg(args);
        let reply: Vec<String> = self.redis.query(&command).await?;
        match reply.split_first() {
            Some((tag, fields)) if tag == "ok" => parse(id, fields),
            Some((tag, [])) if tag == "missing" => Err(CartError::NotFound.into()),
            Some((tag, [])) if tag == "limit" => Err(CartError::InvalidQuantity.into()),
            _ => Err(StoreError::Corrupt(format!(
                "unexpected script reply for {id}: {reply:?}"
            ))),
        }
    }
}

/// A cart from its hash's fields (`HGETALL`'s flat field, value list).
fn parse(id: CartId, flat: &[String]) -> Result<Cart, StoreError> {
    if !flat.len().is_multiple_of(2) {
        return Err(StoreError::Corrupt(format!("odd HGETALL reply for {id}")));
    }
    let mut created_at = None;
    let mut updated_at = None;
    let mut items = BTreeMap::new();
    for [field, value] in flat.as_chunks::<2>().0 {
        match field.as_str() {
            CREATED_AT => created_at = Some(instant(id, field, value)?),
            UPDATED_AT => updated_at = Some(instant(id, field, value)?),
            sku => {
                let quantity = value.parse::<i32>().map_err(|_| {
                    StoreError::Corrupt(format!(
                        "{id}: quantity of '{sku}' is not an Int: '{value}'"
                    ))
                })?;
                items.insert(sku.to_owned(), i64::from(quantity));
            }
        }
    }
    let missing = |name: &str| StoreError::Corrupt(format!("{id}: missing {name}"));
    Ok(Cart {
        id,
        items,
        created_at: created_at.ok_or_else(|| missing(CREATED_AT))?,
        updated_at: updated_at.ok_or_else(|| missing(UPDATED_AT))?,
    })
}

fn instant(id: CartId, field: &str, value: &str) -> Result<OffsetDateTime, StoreError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| StoreError::Corrupt(format!("{id}: {field} is not an instant: '{value}'")))
}

impl CartStore for RedisCarts {
    async fn create(&self, id: CartId, now: OffsetDateTime) -> Result<Cart, StoreError> {
        let mut command = cmd("EVAL");
        command
            .arg(CREATE)
            .arg(1)
            .arg(key(id))
            .arg(CREATED_AT)
            .arg(UPDATED_AT)
            .arg(java_instant(now))
            .arg(self.ttl_seconds);
        let _: Vec<String> = self.redis.query(&command).await?;
        Ok(Cart::empty(id, now))
    }

    async fn get(&self, id: CartId) -> Result<Option<Cart>, StoreError> {
        let mut command = cmd("HGETALL");
        command.arg(key(id));
        let fields: Vec<String> = self.redis.query(&command).await?;
        if fields.is_empty() {
            return Ok(None);
        }
        parse(id, &fields).map(Some)
    }

    async fn add_item(
        &self,
        id: CartId,
        sku: &Sku,
        quantity: Quantity,
        now: OffsetDateTime,
    ) -> Result<Cart, StoreError> {
        let args = [
            sku.as_str().to_owned(),
            quantity.get().to_string(),
            MAX_QUANTITY.to_string(),
        ];
        self.mutate(id, now, ADD_ITEM, &args).await
    }

    async fn remove_item(
        &self,
        id: CartId,
        sku: &Sku,
        now: OffsetDateTime,
    ) -> Result<Cart, StoreError> {
        self.mutate(id, now, REMOVE_ITEM, &[sku.as_str().to_owned()])
            .await
    }

    async fn delete(&self, id: CartId) -> Result<(), StoreError> {
        let mut command = cmd("DEL");
        command.arg(key(id));
        let removed: i64 = self.redis.query(&command).await?;
        if removed > 0 {
            Ok(())
        } else {
            Err(CartError::NotFound.into())
        }
    }

    async fn ping(&self) -> bool {
        self.redis.ping().await
    }
}
