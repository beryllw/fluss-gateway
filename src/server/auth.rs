use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use futures_util::future::BoxFuture;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Credentials extracted from HTTP Basic Auth.
#[derive(Clone, Debug)]
pub struct BasicAuthCredentials {
    pub username: String,
    pub password: String,
}

/// Axum layer that extracts HTTP Basic Auth credentials and stores them
/// in request extensions. If no auth header is present, the request proceeds
/// without credentials (for unauthenticated endpoints like /health).
#[derive(Clone)]
pub struct AuthLayer;

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService { inner }
    }
}

#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
}

impl<S> Service<Request> for AuthService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, inner);

        Box::pin(async move {
            if let Some(auth_header) = req.headers().get("authorization") {
                if let Ok(auth_str) = auth_header.to_str() {
                    if let Some(creds) = parse_basic_auth(auth_str) {
                        req.extensions_mut().insert(creds);
                    }
                }
            }
            inner.call(req).await
        })
    }
}

pub(crate) fn parse_basic_auth(header_value: &str) -> Option<BasicAuthCredentials> {
    if !header_value.starts_with("Basic ") {
        return None;
    }
    let encoded = &header_value[6..];
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let colon_pos = decoded_str.find(':')?;
    let username = decoded_str[..colon_pos].to_string();
    let password = decoded_str[colon_pos + 1..].to_string();

    if username.is_empty() {
        return None;
    }

    Some(BasicAuthCredentials { username, password })
}

/// Require auth for a route. Returns 401 if no credentials are present.
pub fn require_auth(req: &Request) -> Option<BasicAuthCredentials> {
    req.extensions().get::<BasicAuthCredentials>().cloned()
}

/// Auth rejection response
pub fn auth_rejection() -> Response {
    let mut response = Response::new(axum::body::Body::from(
        serde_json::json!({
            "error_code": 40101,
            "message": "authentication required"
        })
        .to_string(),
    ));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response
        .headers_mut()
        .insert("WWW-Authenticate", HeaderValue::from_static("Basic realm=\"Fluss Gateway\""));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_auth() {
        let creds = parse_basic_auth("Basic dXNlcjpwYXNz").unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_parse_basic_auth_invalid() {
        assert!(parse_basic_auth("Bearer token").is_none());
        assert!(parse_basic_auth("Basic notbase64").is_none());
        assert!(parse_basic_auth("Basic dXNlcg==").is_none()); // no colon
    }

    #[test]
    fn test_parse_basic_auth_empty_user() {
        assert!(parse_basic_auth("Basic OnBhc3M=").is_none()); // :pass
    }
}
