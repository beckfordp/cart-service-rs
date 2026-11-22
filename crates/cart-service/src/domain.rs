// SPDX-License-Identifier: MIT

//! The pure core, as the Scala domain: values parsed at the edge, so the
//! rest of the service holds only valid ones (`rust.md` "parse, don't
//! validate").

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;
use uuid::Uuid;

/// The most units one line may hold (`Quantity.Max`).
pub const MAX_QUANTITY: i64 = 999;

/// The longest sku, in UTF-16 units as Java's `String.length` counts.
const MAX_SKU_LENGTH: usize = 128;

/// Reserved for the cart hash's own fields (`__created_at`, `__updated_at`).
const RESERVED_PREFIX: &str = "__";

/// Why a cart operation was refused; the `Display` is the error body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CartError {
    /// No cart has the id (never created, deleted, or expired).
    #[error("Cart not found")]
    NotFound,
    /// Outside 1..999, or an add that would take a line past 999.
    #[error("Invalid quantity")]
    InvalidQuantity,
    /// Blank, over 128 characters, or starting with `__`.
    #[error("Invalid sku")]
    InvalidSku,
}

/// A cart's id: a UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CartId(Uuid);

impl From<Uuid> for CartId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl FromStr for CartId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(text).map(Self)
    }
}

impl fmt::Display for CartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A catalog sku, kept exactly as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sku(String);

impl Sku {
    /// `raw`, when it isn't blank, over-long or reserved.
    ///
    /// # Errors
    /// [`CartError::InvalidSku`].
    pub fn parse(raw: &str) -> Result<Self, CartError> {
        let blank = raw.trim().is_empty();
        let long = raw.encode_utf16().count() > MAX_SKU_LENGTH;
        if blank || long || raw.starts_with(RESERVED_PREFIX) {
            return Err(CartError::InvalidSku);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The sku's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Units to add: 1..999.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity(i64);

impl Quantity {
    /// `raw`, when it is 1..999.
    ///
    /// # Errors
    /// [`CartError::InvalidQuantity`].
    pub fn parse(raw: i64) -> Result<Self, CartError> {
        if (1..=MAX_QUANTITY).contains(&raw) {
            Ok(Self(raw))
        } else {
            Err(CartError::InvalidQuantity)
        }
    }

    /// The count.
    #[must_use]
    pub fn get(self) -> i64 {
        self.0
    }
}

/// A cart: sku → quantity, and when it was made and last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cart {
    /// Its id.
    pub id: CartId,
    /// Its lines.
    pub items: BTreeMap<String, i64>,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it last changed (a read doesn't bump it).
    pub updated_at: OffsetDateTime,
}

impl Cart {
    /// A new cart with no lines, made `now`.
    #[must_use]
    pub fn empty(id: CartId, now: OffsetDateTime) -> Self {
        Self {
            id,
            items: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// The cart with `quantity` more of `sku`, changed `now`.
    ///
    /// # Errors
    /// [`CartError::InvalidQuantity`] when the line would pass 999.
    pub fn add_item(
        mut self,
        sku: &Sku,
        quantity: Quantity,
        now: OffsetDateTime,
    ) -> Result<Self, CartError> {
        let total = self.items.get(sku.as_str()).copied().unwrap_or(0) + quantity.get();
        if total > MAX_QUANTITY {
            return Err(CartError::InvalidQuantity);
        }
        self.items.insert(sku.as_str().to_owned(), total);
        self.updated_at = now;
        Ok(self)
    }

    /// The cart without `sku`'s line, changed `now`.
    #[must_use]
    pub fn remove_item(mut self, sku: &Sku, now: OffsetDateTime) -> Self {
        self.items.remove(sku.as_str());
        self.updated_at = now;
        self
    }
}
