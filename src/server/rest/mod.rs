use std::collections::HashMap;

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use fluss::row::GenericRow;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::AuthType;
use crate::resilience::HealthStatus;
use crate::server::auth::BasicAuthCredentials;
use crate::server::AppState;
use crate::types::{
    json_to_datum, BucketOffset, CreateDatabaseRequest, CreateTableRequest, DropDatabaseRequest,
    DropTableRequest, GatewayError, ListOffsetsResponse, ListPartitionsResponse, LookupParams,
    ProduceRequest, ScanParams, WriteResult,
};
use fluss::rpc::message::OffsetSpec;

/// Query parameters for health endpoint
#[derive(Debug, Deserialize)]
pub struct HealthParams {
    #[serde(default)]
    pub deep: Option<bool>,
}

/// Extract credentials from the request based on auth mode.
fn extract_creds(
    auth_type: &AuthType,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Option<BasicAuthCredentials>, GatewayError> {
    match auth_type {
        AuthType::None => Ok(None),
        AuthType::Passthrough => {
            let ext =
                ext.ok_or_else(|| GatewayError::Unauthorized("authentication required".into()))?;
            Ok(Some(ext.0))
        }
    }
}

// === Health ===

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    summary = "Health check",
    description = "Returns the health status of the gateway. Use ?deep=true for a deep health check that verifies Fluss connectivity.",
    params(
        ("deep" = Option<bool>, Query, description = "Perform deep health check by verifying Fluss connectivity")
    ),
    responses(
        (status = 200, description = "Gateway health status", body = serde_json::Value, 
            example = json!({"status": "healthy", "fluss": "connected", "timestamp": "2024-01-01T00:00:00Z"}))
    )
)]
pub async fn health(
    Query(params): Query<HealthParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let deep = params.deep.unwrap_or(false);

    if !deep {
        // Simple health check - just verify the gateway is running
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })),
        );
    }

    // Deep health check - verify Fluss connectivity
    let (fluss_status, circuit_status) = check_fluss_health(&state).await;

    let status = match (&fluss_status, &circuit_status) {
        (_, HealthStatus::Unhealthy) => "unhealthy",
        (_, HealthStatus::Degraded) => "degraded",
        (Ok(_), _) => "healthy",
        (Err(_), _) => "degraded",
    };

    let status_code = match status {
        "healthy" => StatusCode::OK,
        "degraded" => StatusCode::OK,
        "unhealthy" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::OK,
    };

    (
        status_code,
        Json(serde_json::json!({
            "status": status,
            "fluss": if fluss_status.is_ok() { "connected" } else { "disconnected" },
            "circuit_breaker": circuit_status.to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// Check Fluss connectivity and circuit breaker status
async fn check_fluss_health(state: &AppState) -> (Result<(), GatewayError>, HealthStatus) {
    let circuit_status = state.circuit_breaker.health().await;

    // If circuit is open, don't attempt the check
    if circuit_status == HealthStatus::Unhealthy {
        return (
            Err(GatewayError::Internal("circuit breaker is open".into())),
            circuit_status,
        );
    }

    // Attempt a lightweight health check
    let result = state.backend.check_health().await;
    (result, circuit_status)
}

// === Metadata ===

#[utoipa::path(
    get,
    path = "/v1/_databases",
    tag = "databases",
    summary = "List all databases",
    description = "Returns a list of all database names in the Fluss cluster",
    responses(
        (status = 200, description = "List of database names", body = Vec<String>, example = json!(["default", "production"])),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn list_databases(
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let dbs = state.backend.list_databases(creds.as_ref()).await?;
    Ok(Json(dbs))
}

#[utoipa::path(
    post,
    path = "/v1/_databases",
    tag = "databases",
    summary = "Create a database",
    description = "Creates a new database in the Fluss cluster",
    request_body = CreateDatabaseRequest,
    responses(
        (status = 201, description = "Database created successfully"),
        (status = 400, description = "Bad request - invalid database name"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn create_database(
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    // Database name must be provided in request body
    let db_name = req.database_name.as_str();
    if db_name.is_empty() {
        return Err(GatewayError::BadRequest(
            "database_name is required in request body".into(),
        ));
    }
    state
        .backend
        .create_database(
            db_name,
            req.comment.as_deref(),
            &req.custom_properties,
            req.ignore_if_exists,
            creds.as_ref(),
        )
        .await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    delete,
    path = "/v1/_databases/{db}",
    tag = "databases",
    summary = "Drop a database",
    description = "Drops an existing database from the Fluss cluster",
    params(
        ("db" = String, Path, description = "Database name to drop")
    ),
    request_body = DropDatabaseRequest,
    responses(
        (status = 204, description = "Database dropped successfully"),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn drop_database(
    Path(db): Path<String>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<DropDatabaseRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    state
        .backend
        .drop_database(&db, req.ignore_if_not_exists, req.cascade, creds.as_ref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/{db}/_tables",
    tag = "tables",
    summary = "List tables in a database",
    description = "Returns a list of all table names in the specified database",
    params(
        ("db" = String, Path, description = "Database name")
    ),
    responses(
        (status = 200, description = "List of table names", body = Vec<String>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn list_tables(
    Path(db): Path<String>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let tables = state.backend.list_tables(&db, creds.as_ref()).await?;
    Ok(Json(tables))
}

#[utoipa::path(
    post,
    path = "/v1/{db}/_tables",
    tag = "tables",
    summary = "Create a table",
    description = "Creates a new table in the specified database with the given schema",
    params(
        ("db" = String, Path, description = "Database name")
    ),
    request_body = CreateTableRequest,
    responses(
        (status = 201, description = "Table created successfully"),
        (status = 400, description = "Bad request - invalid schema"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn create_table(
    Path(db): Path<String>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<CreateTableRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let table_name = req.table_name.as_str();
    if table_name.is_empty() {
        return Err(GatewayError::BadRequest(
            "table_name is required in request body".into(),
        ));
    }

    // Build schema from column specs
    let mut schema_builder = fluss::metadata::Schema::builder();
    for col in &req.schema {
        let data_type = parse_data_type(&col.data_type)?;
        let mut col_builder = schema_builder.column(&col.name, data_type);
        if let Some(comment) = &col.comment {
            col_builder = col_builder.with_comment(comment);
        }
        schema_builder = col_builder;
    }

    // Add primary key if specified
    if let Some(pk) = &req.primary_key {
        if let Some(ref constraint_name) = pk.constraint_name {
            schema_builder =
                schema_builder.primary_key_named(constraint_name, pk.column_names.clone());
        } else {
            schema_builder = schema_builder.primary_key(pk.column_names.clone());
        }
    }

    let schema = schema_builder.build().map_err(fluss_err)?;

    // Collect properties, adding comment if present
    let mut properties = req.properties.clone().unwrap_or_default();
    if let Some(ref c) = req.comment {
        properties.insert("table.comment".to_string(), c.clone());
    }

    state
        .backend
        .create_table(
            &db,
            table_name,
            schema,
            req.partition_keys.clone().unwrap_or_default(),
            req.bucket_count,
            req.bucket_keys.clone().unwrap_or_default(),
            properties,
            req.comment.clone(),
            req.ignore_if_exists,
            creds.as_ref(),
        )
        .await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    put,
    path = "/v1/{db}/_tables/{table}",
    tag = "tables",
    summary = "Get table information",
    description = "Returns detailed information about a table including schema, columns, and bucket count",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "Table information", body = TableInfoResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn table_info_put(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<TableInfoResponse>, GatewayError> {
    table_info_inner(db, table, state, ext).await
}

#[utoipa::path(
    get,
    path = "/v1/{db}/{table}/_info",
    tag = "tables",
    summary = "Get table information",
    description = "Returns detailed information about a table including schema, columns, and bucket count",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "Table information", body = TableInfoResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn table_info_get(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<TableInfoResponse>, GatewayError> {
    table_info_inner(db, table, state, ext).await
}

async fn table_info_inner(
    db: String,
    table: String,
    state: AppState,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<TableInfoResponse>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let info = state
        .backend
        .get_table_info(&db, &table, creds.as_ref())
        .await?;
    let schema = &info.schema;
    let columns: Vec<ColumnInfo> = schema
        .columns()
        .iter()
        .map(|c| ColumnInfo {
            name: c.name().to_string(),
            data_type: format!("{:?}", c.data_type()),
        })
        .collect();
    Ok(Json(TableInfoResponse {
        table_id: info.table_id,
        database: db,
        table,
        columns,
        has_primary_key: schema.primary_key().is_some(),
        num_buckets: info.num_buckets,
    }))
}

/// Table information response
#[derive(Serialize, ToSchema)]
pub struct TableInfoResponse {
    /// Unique table identifier
    pub table_id: i64,
    /// Database name
    pub database: String,
    /// Table name
    pub table: String,
    /// Column definitions
    pub columns: Vec<ColumnInfo>,
    /// Whether the table has a primary key
    pub has_primary_key: bool,
    /// Number of buckets
    pub num_buckets: i32,
}

/// Column information
#[derive(Serialize, ToSchema)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// Column data type
    pub data_type: String,
}

// === KV Lookup ===

#[utoipa::path(
    get,
    path = "/v1/{db}/{table}",
    tag = "lookup",
    summary = "Key-value lookup",
    description = "Lookup a row by primary key. Pass primary key column names and values as query parameters.",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "Matching rows", body = Vec<serde_json::Value>),
        (status = 400, description = "Bad request - invalid key"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn lookup(
    Path((db, table)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let lookup_params = LookupParams::new(params);
    let rows = state
        .backend
        .lookup(&db, &table, &lookup_params, creds.as_ref())
        .await?;
    Ok(Json(rows))
}

// === Prefix Scan (TODO) ===

#[utoipa::path(
    get,
    path = "/v1/{db}/{table}/prefix",
    tag = "scan",
    summary = "Prefix scan (not implemented)",
    description = "Scan rows with a given primary key prefix. Not yet implemented.",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    responses(
        (status = 500, description = "Not implemented")
    )
)]
pub async fn prefix_scan(
    Path((_db, _table)): Path<(String, String)>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    Err(GatewayError::Internal(
        "prefix scan not yet implemented".into(),
    ))
}

// === Batch Lookup ===

/// Batch lookup request body
#[derive(Deserialize, ToSchema)]
pub struct BatchLookupRequest {
    /// List of primary key lookups to perform
    pub keys: Vec<HashMap<String, String>>,
}

#[utoipa::path(
    post,
    path = "/v1/{db}/{table}/batch",
    tag = "lookup",
    summary = "Batch key-value lookup",
    description = "Perform multiple primary key lookups in a single request",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    request_body = BatchLookupRequest,
    responses(
        (status = 200, description = "Matching rows for all keys", body = Vec<serde_json::Value>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn batch_lookup(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<BatchLookupRequest>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let mut results = Vec::new();
    for key in &req.keys {
        let lookup_params = LookupParams::new(key.clone());
        let rows = state
            .backend
            .lookup(&db, &table, &lookup_params, creds.as_ref())
            .await?;
        results.extend(rows);
    }
    Ok(Json(results))
}

// === Log Scan ===

#[utoipa::path(
    post,
    path = "/v1/{db}/{table}/scan",
    tag = "scan",
    summary = "Scan table data",
    description = "Scan rows from a log table with optional projection, limit, and timeout",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    request_body = ScanParams,
    responses(
        (status = 200, description = "Scanned rows", body = Vec<serde_json::Value>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn scan(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(params): Json<ScanParams>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let rows = state
        .backend
        .scan(&db, &table, &params, creds.as_ref())
        .await?;
    Ok(Json(rows))
}

// === List Offsets ===

/// List offsets request body
#[derive(Deserialize, ToSchema)]
pub struct ListOffsetsRequest {
    /// Bucket IDs to query. If omitted, queries bucket 0.
    pub buckets: Option<Vec<i32>>,
    /// Offset specification: "earliest", "latest", or "timestamp"
    pub spec: Option<String>,
    /// Timestamp (milliseconds) - required when spec is "timestamp"
    pub timestamp: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/v1/{db}/{table}/offsets",
    tag = "metadata",
    summary = "List bucket offsets",
    description = "Get the current offset for specified buckets. Supports 'earliest', 'latest', and 'timestamp' offset specifications.",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    request_body = ListOffsetsRequest,
    responses(
        (status = 200, description = "Bucket offsets", body = ListOffsetsResponse),
        (status = 400, description = "Bad request - invalid offset spec"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn list_offsets(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<ListOffsetsRequest>,
) -> Result<Json<ListOffsetsResponse>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;

    let spec = match req.spec.as_deref().unwrap_or("earliest") {
        "earliest" => OffsetSpec::Earliest,
        "latest" => OffsetSpec::Latest,
        "timestamp" => {
            let ts = req.timestamp.ok_or_else(|| {
                GatewayError::BadRequest("timestamp is required when spec=timestamp".into())
            })?;
            OffsetSpec::Timestamp(ts)
        }
        other => {
            return Err(GatewayError::BadRequest(format!(
                "invalid offset spec: {}",
                other
            )))
        }
    };

    let buckets: Vec<i32> = req.buckets.unwrap_or_else(|| (0..).take(1).collect());
    let offsets = state
        .backend
        .list_offsets(&db, &table, &buckets, spec, creds.as_ref())
        .await?;

    let offset_list: Vec<BucketOffset> = offsets
        .into_iter()
        .map(|(bucket_id, offset)| BucketOffset { bucket_id, offset })
        .collect();

    Ok(Json(ListOffsetsResponse {
        table_path: format!("{}/{}", db, table),
        spec: req.spec.unwrap_or_else(|| "earliest".to_string()),
        offsets: offset_list,
    }))
}

// === List Partitions ===

#[utoipa::path(
    get,
    path = "/v1/{db}/{table}/partitions",
    tag = "metadata",
    summary = "List table partitions",
    description = "Returns all partitions for the specified table",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "List of partitions", body = ListPartitionsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn list_partitions(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<ListPartitionsResponse>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let partitions = state
        .backend
        .list_partitions(&db, &table, creds.as_ref())
        .await?;

    let partition_infos: Vec<crate::types::PartitionInfo> = partitions
        .into_iter()
        .map(|p| crate::types::PartitionInfo {
            partition_id: p.get_partition_id(),
            partition_name: p.get_partition_name(),
            partition_spec: p.get_partition_spec().get_spec_map().clone(),
        })
        .collect();

    Ok(Json(ListPartitionsResponse {
        table_path: format!("{}/{}", db, table),
        partitions: partition_infos,
    }))
}

// === Drop Table ===

#[utoipa::path(
    delete,
    path = "/v1/{db}/_tables/{table}",
    tag = "tables",
    summary = "Drop a table",
    description = "Drops an existing table from the specified database",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    request_body = DropTableRequest,
    responses(
        (status = 204, description = "Table dropped successfully"),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn drop_table(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<DropTableRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    state
        .backend
        .drop_table(&db, &table, req.ignore_if_not_exists, creds.as_ref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_data_type(type_str: &str) -> Result<fluss::metadata::DataType, GatewayError> {
    use fluss::metadata::DataTypes;
    match type_str.to_lowercase().as_str() {
        "boolean" | "bool" => Ok(DataTypes::boolean()),
        "tinyint" | "i8" => Ok(DataTypes::tinyint()),
        "smallint" | "i16" => Ok(DataTypes::smallint()),
        "int" | "integer" | "i32" => Ok(DataTypes::int()),
        "bigint" | "long" | "i64" => Ok(DataTypes::bigint()),
        "float" | "f32" => Ok(DataTypes::float()),
        "double" | "f64" => Ok(DataTypes::double()),
        "string" | "varchar" => Ok(DataTypes::string()),
        "bytes" | "binary" | "blob" => Ok(DataTypes::bytes()),
        other => Err(GatewayError::BadRequest(format!(
            "unsupported data type: {}",
            other
        ))),
    }
}

fn fluss_err(e: fluss::error::Error) -> GatewayError {
    GatewayError::FlussError(e.to_string())
}

// === Produce (Write) ===

#[utoipa::path(
    post,
    path = "/v1/{db}/{table}/rows",
    tag = "produce",
    summary = "Write rows to a table",
    description = "Write data rows to a table. For primary key tables, uses upsert or delete based on change_type. For log tables, appends rows.",
    params(
        ("db" = String, Path, description = "Database name"),
        ("table" = String, Path, description = "Table name")
    ),
    request_body = ProduceRequest,
    responses(
        (status = 200, description = "Write successful", body = WriteResult),
        (status = 400, description = "Bad request - invalid data"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("basic_auth" = [])
    )
)]
pub async fn produce(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<WriteResult>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let table_info = state
        .backend
        .get_table_info(&db, &table, creds.as_ref())
        .await?;
    let schema = &table_info.schema;
    let columns = schema.columns();
    let field_count = columns.len();

    let mut rows = Vec::with_capacity(req.rows.len());
    for (i, prow) in req.rows.iter().enumerate() {
        let mut row = GenericRow::new(field_count);
        for (j, value) in prow.values.iter().enumerate() {
            if j >= field_count {
                break;
            }
            let datum = json_to_datum(value, columns[j].data_type())
                .map_err(|e| GatewayError::BadRequest(format!("row[{}] field[{}]: {}", i, j, e)))?;
            row.set_field(j, datum);
        }
        rows.push(row);
    }

    let result = if schema.primary_key().is_some() {
        let has_delete = req
            .rows
            .iter()
            .any(|r| r.change_type.as_deref() == Some("Delete"));
        if has_delete {
            state
                .backend
                .delete_rows(&db, &table, rows, creds.as_ref())
                .await?
        } else {
            state
                .backend
                .upsert_rows(&db, &table, rows, creds.as_ref())
                .await?
        }
    } else {
        state
            .backend
            .append_rows(&db, &table, rows, creds.as_ref())
            .await?
    };

    Ok(Json(result))
}

// === Error response ===

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        let status =
            StatusCode::from_u16(self.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::json!({
            "error_code": self.error_code(),
            "message": self.to_string(),
        });
        (status, Json(body)).into_response()
    }
}
