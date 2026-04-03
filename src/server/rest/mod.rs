use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use fluss::row::GenericRow;
use serde::{Deserialize, Serialize};

use crate::backend::FlussBackend;
use crate::types::{
    GatewayError, LookupParams, ProduceRequest, ScanParams, WriteResult, json_to_datum,
};

// === Health ===

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// === Metadata ===

pub async fn list_databases(
    State(backend): State<Arc<FlussBackend>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let dbs = backend.list_databases().await?;
    Ok(Json(dbs))
}

pub async fn list_tables(
    Path(db): Path<String>,
    State(backend): State<Arc<FlussBackend>>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let tables = backend.list_tables(&db).await?;
    Ok(Json(tables))
}

pub async fn table_info(
    Path((db, table)): Path<(String, String)>,
    State(backend): State<Arc<FlussBackend>>,
) -> Result<Json<TableInfoResponse>, GatewayError> {
    let info = backend.get_table_info(&db, &table).await?;
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
    State(backend): State<Arc<FlussBackend>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let lookup_params = LookupParams::new(params);
    let rows = backend.lookup(&db, &table, &lookup_params).await?;
    Ok(Json(rows))
}

// === Prefix Scan (TODO) ===

pub async fn prefix_scan(
    Path((_db, _table)): Path<(String, String)>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    Err(GatewayError::Internal("prefix scan not yet implemented".into()))
}

// === Batch Lookup (TODO) ===

#[derive(Deserialize)]
pub(crate) struct BatchLookupRequest {
    #[allow(dead_code)]
    keys: Vec<HashMap<String, String>>,
}

pub async fn batch_lookup(
    Path((_db, _table)): Path<(String, String)>,
    State(_backend): State<Arc<FlussBackend>>,
    Json(_req): Json<BatchLookupRequest>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    Err(GatewayError::Internal("batch lookup not yet implemented".into()))
}

// === Log Scan ===

pub async fn scan(
    Path((db, table)): Path<(String, String)>,
    State(backend): State<Arc<FlussBackend>>,
    Json(params): Json<ScanParams>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let rows = backend.scan(&db, &table, &params).await?;
    Ok(Json(rows))
}

// === Produce (Write) ===

pub async fn produce(
    Path((db, table)): Path<(String, String)>,
    State(backend): State<Arc<FlussBackend>>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<WriteResult>, GatewayError> {
    let table_info = backend.get_table_info(&db, &table).await?;
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
            backend.delete_rows(&db, &table, rows).await?
        } else {
            backend.upsert_rows(&db, &table, rows).await?
        }
    } else {
        backend.append_rows(&db, &table, rows).await?
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
