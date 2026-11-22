// SPDX-License-Identifier: MIT

//! The HTTP server: routes, binding and graceful shutdown. The health routes,
//! tapir's plain-text errors and the HTTP edge (CORS, the Allow-less 405) are
//! gluon-rs `service-basics`.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use axum::Router;
use axum::routing::{delete, get, post};
use service_basics::cors::{CorsPolicy, Unrouted};
use service_basics::edge::edge;
use service_basics::health::{self, Readiness};
use service_basics::text_error::{self, TextError, method_not_allowed};
use telemetry::logging::Logger;
use tokio::net::TcpListener;

use crate::store::CartStore;
use crate::{docs, routes};

/// What the handlers share.
#[derive(Clone)]
pub struct AppState<S> {
    /// Where the carts live.
    pub store: S,
    /// The service's JSON log.
    pub logger: Logger,
}

/// Ready when the store (Redis) answers a ping within the client's timeout.
impl<S: CartStore> Readiness for AppState<S> {
    async fn ready(&self) -> bool {
        self.store.ping().await
    }
}

/// The service: its routes behind the Scala services' edge, CORS outermost.
pub fn app<S: CartStore>(state: AppState<S>) -> Router {
    edge(routes(state), CorsPolicy::default())
}

/// The carts routes, health and the docs; tapir's 405 for a known path with
/// another method and 404 for any other.
pub fn routes<S: CartStore>(state: AppState<S>) -> Router {
    health::routes()
        .route("/carts", post(routes::create::<S>))
        .route(
            "/carts/{id}",
            get(routes::get::<S>).delete(routes::delete::<S>),
        )
        .route("/carts/{id}/items", post(routes::add_item::<S>))
        .route("/carts/{id}/items/{sku}", delete(routes::remove_item::<S>))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .with_state(state)
        .merge(docs::router())
}

/// 404 for any unknown path, marked [`Unrouted`] for CORS.
async fn not_found() -> (Unrouted, TextError) {
    (Unrouted, text_error::not_found().await)
}

/// Serves the API on `0.0.0.0:<port>` until `shutdown` completes.
///
/// # Errors
/// When the port can't be bound or serving fails.
pub async fn run<S: CartStore>(
    port: u16,
    state: AppState<S>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown)
        .await
}
