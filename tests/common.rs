//! Common utilities for integration tests.
//!
//! This module provides helpers for:
//! - Fluss client for table creation
//! - HTTP client wrapper for gateway API

use std::process::Command;
use std::time::Duration;

use reqwest::Client;

const GATEWAY_URL: &str = "http://localhost:8080";
const FLUSS_COORDINATOR: &str = "localhost:9123";

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

/// Stop the compose cluster and gateway process.
/// Kept for manual invocation — teardown test handles cleanup automatically.
#[allow(dead_code)]
pub fn stop_cluster() {
    const COMPOSE_FILE: &str = "deploy/docker/docker-compose.dev.yml";
    const COMPOSE_PROJECT: &str = "fluss-gateway";

    // Kill the gateway process
    if let Ok(pid_str) = std::fs::read_to_string("/tmp/fluss-gateway-test.pid") {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            let _ = Command::new("kill").arg(pid.to_string()).status();
            let _ = std::fs::remove_file("/tmp/fluss-gateway-test.pid");
        }
    }

    let _ = Command::new("podman")
        .args(["compose", "--project-name", COMPOSE_PROJECT, "-f", COMPOSE_FILE, "down"])
        .status();
}
