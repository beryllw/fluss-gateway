mod auth;
mod rest;

use std::sync::Arc;

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
    routing::{get, post},
    Router,
};

use crate::backend::FlussBackend;

pub struct GatewayServer {
    backend: Arc<FlussBackend>,
}

impl GatewayServer {
    pub async fn new(coordinator_addr: &str) -> Result<Self, crate::types::GatewayError> {
        let backend = FlussBackend::new(coordinator_addr).await?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    pub async fn new_with_auth(
        coordinator_addr: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, crate::types::GatewayError> {
        let backend = FlussBackend::with_auth(coordinator_addr, username, password).await?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    pub async fn run(&self, addr: &str) -> Result<(), crate::types::GatewayError> {
        let app = self.app();

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| crate::types::GatewayError::Internal(e.to_string()))?;

        tracing::info!(addr = %addr, "listening");
        axum::serve(listener, app)
            .await
            .map_err(|e| crate::types::GatewayError::Internal(e.to_string()))?;

        Ok(())
    }

    fn app(&self) -> Router {
        let api = Router::new()
            .route("/v1/_databases", get(rest::list_databases))
            .route("/v1/{db}/_tables", get(rest::list_tables))
            .route("/v1/{db}/{table}/_info", get(rest::table_info))
            .route("/v1/{db}/{table}", get(rest::lookup))
            .route("/v1/{db}/{table}/prefix", get(rest::prefix_scan))
            .route("/v1/{db}/{table}/batch", post(rest::batch_lookup))
            .route("/v1/{db}/{table}/scan", post(rest::scan))
            .route("/v1/{db}/{table}/rows", post(rest::produce))
            .with_state(self.backend.clone())
            .layer(axum::middleware::from_fn(auth_middleware));

        Router::new()
            .route("/health", get(rest::health))
            .merge(api)
    }
}

/// Auth middleware: extracts HTTP Basic Auth credentials and passes them
/// to the backend. If credentials are present, they are used for SASL/PLAIN
/// authentication with Fluss. If no credentials are present and the backend
/// requires auth, the request will fail at the Fluss level.
async fn auth_middleware(req: Request, next: Next) -> Response {
    // Extract Basic Auth credentials from the HTTP request
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(creds) = auth::parse_basic_auth(auth_str) {
                // Store credentials in request extensions
                // Downstream handlers can use them if needed
                let mut req = req;
                req.extensions_mut().insert(creds);
                return next.run(req).await;
            }
        }
    }
    next.run(req).await
}
