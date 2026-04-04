//! Integration tests for Fluss Gateway against a real Fluss cluster.
//!
//! These tests start a Fluss cluster (ZooKeeper + Coordinator + TabletServer)
//! via Docker Compose, then test the Gateway REST API end-to-end.
//!
//! Run with: `cargo test --test integration`
//! The tests will automatically start the cluster if not already running.

mod common;

use common::{GatewayClient, create_test_log_table, create_test_pk_table, start_cluster, stop_cluster, wait_for_gateway};

/// Setup: start cluster and wait for readiness.
async fn setup() -> GatewayClient {
    start_cluster().await.expect("Failed to start cluster");
    wait_for_gateway(120)
        .await
        .expect("Gateway did not become ready");
    GatewayClient::new()
}

/// Teardown: stop cluster (only if GATEWAY_KEEP_CLUSTER env is not set).
fn teardown() {
    if std::env::var("GATEWAY_KEEP_CLUSTER").is_err() {
        stop_cluster();
    }
}

// ============================================================================
// Health
// ============================================================================

#[tokio::test]
async fn test_health() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let body = client.health().await.expect("health request failed");
    assert_eq!(body["status"], "ok");
}

// ============================================================================
// Metadata
// ============================================================================

#[tokio::test]
async fn test_list_databases() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let dbs = client.list_databases().await.expect("list_databases failed");
    // Fluss always has at least a default database
    assert!(!dbs.is_empty(), "Expected at least one database");
}

#[tokio::test]
async fn test_list_tables() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    // Create a test database first
    let db = "test_meta_db";
    let table = "test_meta_table";
    create_test_log_table(db, table)
        .await
        .expect("Failed to create test table");

    let tables = client.list_tables(db).await.expect("list_tables failed");
    assert!(
        tables.contains(&table.to_string()),
        "Expected table {} in {:?}",
        table,
        tables
    );
}

#[tokio::test]
async fn test_table_info() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_info_db";
    let table = "test_info_table";
    create_test_log_table(db, table)
        .await
        .expect("Failed to create test table");

    let info = client.table_info(db, table).await.expect("table_info failed");
    assert_eq!(info["database"], db);
    assert_eq!(info["table"], table);
    assert!(info["columns"].as_array().is_some());
    assert!(info["columns"].as_array().unwrap().len() >= 3);
}

// ============================================================================
// Log Table: Append + Scan
// ============================================================================

#[tokio::test]
async fn test_append_and_scan() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_log_db";
    let table = "test_log_table";
    create_test_log_table(db, table)
        .await
        .expect("Failed to create test table");

    // Append rows
    let result = client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [
                    { "values": [1, "Alice", 100] },
                    { "values": [2, "Bob", 200] },
                    { "values": [3, "Charlie", 300] }
                ]
            }),
        )
        .await
        .expect("produce failed");
    assert_eq!(result["row_count"], 3);

    // Give Fluss a moment to commit
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Scan rows
    let rows = client
        .scan(db, table, &serde_json::json!({ "timeout_ms": 5000 }))
        .await
        .expect("scan failed");

    let arr = rows.as_array().expect("scan result should be array");
    assert!(
        arr.len() >= 3,
        "Expected at least 3 rows, got {}: {:?}",
        arr.len(),
        rows
    );
}

// ============================================================================
// PK Table: Upsert + Lookup + Delete
// ============================================================================

#[tokio::test]
async fn test_upsert_and_lookup() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_pk_db";
    let table = "test_pk_table";
    create_test_pk_table(db, table)
        .await
        .expect("Failed to create test table");

    // Upsert rows
    let result = client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [
                    { "values": [1, "alice", "alice@test.com"] },
                    { "values": [2, "bob", "bob@test.com"] }
                ]
            }),
        )
        .await
        .expect("produce failed");
    assert_eq!(result["row_count"], 2);

    // Give Fluss a moment to commit
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Lookup by PK
    let rows = client
        .lookup(db, table, &[("user_id", "1")])
        .await
        .expect("lookup failed");

    let arr = rows.as_array().expect("lookup result should be array");
    assert!(
        arr.len() >= 1,
        "Expected at least 1 row from lookup, got {}: {:?}",
        arr.len(),
        rows
    );
}

