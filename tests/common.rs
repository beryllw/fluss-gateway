//! Common utilities for integration tests.
//!
//! This module provides helpers for:
//! - Docker Compose lifecycle management
//! - Fluss client for table creation
//! - HTTP client wrapper for gateway API
//! - Retry/wait helpers for cluster readiness

use std::process::Command;
use std::time::{Duration, Instant};

use reqwest::Client;

const GATEWAY_URL: &str = "http://localhost:8080";
const FLUSS_COORDINATOR: &str = "localhost:9123";
const COMPOSE_FILE: &str = "deploy/docker/docker-compose.dev.yml";
const COMPOSE_PROJECT: &str = "fluss-gateway";

// Global mutex to ensure only one test manages the cluster at a time
static CLUSTER_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run a compose command with podman and the correct project name.
fn compose(args: &[&str]) -> std::process::ExitStatus {
    Command::new("podman")
        .args(["compose", "--project-name", COMPOSE_PROJECT, "-f", COMPOSE_FILE])
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run podman compose: {}", e))
}

/// Start the compose cluster and gateway binary if not already running.
pub async fn start_cluster() -> Result<(), String> {
    let _lock = CLUSTER_MUTEX.lock();

    // Check if gateway is already running
    if is_gateway_ready().await {
        println!("Gateway already running, skipping startup");
        return Ok(());
    }

    println!("Starting Fluss cluster via podman compose...");
    if !compose(&["up", "-d"]).success() {
        return Err("podman compose up failed".to_string());
    }

    // Wait for Fluss to be healthy
    let mut retries = 0;
    while retries < 30 {
        let status = Command::new("podman")
            .args(["inspect", "--format={{.State.Health.Status}}", "fluss-gateway-coordinator-server-1"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());

        if status.as_deref().map(|s| s.trim()) == Some("healthy") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        retries += 1;
    }

    // Start the gateway binary as a subprocess
    println!("Starting gateway binary...");
    let binary = env!("CARGO_BIN_EXE_fluss-gateway");
    let child = Command::new(binary)
        .arg("serve")
        .arg("--fluss-coordinator=localhost:9123")
        .arg("--port=8080")
        .arg("--auth-type=none")
        .spawn()
        .map_err(|e| format!("Failed to start gateway binary: {}", e))?;

    // Store PID for cleanup
    let pid = child.id();
    std::fs::write("/tmp/fluss-gateway-test.pid", pid.to_string())
        .map_err(|e| format!("Failed to write PID: {}", e))?;

    // Detach: we'll clean up via PID later
    std::mem::forget(child);

    Ok(())
}

/// Stop the compose cluster and gateway process (kept for manual use or GATEWAY_KEEP_CLUSTER).
#[allow(dead_code)]
pub fn stop_cluster() {
    let _lock = CLUSTER_MUTEX.lock();

    // Kill the gateway process
    if let Ok(pid_str) = std::fs::read_to_string("/tmp/fluss-gateway-test.pid") {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            let _ = Command::new("kill").arg(pid.to_string()).status();
            let _ = std::fs::remove_file("/tmp/fluss-gateway-test.pid");
        }
    }

    let _ = compose(&["down"]);
}

