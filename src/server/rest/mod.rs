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
    GatewayError, LookupParams, ProduceRequest, ScanParams, WriteResult, json_to_datum,
};

/// Extract credentials from the request based on auth mode.
fn extract_creds(
    auth_type: &AuthType,
    ext: Option<Extension<BasicAuthCredentials>>,
) -> Result<Option<BasicAuthCredentials>, GatewayError> {
    match auth_type {
        AuthType::None => Ok(None),
        AuthType::Passthrough => {
            let ext = ext.ok_or_else(|| GatewayError::Unauthorized("authentication required".into()))?;
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
    let info = state.backend.get_table_info(&db, &table, creds.as_ref()).await?;
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
    Err(GatewayError::Internal("prefix scan not yet implemented".into()))
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
    let rows = state.backend.scan(&db, &table, &params, creds.as_ref()).await?;
    Ok(Json(rows))
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
            state.backend.delete_rows(&db, &table, rows, creds.as_ref()).await?
        } else {
            state.backend.upsert_rows(&db, &table, rows, creds.as_ref()).await?
        }
    } else {
        state.backend.append_rows(&db, &table, rows, creds.as_ref()).await?
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
