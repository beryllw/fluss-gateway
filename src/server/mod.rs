mod rest;

use std::sync::Arc;

use axum::{
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
        Router::new()
            .route("/health", get(rest::health))
            .route("/v1/_databases", get(rest::list_databases))
            .route("/v1/{db}/_tables", get(rest::list_tables))
            .route("/v1/{db}/{table}/_info", get(rest::table_info))
            .route("/v1/{db}/{table}", get(rest::lookup))
            .route("/v1/{db}/{table}/prefix", get(rest::prefix_scan))
            .route("/v1/{db}/{table}/batch", post(rest::batch_lookup))
            .route("/v1/{db}/{table}/scan", post(rest::scan))
            .route("/v1/{db}/{table}/rows", post(rest::produce))
            .with_state(self.backend.clone())
    }
}