#[tokio::test]
async fn test_delete_pk() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_del_db";
    let table = "test_del_table";
    create_test_pk_table(db, table)
        .await
        .expect("Failed to create test table");

    // Upsert a row
    client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [{ "values": [99, "to_delete", "del@test.com"] }]
            }),
        )
        .await
        .expect("produce failed");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Delete the row
    let result = client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [{ "values": [99, "to_delete", "del@test.com"], "change_type": "Delete" }]
            }),
        )
        .await
        .expect("delete failed");
    assert_eq!(result["row_count"], 1);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Lookup should return empty or not contain the deleted row
    let rows = client
        .lookup(db, table, &[("user_id", "99")])
        .await
        .expect("lookup after delete failed");

    let arr = rows.as_array().expect("lookup result should be array");
    assert!(
        arr.is_empty(),
        "Expected empty result after delete, got: {:?}",
        rows
    );
}

// ============================================================================
// Batch Lookup
// ============================================================================

#[tokio::test]
async fn test_batch_lookup() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_batch_db";
    let table = "test_batch_table";
    create_test_pk_table(db, table)
        .await
        .expect("Failed to create test table");

    // Upsert multiple rows
    client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [
                    { "values": [10, "user_a", "a@test.com"] },
                    { "values": [20, "user_b", "b@test.com"] },
                    { "values": [30, "user_c", "c@test.com"] }
                ]
            }),
        )
        .await
        .expect("produce failed");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Batch lookup for two keys
    let rows = client
        .batch_lookup(
            db,
            table,
            &[
                vec![("user_id".to_string(), "10".to_string())],
                vec![("user_id".to_string(), "20".to_string())],
            ],
        )
        .await
        .expect("batch lookup failed");

    let arr = rows.as_array().expect("batch lookup result should be array");
    assert!(
        arr.len() >= 2,
        "Expected at least 2 rows from batch lookup, got {}: {:?}",
        arr.len(),
        rows
    );
}

#[tokio::test]
async fn test_batch_lookup_empty_keys() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_batch_empty_db";
    let table = "test_batch_empty_table";
    create_test_pk_table(db, table)
        .await
        .expect("Failed to create test table");

    let rows = client
        .batch_lookup(db, table, &[])
        .await
        .expect("batch lookup with empty keys failed");

    let arr = rows.as_array().expect("result should be array");
    assert!(arr.is_empty(), "Expected empty result for empty keys");
}

// ============================================================================
// Error Handling
// ============================================================================

#[tokio::test]
async fn test_table_info_not_found() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let result = client.table_info("nonexistent_db", "nonexistent_table").await;
    assert!(result.is_err(), "Expected error for non-existent table");
}

#[tokio::test]
async fn test_produce_invalid_table() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let result = client
        .produce(
            "nonexistent_db",
            "nonexistent_table",
            &serde_json::json!({
                "rows": [{ "values": [1, "test"] }]
            }),
        )
        .await;

    assert!(result.is_err(), "Expected error for non-existent table");
}

#[tokio::test]
async fn test_lookup_missing_pk() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_missing_pk_db";
    let table = "test_missing_pk_table";
    create_test_pk_table(db, table)
        .await
        .expect("Failed to create test table");

    // Lookup for non-existent key should succeed but return empty
    let rows = client
        .lookup(db, table, &[("user_id", "99999")])
        .await
        .expect("lookup should not error");

    let arr = rows.as_array().expect("result should be array");
    assert!(
        arr.is_empty(),
        "Expected empty result for non-existent key"
    );
}
