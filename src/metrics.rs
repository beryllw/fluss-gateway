use std::sync::OnceLock;

use axum::response::IntoResponse;

/// Global metrics recorder handle for scraping.
static RECORDER: OnceLock<metrics_exporter_prometheus::PrometheusHandle> = OnceLock::new();

/// Prometheus recorder that exposes metrics via HTTP.
pub struct PrometheusRecorder;

impl PrometheusRecorder {
    pub fn install() -> Result<&'static Self, Box<dyn std::error::Error + Send + Sync>> {
        let recorder_handle =
            metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder()?;
        RECORDER.get_or_init(|| recorder_handle);
        Ok(&PROMETHEUS_RECORDER)
    }

    pub fn scrape() -> String {
        RECORDER.get().map(|h| h.render()).unwrap_or_default()
    }
}

static PROMETHEUS_RECORDER: PrometheusRecorder = PrometheusRecorder;

pub fn scrape_metrics() -> String {
    PrometheusRecorder::scrape()
}

pub async fn metrics_handler() -> impl IntoResponse {
    let metrics = scrape_metrics();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics,
    )
}

pub fn http_requests_total(method: &str, path: &str, status: u16) {
    metrics::counter!(
        "http_requests_total",
        "method" => method.to_string(),
        "path" => path.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

pub fn http_request_duration_seconds(method: &str, path: &str, duration_secs: f64) {
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method.to_string(),
        "path" => path.to_string()
    )
    .record(duration_secs);
}

pub fn errors_total(error_type: &str) {
    metrics::counter!(
        "errors_total",
        "type" => error_type.to_string()
    )
    .increment(1);
}

pub fn record_http_request(method: &str, path: &str, status: u16) {
    http_requests_total(method, path, status);
}

pub fn record_http_request_duration(method: &str, path: &str, duration_secs: f64) {
    http_request_duration_seconds(method, path, duration_secs);
}

pub fn record_error(error_type: &str) {
    errors_total(error_type);
}

pub fn normalize_path(uri: &str) -> String {
    let path = uri.split('?').next().unwrap_or(uri);
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() >= 3 && parts[1] == "v1" {
        // /v1/_databases, /v1/{db}/_tables, etc.
        if parts.len() == 3 {
            return path.to_string();
        }
        // /v1/{db}/_tables (collection endpoints)
        if parts.len() == 4 && parts[3].starts_with('_') {
            return format!("/v1/{{db}}/{}", parts[3]);
        }
        // /v1/{db}/{table}
        if parts.len() == 4 {
            return "/v1/{db}/{table}".to_string();
        }
        // /v1/{db}/{table}/<action>
        let mut normalized = String::from("/v1/{db}/{table}");
        normalized.push('/');
        normalized.push_str(parts[4]);
        return normalized;
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_api_routes() {
        assert_eq!(
            normalize_path("/v1/mydb/mytable/rows"),
            "/v1/{db}/{table}/rows"
        );
        assert_eq!(
            normalize_path("/v1/mydb/mytable/scan"),
            "/v1/{db}/{table}/scan"
        );
        assert_eq!(normalize_path("/v1/mydb/mytable"), "/v1/{db}/{table}");
        assert_eq!(normalize_path("/v1/_databases"), "/v1/_databases");
        assert_eq!(normalize_path("/v1/mydb/_tables"), "/v1/{db}/_tables");
    }

    #[test]
    fn test_normalize_path_system_routes() {
        assert_eq!(normalize_path("/health"), "/health");
        assert_eq!(normalize_path("/metrics"), "/metrics");
    }

    #[test]
    fn test_normalize_path_with_query() {
        assert_eq!(
            normalize_path("/v1/mydb/mytable/rows?format=arrow"),
            "/v1/{db}/{table}/rows"
        );
    }
}