/// Check if the gateway health endpoint responds.
pub async fn is_gateway_ready() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(format!("{}/health", GATEWAY_URL))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Wait for the gateway to become ready, with a timeout.
pub async fn wait_for_gateway(timeout_secs: u64) -> Result<(), String> {
    let start = Instant::now();
    let interval = Duration::from_secs(3);

    while start.elapsed() < Duration::from_secs(timeout_secs) {
        if is_gateway_ready().await {
            println!("Gateway is ready!");
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }

    Err(format!(
        "Gateway did not become ready within {} seconds",
        timeout_secs
    ))
}

/// Create a test log table via Fluss client.
pub async fn create_test_log_table(db: &str, table: &str) -> Result<(), String> {
    use fluss::client::FlussConnection;
    use fluss::config::Config;
    use fluss::metadata::{DataType, Schema, TableDescriptor, TablePath};

    let mut config = Config::default();
    config.bootstrap_servers = FLUSS_COORDINATOR.to_string();

    let conn = FlussConnection::new(config)
        .await
        .map_err(|e| format!("Failed to connect to Fluss: {}", e))?;

    let admin = conn
        .get_admin()
        .map_err(|e| format!("Failed to get admin: {}", e))?;

    // Ensure database exists
    let exists = admin
        .database_exists(db)
        .await
        .map_err(|e| format!("Failed to check database: {}", e))?;

    if !exists {
        admin
            .create_database(db, None, true)
            .await
            .map_err(|e| format!("Failed to create database {}: {}", db, e))?;
    }

    // Create log table (no primary key)
    let schema = Schema::builder()
        .column("id", DataType::Int(Default::default()))
        .column("name", DataType::String(Default::default()))
        .column("value", DataType::BigInt(Default::default()))
        .build()
        .map_err(|e| format!("Failed to build schema: {}", e))?;

    let descriptor = TableDescriptor::builder()
        .schema(schema)
        .build()
        .map_err(|e| format!("Failed to build descriptor: {}", e))?;

    let table_path = TablePath::new(db, table);

    // Try to create, ignore if already exists
    let result = admin.create_table(&table_path, &descriptor, true).await;

    if let Err(e) = result {
        return Err(format!("Failed to create table: {}", e));
    }

    Ok(())
}

/// Create a test PK table via Fluss client.
pub async fn create_test_pk_table(db: &str, table: &str) -> Result<(), String> {
    use fluss::client::FlussConnection;
    use fluss::config::Config;
    use fluss::metadata::{DataType, Schema, TableDescriptor, TablePath};

    let mut config = Config::default();
    config.bootstrap_servers = FLUSS_COORDINATOR.to_string();

    let conn = FlussConnection::new(config)
        .await
        .map_err(|e| format!("Failed to connect to Fluss: {}", e))?;

    let admin = conn
        .get_admin()
        .map_err(|e| format!("Failed to get admin: {}", e))?;

    // Ensure database exists
    let exists = admin
        .database_exists(db)
        .await
        .map_err(|e| format!("Failed to check database: {}", e))?;

    if !exists {
        admin
            .create_database(db, None, true)
            .await
            .map_err(|e| format!("Failed to create database {}: {}", db, e))?;
    }

    // Create PK table
    let schema = Schema::builder()
        .column("user_id", DataType::Int(Default::default()))
        .column("username", DataType::String(Default::default()))
        .column("email", DataType::String(Default::default()))
        .primary_key(vec!["user_id"])
        .build()
        .map_err(|e| format!("Failed to build schema: {}", e))?;

    let descriptor = TableDescriptor::builder()
        .schema(schema)
        .build()
        .map_err(|e| format!("Failed to build descriptor: {}", e))?;

    let table_path = TablePath::new(db, table);

    let result = admin.create_table(&table_path, &descriptor, true).await;

    if let Err(e) = result {
        return Err(format!("Failed to create table: {}", e));
    }

    Ok(())
}

/// HTTP client wrapper for gateway API.
pub struct GatewayClient {
    client: Client,
    base_url: String,
}

impl GatewayClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to create HTTP client"),
            base_url: GATEWAY_URL.to_string(),
        }
    }

    pub async fn health(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Unexpected status {}: {:?}", status, body));
        }

        Ok(body)
    }

    pub async fn list_databases(&self) -> Result<Vec<String>, String> {
        let resp = self
            .client
            .get(format!("{}/v1/_databases", self.base_url))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: Vec<String> = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }

    pub async fn list_tables(&self, db: &str) -> Result<Vec<String>, String> {
        let resp = self
            .client
            .get(format!("{}/v1/{}/_tables", self.base_url, db))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: Vec<String> = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }

    pub async fn table_info(&self, db: &str, table: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(format!(
                "{}/v1/{}/{}/_info",
                self.base_url, db, table
            ))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Unexpected status {}: {:?}", status, body));
        }

        Ok(body)
    }

    pub async fn lookup(
        &self,
        db: &str,
        table: &str,
        params: &[(&str, &str)],
    ) -> Result<serde_json::Value, String> {
        let mut url = format!("{}/v1/{}/{}", self.base_url, db, table);
        if !params.is_empty() {
            let qs: Vec<String> = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            url.push_str(&format!("?{}", qs.join("&")));
        }

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }

    pub async fn produce(
        &self,
        db: &str,
        table: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(format!("{}/v1/{}/{}/rows", self.base_url, db, table))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Unexpected status {}: {:?}", status, body));
        }

        Ok(body)
    }

    pub async fn scan(
        &self,
        db: &str,
        table: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(format!("{}/v1/{}/{}/scan", self.base_url, db, table))
            .json(params)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }

    pub async fn batch_lookup(
        &self,
        db: &str,
        table: &str,
        keys: &[Vec<(String, String)>],
    ) -> Result<serde_json::Value, String> {
        let body = serde_json::json!({
            "keys": keys.iter().map(|k| {
                let mut map = serde_json::Map::new();
                for (key, val) in k {
                    map.insert(key.clone(), serde_json::Value::String(val.clone()));
                }
                serde_json::Value::Object(map)
            }).collect::<Vec<_>>()
        });

        let resp = self
            .client
            .post(format!("{}/v1/{}/{}/batch", self.base_url, db, table))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }

    // === Phase 7: Metadata Management ===

    pub async fn create_database(&self, db_name: &str) -> Result<u16, String> {
        let body = serde_json::json!({
            "database_name": db_name,
            "comment": format!("test database {}", db_name),
            "ignore_if_exists": true,
        });

        let resp = self
            .client
            .post(format!("{}/v1/_databases", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        Ok(resp.status().as_u16())
    }

    pub async fn drop_database(&self, db_name: &str) -> Result<u16, String> {
        let body = serde_json::json!({
            "ignore_if_not_exists": true,
            "cascade": true,
        });

        let resp = self
            .client
            .delete(format!("{}/v1/_databases/{}", self.base_url, db_name))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        Ok(resp.status().as_u16())
    }

    pub async fn create_table(&self, db: &str, table_name: &str) -> Result<u16, String> {
        let body = serde_json::json!({
            "table_name": table_name,
            "schema": [
                { "name": "id", "data_type": "int" },
                { "name": "name", "data_type": "string" },
                { "name": "value", "data_type": "bigint" },
            ],
            "ignore_if_exists": true,
        });

        let resp = self
            .client
            .post(format!("{}/v1/{}/_tables", self.base_url, db))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        Ok(resp.status().as_u16())
    }

    pub async fn drop_table(&self, db: &str, table: &str) -> Result<u16, String> {
        let body = serde_json::json!({
            "ignore_if_not_exists": true,
        });

        let resp = self
            .client
            .delete(format!("{}/v1/{}/_tables/{}", self.base_url, db, table))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        Ok(resp.status().as_u16())
    }

    pub async fn list_offsets(
        &self,
        db: &str,
        table: &str,
    ) -> Result<serde_json::Value, String> {
        let body = serde_json::json!({
            "spec": "earliest",
            "buckets": [0],
        });

        let resp = self
            .client
            .post(format!("{}/v1/{}/{}/offsets", self.base_url, db, table))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Unexpected status {}: {:?}", status, body));
        }

        Ok(body)
    }

    pub async fn list_partitions(
        &self,
        db: &str,
        table: &str,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(format!("{}/v1/{}/{}/partitions", self.base_url, db, table))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(body)
    }
}
