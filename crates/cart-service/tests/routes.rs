// SPDX-License-Identifier: MIT

//! The routes over stores in memory: the order of judgement (path, body,
//! line, cart), the error bodies, a store failure a 500, and the docs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The stores in memory have nothing to await.
#![allow(clippy::unused_async_trait_impl)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use cart_service::domain::{Cart, CartError, CartId, Quantity, Sku};
use cart_service::server::{AppState, app};
use cart_service::store::{CartStore, StoreError};
use serde_json::{Value, json};
use telemetry::logging::Logger;
use time::OffsetDateTime;
use tower::ServiceExt;

/// Carts in a map.
#[derive(Clone, Default)]
struct Memory(Arc<Mutex<HashMap<String, Cart>>>);

fn refused<T>(error: CartError) -> Result<T, StoreError> {
    Err(StoreError::Refused(error))
}

impl CartStore for Memory {
    async fn create(&self, id: CartId, now: OffsetDateTime) -> Result<Cart, StoreError> {
        let cart = Cart::empty(id, now);
        self.0.lock().unwrap().insert(id.to_string(), cart.clone());
        Ok(cart)
    }

    async fn get(&self, id: CartId) -> Result<Option<Cart>, StoreError> {
        Ok(self.0.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn add_item(
        &self,
        id: CartId,
        sku: &Sku,
        quantity: Quantity,
        now: OffsetDateTime,
    ) -> Result<Cart, StoreError> {
        let mut carts = self.0.lock().unwrap();
        let Some(cart) = carts.get(&id.to_string()).cloned() else {
            return refused(CartError::NotFound);
        };
        let cart = cart.add_item(sku, quantity, now)?;
        carts.insert(id.to_string(), cart.clone());
        Ok(cart)
    }

    async fn remove_item(
        &self,
        id: CartId,
        sku: &Sku,
        now: OffsetDateTime,
    ) -> Result<Cart, StoreError> {
        let mut carts = self.0.lock().unwrap();
        let Some(cart) = carts.get(&id.to_string()).cloned() else {
            return refused(CartError::NotFound);
        };
        let cart = cart.remove_item(sku, now);
        carts.insert(id.to_string(), cart.clone());
        Ok(cart)
    }

    async fn delete(&self, id: CartId) -> Result<(), StoreError> {
        match self.0.lock().unwrap().remove(&id.to_string()) {
            Some(_) => Ok(()),
            None => refused(CartError::NotFound),
        }
    }

    async fn ping(&self) -> bool {
        true
    }
}

/// A store whose Redis is down (or holds a corrupt cart).
#[derive(Clone)]
struct Broken(fn() -> StoreError);

impl CartStore for Broken {
    async fn create(&self, _: CartId, _: OffsetDateTime) -> Result<Cart, StoreError> {
        Err((self.0)())
    }
    async fn get(&self, _: CartId) -> Result<Option<Cart>, StoreError> {
        Err((self.0)())
    }
    async fn add_item(
        &self,
        _: CartId,
        _: &Sku,
        _: Quantity,
        _: OffsetDateTime,
    ) -> Result<Cart, StoreError> {
        Err((self.0)())
    }
    async fn remove_item(&self, _: CartId, _: &Sku, _: OffsetDateTime) -> Result<Cart, StoreError> {
        Err((self.0)())
    }
    async fn delete(&self, _: CartId) -> Result<(), StoreError> {
        Err((self.0)())
    }
    async fn ping(&self) -> bool {
        false
    }
}

fn service<S: CartStore>(store: S) -> Router {
    app(AppState {
        store,
        logger: Logger::stdout("cart-service-test"),
    })
}

async fn call(app: &Router, method: &str, path: &str, body: Option<&str>) -> (StatusCode, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

const MISSING: &str = "/carts/00000000-0000-4000-8000-000000000000";

#[tokio::test]
async fn a_cart_is_created_added_to_removed_from_and_deleted() {
    let app = service(Memory::default());
    let (status, body) = call(&app, "POST", "/carts", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let cart = json(&body);
    assert_eq!(cart["items"], json!({}));
    assert_eq!(cart["createdAt"], cart["updatedAt"]);
    let path = format!("/carts/{}", cart["id"].as_str().unwrap());
    let items = format!("{path}/items");
    let line = r#"{"sku":"SKU-MUG-001","quantity":2}"#;
    assert_eq!(
        json(&call(&app, "POST", &items, Some(line)).await.1)["items"],
        json!({"SKU-MUG-001": 2})
    );
    assert_eq!(
        json(&call(&app, "POST", &items, Some(line)).await.1)["items"],
        json!({"SKU-MUG-001": 4})
    );
    assert_eq!(
        json(&call(&app, "GET", &path, None).await.1)["items"],
        json!({"SKU-MUG-001": 4})
    );
    let (status, body) = call(&app, "DELETE", &format!("{items}/SKU-MUG-001"), None).await;
    assert_eq!(
        (status, json(&body)["items"].clone()),
        (StatusCode::OK, json!({}))
    );
    assert_eq!(
        call(&app, "DELETE", &path, None).await,
        (StatusCode::NO_CONTENT, String::new())
    );
    assert_eq!(
        call(&app, "GET", &path, None).await,
        (
            StatusCode::NOT_FOUND,
            r#"{"error":"Cart not found"}"#.to_owned()
        )
    );
    assert_eq!(
        call(&app, "DELETE", &path, None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_path_then_the_body_then_the_line_then_the_cart_is_judged() {
    let app = service(Memory::default());
    let bad = |message: &str| (StatusCode::BAD_REQUEST, message.to_owned());
    let id = "Invalid value for: path parameter id";
    assert_eq!(call(&app, "GET", "/carts/nope", None).await, bad(id));
    assert_eq!(call(&app, "DELETE", "/carts/nope", None).await, bad(id));
    assert_eq!(
        call(&app, "POST", "/carts/nope/items", Some("{")).await,
        bad(id)
    );
    assert_eq!(
        call(&app, "DELETE", "/carts/nope/items/A", None).await,
        bad(id)
    );
    let sku = "Invalid value for: path parameter sku";
    assert_eq!(
        call(&app, "DELETE", &format!("{MISSING}/items/__x"), None).await,
        bad(sku)
    );
    let (status, body) = call(&app, "POST", &format!("{MISSING}/items"), Some("")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.starts_with("Invalid value for: body ("), "{body}");
    let items = format!("{MISSING}/items");
    let refused = |reason: &str| {
        (
            StatusCode::BAD_REQUEST,
            format!(r#"{{"error":"{reason}"}}"#),
        )
    };
    assert_eq!(
        call(&app, "POST", &items, Some(r#"{"sku":" ","quantity":1}"#)).await,
        refused("Invalid sku")
    );
    assert_eq!(
        call(&app, "POST", &items, Some(r#"{"sku":"A","quantity":0}"#)).await,
        refused("Invalid quantity")
    );
    let (status, _) = call(&app, "POST", &items, Some(r#"{"sku":"A","quantity":1}"#)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a valid line, then the cart");
    assert_eq!(
        call(&app, "DELETE", &format!("{MISSING}/items/A"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_store_failure_is_a_500_never_a_missing_cart() {
    for failure in [
        (|| StoreError::Unavailable("Redis unavailable: refused".to_owned())) as fn() -> StoreError,
        || StoreError::Corrupt("cart:1: quantity of 'A' is not an Int".to_owned()),
    ] {
        let app = service(Broken(failure));
        let line = Some(r#"{"sku":"A","quantity":1}"#);
        for (method, path, body) in [
            ("POST", "/carts".to_owned(), None),
            ("GET", MISSING.to_owned(), None),
            ("DELETE", MISSING.to_owned(), None),
            ("POST", format!("{MISSING}/items"), line),
            ("DELETE", format!("{MISSING}/items/A"), None),
        ] {
            let got = call(&app, method, &path, body).await;
            assert_eq!(
                got,
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_owned()
                ),
                "{method} {path}"
            );
        }
        assert_eq!(
            call(&app, "GET", "/health/ready", None).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

#[tokio::test]
async fn unknown_paths_and_methods_are_tapirs() {
    let app = service(Memory::default());
    assert_eq!(
        call(&app, "GET", "/nope", None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, "PUT", "/carts", None).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn the_docs_describe_the_five_operations_and_no_events() {
    let app = service(Memory::default());
    let (status, yaml) = call(&app, "GET", "/docs/docs.yaml", None).await;
    assert_eq!(status, StatusCode::OK);
    for operation in [
        "postCarts",
        "getCartsId",
        "deleteCartsId",
        "postCartsIdItems",
        "deleteCartsIdItemsSku",
        "getHealthReady",
    ] {
        assert!(yaml.contains(operation), "{operation}");
    }
    assert_eq!(call(&app, "GET", "/docs/", None).await.0, StatusCode::OK);
    let (status, document) = call(&app, "GET", "/docs/asyncapi.json", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&document)["channels"], json!({}));
    let (status, page) = call(&app, "GET", "/docs/asyncapi/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("publishes and consumes nothing"), "{page}");
}
