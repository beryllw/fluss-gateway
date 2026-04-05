pub(crate) mod auth;
mod rest;

use std::sync::Arc;

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
    routing::{get, post, delete, put},
    Router,
};

use crate::backend::FlussBackend;
use crate::config::GatewayConfig;

/// Wait for SIGINT or SIGTERM and return, triggering graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install ctrl_c handler");
        tracing::info!("received shutdown signal (SIGINT)");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
        tracing::info!("received shutdown signal (SIGTERM)");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub struct GatewayServer {
    backend: Arc<FlussBackend>,
    config: GatewayConfig,
}

impl GatewayServer {
    pub async fn new(config: GatewayConfig) -> Result<Self, crate::types::GatewayError> {
        let backend = FlussBackend::new(
            &config.fluss.coordinator,
            config.auth.clone(),
            config.pool.clone(),
        )
        .await?;
        Ok(Self {
            backend: Arc::new(backend),
            config,
        })
    }

    pub async fn run(&self, addr: &str) -> Result<(), crate::types::GatewayError> {
        let app = self.app();

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| crate::types::GatewayError::Internal(e.to_string()))?;

        let shutdown_signal = shutdown_signal();

        tracing::info!("listening on {}", addr);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| crate::types::GatewayError::Internal(e.to_string()))?;

        // Graceful shutdown: close the connection pool
        self.backend.pool().close();
        tracing::info!("server shut down gracefully");

        Ok(())
    }

    fn app(&self) -> Router {
        let shared = AppState {
            backend: self.backend.clone(),
            auth_type: self.config.auth.r#type.clone(),
        };

        let api = Router::new()
            .route("/v1/_databases", get(rest::list_databases))
            .route("/v1/_databases", post(rest::create_database))
            .route("/v1/_databases/{db}", delete(rest::drop_database))
            .route("/v1/{db}/_tables", get(rest::list_tables))
            .route("/v1/{db}/_tables", post(rest::create_table))
            .route("/v1/{db}/_tables/{table}", put(rest::table_info))
            .route("/v1/{db}/_tables/{table}", delete(rest::drop_table))
            .route("/v1/{db}/{table}/_info", get(rest::table_info))
            .route("/v1/{db}/{table}", get(rest::lookup))
            .route("/v1/{db}/{table}/prefix", get(rest::prefix_scan))
            .route("/v1/{db}/{table}/batch", post(rest::batch_lookup))
            .route("/v1/{db}/{table}/scan", post(rest::scan))
            .route("/v1/{db}/{table}/rows", post(rest::produce))
            .route("/v1/{db}/{table}/offsets", post(rest::list_offsets))
            .route("/v1/{db}/{table}/partitions", get(rest::list_partitions))
            .with_state(shared)
            .layer(axum::middleware::from_fn(auth_middleware));

        Router::new()
            .route("/health", get(rest::health))
            .merge(api)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<FlussBackend>,
    pub auth_type: crate::config::AuthType,
}

/// Auth middleware: extracts HTTP Basic Auth credentials and stores them
/// in request extensions for downstream handlers.
async fn auth_middleware(req: Request, next: Next) -> Response {
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(creds) = auth::parse_basic_auth(auth_str) {
                let mut req = req;
                req.extensions_mut().insert(creds);
                return next.run(req).await;
            }
        }
    }
    next.run(req).await
}
