// SPDX-License-Identifier: MIT

//! The carts routes, as the Scala `CartRoutes`: the path parsed first (a
//! malformed id or sku is tapir's plain-text 400), then the body (an
//! undecodable one likewise), then the line's sku and quantity (400 with the
//! JSON reason), and only then the cart (404). Every error the store raises
//! that isn't a refusal is a 500, never a missing cart.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use service_basics::jvm::java_instant;
use service_basics::text_error::TextError;
use std::collections::BTreeMap;
use telemetry::logging::Logger;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::{Cart, CartError, CartId, Quantity, Sku};
use crate::server::AppState;
use crate::store::{CartStore, StoreError};

/// A cart, as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CartResponse {
    /// Cart id (UUID).
    pub id: String,
    /// Line items: sku -> quantity (1..999 per line); empty for a new cart.
    pub items: BTreeMap<String, i64>,
    /// When the cart was created (UTC).
    pub created_at: String,
    /// When the cart last changed (UTC); a read does not bump it.
    pub updated_at: String,
}

impl From<Cart> for CartResponse {
    fn from(cart: Cart) -> Self {
        Self {
            id: cart.id.to_string(),
            items: cart.items,
            created_at: java_instant(cart.created_at),
            updated_at: java_instant(cart.updated_at),
        }
    }
}

/// Error body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ErrorResponse {
    /// Human-readable reason.
    pub error: String,
}

/// One line to add to a cart.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct AddItemRequest {
    /// Catalog sku; non-blank, at most 128 chars, never starting with `__`.
    pub sku: String,
    /// Units to add, 1..999.
    pub quantity: i32,
}

/// Why a route didn't answer its success.
#[derive(Debug)]
pub enum RouteError {
    /// A path segment that doesn't decode (tapir's text 400).
    Path(&'static str),
    /// A body that doesn't decode (tapir's text 400).
    Body(String),
    /// A refusal with its JSON reason.
    Refused(CartError),
    /// Redis failed or held a corrupt cart.
    Store(String),
}

impl From<StoreError> for RouteError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Refused(refused) => Self::Refused(refused),
            other => Self::Store(other.to_string()),
        }
    }
}

impl IntoResponse for RouteError {
    fn into_response(self) -> Response {
        match self {
            Self::Path(name) => text(
                StatusCode::BAD_REQUEST,
                format!("Invalid value for: path parameter {name}"),
            ),
            Self::Body(detail) => text(
                StatusCode::BAD_REQUEST,
                format!("Invalid value for: body ({detail})"),
            ),
            Self::Refused(refused) => {
                let status = match refused {
                    CartError::NotFound => StatusCode::NOT_FOUND,
                    CartError::InvalidQuantity | CartError::InvalidSku => StatusCode::BAD_REQUEST,
                };
                (
                    status,
                    Json(ErrorResponse {
                        error: refused.to_string(),
                    }),
                )
                    .into_response()
            }
            Self::Store(_) => text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_owned(),
            ),
        }
    }
}

fn text(status: StatusCode, message: String) -> Response {
    TextError { status, message }.into_response()
}

/// Now, at the resolution of Java's clock (microseconds), as the Scala
/// service stamps carts.
fn now() -> OffsetDateTime {
    let now = OffsetDateTime::now_utc();
    now.replace_nanosecond(now.nanosecond() / 1_000 * 1_000)
        .unwrap_or(now)
}

fn cart_id(raw: &str) -> Result<CartId, RouteError> {
    raw.parse().map_err(|_| RouteError::Path("id"))
}

fn logged<T>(
    logger: &Logger,
    id: Option<CartId>,
    result: Result<T, RouteError>,
) -> Result<T, RouteError> {
    if let Err(error) = &result {
        let cart = id.map(|id| id.to_string()).unwrap_or_default();
        match error {
            RouteError::Store(reason) => {
                logger.error(&[("cart_id", &cart), ("error", reason)], "Request failed");
            }
            RouteError::Refused(refused) => {
                logger.warn(&[("cart_id", &cart)], &refused.to_string());
            }
            RouteError::Path(_) | RouteError::Body(_) => {}
        }
    }
    result
}

/// `POST /carts`: a new empty cart (201).
///
/// # Errors
/// A store failure (500).
pub async fn create<S: CartStore>(
    State(state): State<AppState<S>>,
) -> Result<(StatusCode, Json<CartResponse>), RouteError> {
    let id = CartId::from(Uuid::new_v4());
    let result = state
        .store
        .create(id, now())
        .await
        .map_err(RouteError::from);
    let cart = logged(&state.logger, Some(id), result)?;
    Ok((StatusCode::CREATED, Json(cart.into())))
}

/// `GET /carts/{id}`.
///
/// # Errors
/// A malformed id (400), no such cart (404), a store failure (500).
pub async fn get<S: CartStore>(
    State(state): State<AppState<S>>,
    Path(raw): Path<String>,
) -> Result<Json<CartResponse>, RouteError> {
    let id = cart_id(&raw)?;
    let result = match state.store.get(id).await {
        Ok(Some(cart)) => Ok(cart),
        Ok(None) => Err(RouteError::Refused(CartError::NotFound)),
        Err(error) => Err(error.into()),
    };
    Ok(Json(logged(&state.logger, Some(id), result)?.into()))
}

/// `DELETE /carts/{id}` (204).
///
/// # Errors
/// A malformed id (400), no such cart (404), a store failure (500).
pub async fn delete<S: CartStore>(
    State(state): State<AppState<S>>,
    Path(raw): Path<String>,
) -> Result<StatusCode, RouteError> {
    let id = cart_id(&raw)?;
    let result = state.store.delete(id).await.map_err(RouteError::from);
    logged(&state.logger, Some(id), result)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /carts/{id}/items`: the body is judged before the cart is looked up.
///
/// # Errors
/// A malformed id or body, an invalid sku or quantity, a line past 999 (400),
/// no such cart (404), a store failure (500).
pub async fn add_item<S: CartStore>(
    State(state): State<AppState<S>>,
    Path(raw): Path<String>,
    body: Bytes,
) -> Result<Json<CartResponse>, RouteError> {
    let id = cart_id(&raw)?;
    let request: AddItemRequest =
        serde_json::from_slice(&body).map_err(|error| RouteError::Body(error.to_string()))?;
    let result = async {
        let sku = Sku::parse(&request.sku)?;
        let quantity = Quantity::parse(i64::from(request.quantity))?;
        Ok(state.store.add_item(id, &sku, quantity, now()).await?)
    }
    .await;
    Ok(Json(logged(&state.logger, Some(id), result)?.into()))
}

/// `DELETE /carts/{id}/items/{sku}`: drops the line; an absent sku answers the
/// cart unchanged.
///
/// # Errors
/// A malformed id or sku (400), no such cart (404), a store failure (500).
pub async fn remove_item<S: CartStore>(
    State(state): State<AppState<S>>,
    Path((raw, sku)): Path<(String, String)>,
) -> Result<Json<CartResponse>, RouteError> {
    let id = cart_id(&raw)?;
    let sku = Sku::parse(&sku).map_err(|_| RouteError::Path("sku"))?;
    let result = state
        .store
        .remove_item(id, &sku, now())
        .await
        .map_err(RouteError::from);
    Ok(Json(logged(&state.logger, Some(id), result)?.into()))
}

impl From<CartError> for RouteError {
    fn from(error: CartError) -> Self {
        Self::Refused(error)
    }
}
