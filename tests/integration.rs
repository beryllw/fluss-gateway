//! Integration tests for Fluss Gateway against a real Fluss cluster.
//!
//! These tests start a Fluss cluster (ZooKeeper + Coordinator + TabletServer)
//! via Docker Compose, then test the Gateway REST API end-to-end.
//!
//! Run with: `cargo test --test integration`
//! The tests will automatically start the cluster if not already running.

mod common;

use common::{GatewayClient, create_test_log_table, create_test_pk_table, start_cluster, wait_for_gateway};

/// Setup: start cluster and wait for readiness.
async fn setup() -> GatewayClient {
    start_cluster().await.expect("Failed to start cluster");
    wait_for_gateway(120)
        .await
        .expect("Gateway did not become ready");
    GatewayClient::new()
}

/// Teardown: stop cluster only when GATEWAY_KEEP_CLUSTER is NOT set.
/// When tests run in parallel (default), teardown is a no-op to avoid
/// killing the cluster mid-test.
fn teardown() {
    // No-op: the cluster is shared across parallel tests.
    // Clean up manually with `podman compose down` after test runs if needed.
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

    let db = "test_log_db_append";
    let table = "test_log_table_append";
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
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

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

// ============================================================================
// Phase 7: Metadata Management
// ============================================================================

#[tokio::test]
async fn test_create_database() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_create_db_meta";
    let status = client.create_database(db).await.expect("create_database failed");
    assert_eq!(status, 201, "Expected 201 Created, got {}", status);
}

#[tokio::test]
async fn test_create_database_idempotent() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_idempotent_db";
    let status1 = client.create_database(db).await.expect("create_database failed");
    assert_eq!(status1, 201);

    // Second create with ignore_if_exists=true should also succeed
    let status2 = client.create_database(db).await.expect("create_database failed");
    assert_eq!(status2, 201);
}

#[tokio::test]
async fn test_drop_database() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_drop_db_meta";
    // Ensure it exists
    client.create_database(db).await.expect("create_database failed");

    let status = client.drop_database(db).await.expect("drop_database failed");
    assert_eq!(status, 204, "Expected 204 No Content, got {}", status);
}

#[tokio::test]
async fn test_create_table_via_gateway() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_create_table_db";
    client.create_database(db).await.expect("create_database failed");

    let status = client
        .create_table(db, "gateway_created_table")
        .await
        .expect("create_table failed");
    assert_eq!(status, 201, "Expected 201 Created, got {}", status);

    // Verify the table exists via list_tables
    let tables = client.list_tables(db).await.expect("list_tables failed");
    assert!(
        tables.contains(&"gateway_created_table".to_string()),
        "Expected table gateway_created_table in {:?}",
        tables
    );
}

#[tokio::test]
async fn test_drop_table_via_gateway() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_drop_table_db";
    client.create_database(db).await.expect("create_database failed");

    // Create via gateway
    client
        .create_table(db, "table_to_drop")
        .await
        .expect("create_table failed");

    // Drop via gateway
    let status = client
        .drop_table(db, "table_to_drop")
        .await
        .expect("drop_table failed");
    assert_eq!(status, 204, "Expected 204 No Content, got {}", status);
}

#[tokio::test]
async fn test_list_offsets() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_offsets_db";
    let table = "test_offsets_table";
    create_test_log_table(db, table)
        .await
        .expect("Failed to create test table");

    // Write some data first
    let result = client
        .produce(
            db,
            table,
            &serde_json::json!({
                "rows": [{ "values": [1, "test", 42] }]
            }),
        )
        .await
        .expect("produce failed");
    assert_eq!(result["row_count"], 1);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let offsets = client
        .list_offsets(db, table)
        .await
        .expect("list_offsets failed");
    assert!(offsets["offsets"].as_array().is_some());
}

#[tokio::test]
async fn test_list_partitions_empty() {
    let client = setup().await;
    let _guard = scopeguard::guard((), |_| teardown());

    let db = "test_partitions_db_v2";
    let table = "test_partitions_table_v2";
    create_test_log_table(db, table)
        .await
        .expect("Failed to create test table");

    // Non-partitioned table should return empty partitions
    let partitions = client
        .list_partitions(db, table)
        .await
        .expect("list_partitions failed");
    assert!(
        partitions["partitions"].as_array().is_some(),
        "Expected 'partitions' key in response: {:?}",
        partitions
    );
    assert_eq!(partitions["partitions"].as_array().unwrap().len(), 0);
}
