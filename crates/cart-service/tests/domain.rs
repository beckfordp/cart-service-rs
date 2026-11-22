// SPDX-License-Identifier: MIT

//! The pure core, as the Scala domain (`Sku`, `Quantity`, `Cart`, `CartId`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cart_service::domain::{Cart, CartError, CartId, MAX_QUANTITY, Quantity, Sku};
use time::macros::datetime;

#[test]
fn a_sku_is_non_blank_at_most_128_utf16_units_and_never_reserved() {
    assert_eq!(Sku::parse("SKU-MUG-001").unwrap().as_str(), "SKU-MUG-001");
    assert_eq!(
        Sku::parse(" padded ").unwrap().as_str(),
        " padded ",
        "kept as sent"
    );
    for bad in ["", "   ", "__created_at", "__x"] {
        assert_eq!(Sku::parse(bad), Err(CartError::InvalidSku), "{bad:?}");
    }
    assert!(Sku::parse(&"S".repeat(128)).is_ok());
    assert_eq!(Sku::parse(&"S".repeat(129)), Err(CartError::InvalidSku));
    // Java's String.length counts UTF-16 units: 64 emoji are 128.
    assert!(Sku::parse(&"😀".repeat(64)).is_ok());
    assert_eq!(Sku::parse(&"😀".repeat(65)), Err(CartError::InvalidSku));
}

#[test]
fn a_quantity_is_1_to_999() {
    assert_eq!(MAX_QUANTITY, 999);
    assert_eq!(Quantity::parse(1).unwrap().get(), 1);
    assert_eq!(Quantity::parse(999).unwrap().get(), 999);
    for bad in [0, -1, 1000, i64::MAX] {
        assert_eq!(
            Quantity::parse(bad),
            Err(CartError::InvalidQuantity),
            "{bad}"
        );
    }
}

#[test]
fn a_cart_id_is_a_uuid_rendered_lower_case() {
    let id: CartId = "3F2B8C1E-7A4D-4E9B-9C2A-1D5E6F7A8B90".parse().unwrap();
    assert_eq!(id.to_string(), "3f2b8c1e-7a4d-4e9b-9c2a-1d5e6f7a8b90");
    assert!("not-a-uuid".parse::<CartId>().is_err());
}

#[test]
fn a_new_cart_is_empty_and_adding_past_the_limit_is_refused() {
    let now = datetime!(2026-11-22 12:00:00 UTC);
    let later = datetime!(2026-11-22 12:05:00 UTC);
    let cart = Cart::empty(CartId::from(uuid::Uuid::nil()), now);
    assert!(cart.items.is_empty());
    assert_eq!((cart.created_at, cart.updated_at), (now, now));
    let sku = Sku::parse("SKU-MUG-001").unwrap();
    let cart = cart
        .add_item(&sku, Quantity::parse(995).unwrap(), later)
        .unwrap();
    assert_eq!(cart.items["SKU-MUG-001"], 995);
    assert_eq!((cart.created_at, cart.updated_at), (now, later));
    let cart = cart
        .add_item(&sku, Quantity::parse(4).unwrap(), later)
        .unwrap();
    assert_eq!(cart.items["SKU-MUG-001"], 999);
    assert_eq!(
        cart.clone()
            .add_item(&sku, Quantity::parse(1).unwrap(), later),
        Err(CartError::InvalidQuantity)
    );
    let cart = cart.remove_item(&sku, later);
    assert!(cart.items.is_empty());
}

#[test]
fn the_error_bodies_are_the_scala_services() {
    assert_eq!(CartError::NotFound.to_string(), "Cart not found");
    assert_eq!(CartError::InvalidQuantity.to_string(), "Invalid quantity");
    assert_eq!(CartError::InvalidSku.to_string(), "Invalid sku");
}
