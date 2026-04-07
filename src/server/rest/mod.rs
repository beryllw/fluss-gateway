use std::collections::HashMap;

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use fluss::row::GenericRow;
use serde::{Deserialize, Serialize};

use crate::config::AuthType;
use crate::server::auth::BasicAuthCredentials;
use crate::server::AppState;
use crate::types::{
    json_to_datum, BucketOffset, CreateDatabaseRequest, CreateTableRequest, DropDatabaseRequest,
    DropTableRequest, GatewayError, ListOffsetsResponse, ListPartitionsResponse, LookupParams,
    ProduceRequest, ScanParams, WriteResult,
};
use fluss::rpc::message::OffsetSpec;

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

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// === Metadata ===

pub async fn list_databases(
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let dbs = state.backend.list_databases(creds.as_ref()).await?;
    Ok(Json(dbs))
}

pub async fn list_tables(
    Path(db): Path<String>,
    State(state): State<AppState>,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let creds = extract_creds(&state.auth_type, ext)?;
    let tables = state.backend.list_tables(&db, creds.as_ref()).await?;
    Ok(Json(tables))
}

pub async fn table_info(
    Path((db, table)): Path<(String, String)>,
    State(state): State<AppState>,
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

#[derive(Serialize)]
pub struct TableInfoResponse {
    pub table_id: i64,
    pub database: String,
    pub table: String,
    pub columns: Vec<ColumnInfo>,
    pub has_primary_key: bool,
    pub num_buckets: i32,
}

#[derive(Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

// === KV Lookup ===

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

pub async fn prefix_scan(
    Path((_db, _table)): Path<(String, String)>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    Err(GatewayError::Internal(
        "prefix scan not yet implemented".into(),
    ))
}

// === Batch Lookup ===

#[derive(Deserialize)]
pub(crate) struct BatchLookupRequest {
    keys: Vec<HashMap<String, String>>,
}

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

// === Metadata Management (Phase 7) ===

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

#[derive(Deserialize)]
pub struct ListOffsetsRequest {
    pub buckets: Option<Vec<i32>>,
    pub spec: Option<String>,
    pub timestamp: Option<i64>,
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
