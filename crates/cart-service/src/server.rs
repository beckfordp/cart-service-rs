// SPDX-License-Identifier: MIT

//! The HTTP server: routes, binding and graceful shutdown. The health routes,
//! tapir's plain-text errors and the HTTP edge (CORS, the Allow-less 405) are
//! gluon-rs `service-basics`.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use redis_cache::Cache;
use service_basics::cors::{CorsPolicy, Unrouted};
use service_basics::edge::edge;
use service_basics::health::{self, Readiness};
use service_basics::text_error::{self, TextError, method_not_allowed};
use tokio::net::TcpListener;

/// What the handlers share.
#[derive(Clone)]
pub struct AppState {
    /// The Redis the carts live in.
    pub redis: Arc<Cache>,
}

/// Ready when Redis answers a ping within the client's timeout.
impl Readiness for AppState {
    async fn ready(&self) -> bool {
        self.redis.ping().await
    }
}

/// The service: its routes behind the Scala services' edge, CORS outermost.
pub fn app(redis: Arc<Cache>) -> Router {
    edge(routes(redis), CorsPolicy::default())
}

/// The routes: health, tapir's 405 for a known path with another method and
/// 404 for any other.
pub fn routes(redis: Arc<Cache>) -> Router {
    health::routes()
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .with_state(AppState { redis })
}

/// 404 for any unknown path, marked [`Unrouted`] for CORS.
async fn not_found() -> (Unrouted, TextError) {
    (Unrouted, text_error::not_found().await)
}

/// Serves the API on `0.0.0.0:<port>` until `shutdown` completes.
///
/// # Errors
/// When the port can't be bound or serving fails.
pub async fn run(
    port: u16,
    redis: Arc<Cache>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    axum::serve(listener, app(redis))
        .with_graceful_shutdown(shutdown)
        .await
}
