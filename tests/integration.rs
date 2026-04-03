//! Integration tests for Fluss Gateway.
//!
//! These tests require a running gateway server.
//! Set `GATEWAY_URL` env var (default: http://localhost:8080).
//!
//! Run with: `GATEWAY_URL=http://localhost:8080 cargo test --test integration -- --ignored`

use reqwest::Client;

fn gateway_url() -> String {
    std::env::var("GATEWAY_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

async fn client() -> Client {
    Client::builder()
        .build()
        .expect("failed to create HTTP client")
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_health_endpoint() {
    let url = format!("{}/health", gateway_url());
    let resp = client().await.get(&url).send().await.unwrap();

    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_list_databases() {
    let url = format!("{}/v1/_databases", gateway_url());
    let resp = client().await.get(&url).send().await.unwrap();

    // Should return 200 even if no databases exist
    assert_eq!(resp.status().as_u16(), 200);
    let dbs: Vec<String> = resp.json().await.unwrap();
    assert!(dbs.is_empty() || dbs.iter().any(|db| db == "fluss"));
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_table_info_not_found() {
    let url = format!("{}/v1/nonexistent/table/_info", gateway_url());
    let resp = client().await.get(&url).send().await.unwrap();

    // Should return 4xx or 5xx error response
    assert!(resp.status().is_client_error() || resp.status().is_server_error());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("error_code").is_some());
    assert!(body.get("message").is_some());
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_lookup_missing_pk() {
    let url = format!("{}/v1/nonexistent/table", gateway_url());
    let resp = client().await.get(&url).send().await.unwrap();

    assert!(resp.status().is_client_error() || resp.status().is_server_error());
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_scan_empty_params() {
    let url = format!("{}/v1/nonexistent/table/scan", gateway_url());
    let resp = client()
        .await
        .post(&url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_client_error() || resp.status().is_server_error());
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_produce_invalid_table() {
    let url = format!("{}/v1/nonexistent/table/rows", gateway_url());
    let resp = client()
        .await
        .post(&url)
        .json(&serde_json::json!({
            "rows": [{"values": [1, "test"]}]
        }))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_client_error() || resp.status().is_server_error());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("error_code").is_some());
}

#[tokio::test]
#[ignore = "requires running gateway"]
async fn test_batch_lookup_empty_keys() {
    let url = format!("{}/v1/nonexistent/table/batch", gateway_url());
    let resp = client()
        .await
        .post(&url)
        .json(&serde_json::json!({ "keys": [] }))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}
