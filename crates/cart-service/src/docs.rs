// SPDX-License-Identifier: MIT

//! The service's API description: what tapir generated from the Scala
//! endpoints (capture 20261122 openapi.yaml), stated once here next to the
//! DTO types it derives the schemas from, served with Swagger UI (CART-D1);
//! and an AsyncAPI document with no channels, since the service publishes
//! and consumes nothing (CART-D2; the Scala service serves none, decision A6).

use axum::Router;
use serde_json::{Value, json};
use telemetry::docs::openapi::{Api, Operation};
use telemetry::docs::{asyncapi, swagger};

use crate::routes::{AddItemRequest, CartResponse, ErrorResponse};

const CART_ID: &str = "3f2b8c1e-7a4d-4e9b-9c2a-1d5e6f7a8b90";
const ID: &str = "Cart id (UUID). A malformed id returns 400.";

/// A JSON body: its schema component and example. (Always `Some`: the
/// builder takes `None` for a response without one.)
#[allow(clippy::unnecessary_wraps)]
fn cart() -> Option<(&'static str, Value)> {
    Some((
        "CartResponse",
        json!({
            "id": CART_ID,
            "items": {"SKU-MUG-001": 2},
            "createdAt": "2026-11-01T09:30:00Z",
            "updatedAt": "2026-11-01T09:45:00Z",
        }),
    ))
}

#[allow(clippy::unnecessary_wraps)]
fn error(message: &str) -> Option<(&'static str, Value)> {
    Some(("ErrorResponse", json!({ "error": message })))
}

fn not_found(operation: Operation) -> Operation {
    operation.response(404, "No cart with that id.", error("Cart not found"))
}

fn with_id(operation: Operation) -> Operation {
    operation
        .tag("carts")
        .path_parameter("id", ID, json!(CART_ID))
}

/// The OpenAPI document: the five carts operations, then the two health checks.
#[must_use]
pub fn api() -> Api {
    Api::new("Cart Service", "1.0")
        .schema::<AddItemRequest>()
        .schema::<CartResponse>()
        .schema::<ErrorResponse>()
        .operation(
            Operation::post("/carts")
                .tag("carts")
                .summary("Create a cart")
                .description(
                    "Creates an empty cart with a server-generated UUID and a 24h TTL (refreshed by \
                     every write). Not idempotent: each call is a new cart. No request body.",
                )
                .response(201, "", cart()),
        )
        .operation(not_found(
            with_id(Operation::get("/carts/{id}"))
                .summary("Get a cart")
                .description(
                    "One cart with its line items and timestamps. 404 when no such cart exists \
                     (never created, deleted, or expired by its TTL).",
                )
                .response(200, "", cart()),
        ))
        .operation(not_found(
            with_id(Operation::delete("/carts/{id}"))
                .summary("Delete a cart")
                .description("Hard-deletes the cart and every line. Deleting an already-absent cart → 404.")
                .response(204, "", None),
        ))
        .operation(not_found(
            with_id(Operation::post("/carts/{id}/items"))
                .summary("Add an item")
                .description(
                    "Adds the sku's line, or increments it by `quantity`. The body is validated \
                     before the cart lookup: a blank/over-long/reserved sku or a quantity outside \
                     1..999 → 400, and an add that would take an existing line past 999 → 400, \
                     leaving the cart unchanged. Not idempotent: each call increments. 404 when the \
                     cart doesn't exist.",
                )
                .request_body("AddItemRequest", json!({"sku": "SKU-MUG-001", "quantity": 2}))
                .response(200, "", cart())
                .response(
                    400,
                    "Quantity outside 1..999, or an add that would take the line past 999.",
                    error("Invalid quantity"),
                ),
        ))
        .operation(not_found(
            with_id(Operation::delete("/carts/{id}/items/{sku}"))
                .path_parameter(
                    "sku",
                    "Catalog sku: non-blank, at most 128 chars, never starting with `__` (reserved \
                     for cart metadata).",
                    json!("SKU-MUG-001"),
                )
                .summary("Remove an item")
                .description(
                    "Drops the sku's line entirely, whatever its quantity; a sku not in the cart is a \
                     no-op returning the unchanged cart. The path sku is validated like a body sku \
                     (blank/over-long/reserved `__` prefix is a 400 before the handler). 404 when \
                     the cart doesn't exist.",
                )
                .response(200, "", cart()),
        ))
        .operation(
            Operation::get("/health")
                .tag("health")
                .summary("Liveness check")
                .description("200 while the process is up.")
                .response(200, "", None),
        )
        .operation(
            Operation::get("/health/ready")
                .tag("health")
                .summary("Readiness check")
                .description("200 when Redis answers a ping, else 503.")
                .response(200, "", None)
                .response(503, "Redis did not answer the readiness ping (no body).", None),
        )
}

/// The AsyncAPI document: no channels.
#[must_use]
pub fn asyncapi() -> Value {
    asyncapi::document("Cart Service", "1.0", &[])
}

/// Swagger UI at `/docs/` (`/docs/docs.yaml`), and the AsyncAPI document and page.
pub fn router() -> Router {
    swagger::router(api().to_yaml()).merge(asyncapi::router_with_page(&asyncapi()))
}
