pub(crate) mod auth;
mod rest;

use std::sync::Arc;

use axum::{
    extract::Request,
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post, delete, put},
    Json, Router,
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
        let max_body_size = self.config.server.max_body_size;

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
            .layer(axum::middleware::from_fn(move |req, next| {
                body_limit_middleware(req, next, max_body_size)
            }))
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

/// Middleware that checks the Content-Length of POST/PUT requests against
/// the configured max body size. Returns HTTP 413 with a structured error
/// JSON and the `X-Gateway-Max-Body-Size` header when the limit is exceeded.
async fn body_limit_middleware(
    req: Request,
    next: Next,
    max_body_size: usize,
) -> Response {
    // Only enforce limits on methods that carry a request body
    if *req.method() == Method::POST || *req.method() == Method::PUT {
        if let Some(cl) = req.headers().get(axum::http::header::CONTENT_LENGTH) {
            if let Ok(len) = cl.to_str() {
                if let Ok(size) = len.parse::<usize>() {
                    if size > max_body_size {
                        let body = serde_json::json!({
                            "error_code": 41301,
                            "message": format!("request body exceeds the {max_body_size} byte limit"),
                        });
                        let mut resp = (
                            StatusCode::PAYLOAD_TOO_LARGE,
                            Json(body),
                        ).into_response();
                        resp.headers_mut().insert(
                            "X-Gateway-Max-Body-Size",
                            HeaderValue::from_str(&max_body_size.to_string()).unwrap(),
                        );
                        return resp;
                    }
                }
            }
        }
    }
    next.run(req).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http;
    use axum::routing::post;
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    /// A stub handler that echoes back the body size it received.
    async fn echo_size(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
        Json(json!({ "received": body }))
    }

    /// Build a test router with a configurable body limit.
    fn test_app(limit: usize) -> Router {
        Router::new()
            .route("/echo", post(echo_size))
            .layer(axum::middleware::from_fn(move |req, next| {
                body_limit_middleware(req, next, limit)
            }))
    }

    async fn call(
        app: Router,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> (StatusCode, serde_json::Value, http::HeaderMap) {
        let req_body = body.unwrap_or_default();
        let content_len = req_body.len();
        let request = http::Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("content-length", content_len.to_string())
            .body(axum::body::Body::from(req_body))
            .unwrap();

        let response =
            ServiceExt::<http::Request<axum::body::Body>>::oneshot(app, request)
                .await
                .unwrap();

        let status = response.status();
        let headers = response.headers().clone();
        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap_or_default();
        (status, json, headers)
    }

    #[tokio::test]
    async fn test_body_within_limit_passes() {
        let app = test_app(64);
        let (status, _, _) =
            call(app, Method::POST, "/echo", Some(json!({"hello":"world"}).to_string())).await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn test_body_exceeding_limit_returns_413() {
        let limit = 64;
        let app = test_app(limit);

        let big_body = json!({ "data": "a".repeat(200) });
        let body_str = big_body.to_string();
        let (status, json, headers) =
            call(app, Method::POST, "/echo", Some(body_str)).await;

        assert_eq!(status, 413);
        assert_eq!(json["error_code"], 41301);
        assert!(json["message"].as_str().unwrap().contains(&limit.to_string()));
        assert_eq!(
            headers.get("X-Gateway-Max-Body-Size").unwrap().to_str().unwrap(),
            limit.to_string()
        );
    }

    #[tokio::test]
    async fn test_body_exactly_at_limit_passes() {
        // Payload "{\"hello\":\"world\"}" = 18 bytes, Content-Length will be 18
        let app = test_app(18);
        let payload = json!({"hello":"world"}).to_string();
        let (status, _, _) = call(app, Method::POST, "/echo", Some(payload)).await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn test_get_request_bypasses_limit() {
        let app = Router::new()
            .route("/health", get(crate::server::rest::health))
            .layer(axum::middleware::from_fn(move |req, next| {
                body_limit_middleware(req, next, 1) // 1 byte limit, GET should bypass
            }));

        let request = http::Request::builder()
            .method(Method::GET)
            .uri("/health")
            .body(axum::body::Body::empty())
            .unwrap();

        let response =
            ServiceExt::<http::Request<axum::body::Body>>::oneshot(app, request)
                .await
                .unwrap();

        assert_eq!(response.status(), 200);
    }
}
