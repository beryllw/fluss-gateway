use fluss::row::Datum;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

// === Request/Response DTOs ===

/// Lookup parameters: pk column name -> value (from query string)
#[derive(Debug, Clone)]
pub struct LookupParams {
    params: HashMap<String, String>,
}

impl LookupParams {
    pub fn new(params: HashMap<String, String>) -> Self {
        Self { params }
    }

    pub fn get(&self, col: &str) -> Option<&str> {
        self.params.get(col).map(|s| s.as_str())
    }
}

/// Scan parameters (JSON body)
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ScanParams {
    /// Column indices to include in the result
    pub projection: Option<Vec<usize>>,
    /// Maximum number of rows to return
    pub limit: Option<usize>,
    /// Request timeout in milliseconds
    pub timeout_ms: Option<u64>,
}

/// Write result response
#[derive(Debug, Serialize, ToSchema)]
pub struct WriteResult {
    /// Number of rows affected
    pub row_count: usize,
}

/// Produce request body
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProduceRequest {
    /// Data format (optional)
    #[allow(dead_code)]
    pub format: Option<String>,
    /// Rows to write
    pub rows: Vec<ProduceRow>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProduceRow {
    /// Field values for this row
    pub values: Vec<serde_json::Value>,
    /// Change type: "Insert" or "Delete"
    pub change_type: Option<String>,
}

// === Metadata Management DTOs ===

/// Create database request body
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateDatabaseRequest {
    /// Name of the database to create
    pub database_name: String,
    /// Optional comment for the database
    pub comment: Option<String>,
    /// Custom properties for the database
    #[serde(default)]
    pub custom_properties: std::collections::HashMap<String, String>,
    /// If true, do not fail if the database already exists
    #[serde(default)]
    pub ignore_if_exists: bool,
}

/// Drop database request body
#[derive(Debug, Deserialize, ToSchema)]
pub struct DropDatabaseRequest {
    /// If true, do not fail if the database does not exist
    #[serde(default)]
    pub ignore_if_not_exists: bool,
    /// If true, also drop all tables in the database
    #[serde(default)]
    pub cascade: bool,
}

/// Column specification for table creation
#[derive(Debug, Deserialize, ToSchema)]
pub struct ColumnSpec {
    /// Column name
    pub name: String,
    /// Data type (e.g., "int", "bigint", "string")
    pub data_type: String,
    /// Optional column comment
    pub comment: Option<String>,
}

/// Primary key specification
#[derive(Debug, Deserialize, ToSchema)]
pub struct PrimaryKeySpec {
    /// Optional constraint name
    pub constraint_name: Option<String>,
    /// Column names that form the primary key
    pub column_names: Vec<String>,
}

/// Create table request body
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTableRequest {
    /// Name of the table to create
    pub table_name: String,
    /// Column definitions
    pub schema: Vec<ColumnSpec>,
    /// Primary key definition
    pub primary_key: Option<PrimaryKeySpec>,
    /// Partition key column names
    pub partition_keys: Option<Vec<String>>,
    /// Number of buckets
    pub bucket_count: Option<i32>,
    /// Bucket key column names
    pub bucket_keys: Option<Vec<String>>,
    /// Table properties
    pub properties: Option<std::collections::HashMap<String, String>>,
    /// Optional table comment
    pub comment: Option<String>,
    /// If true, do not fail if the table already exists
    #[serde(default)]
    pub ignore_if_exists: bool,
}

/// Drop table request body
#[derive(Debug, Deserialize, ToSchema)]
pub struct DropTableRequest {
    /// If true, do not fail if the table does not exist
    #[serde(default)]
    pub ignore_if_not_exists: bool,
}

/// Offset information for a specific bucket
#[derive(Debug, Serialize, ToSchema)]
pub struct BucketOffset {
    /// Bucket ID
    pub bucket_id: i32,
    /// Offset value
    pub offset: i64,
}

/// List offsets response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListOffsetsResponse {
    /// Table path in format "database/table"
    pub table_path: String,
    /// Offset specification: "earliest", "latest", or "timestamp"
    pub spec: String,
    /// List of bucket offsets
    pub offsets: Vec<BucketOffset>,
}

/// List partitions response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListPartitionsResponse {
    /// Table path in format "database/table"
    pub table_path: String,
    /// List of partition information
    pub partitions: Vec<PartitionInfo>,
}

/// Partition information
#[derive(Debug, Serialize, ToSchema)]
pub struct PartitionInfo {
    /// Partition ID
    pub partition_id: i64,
    /// Partition name
    pub partition_name: String,
    /// Partition specification key-value pairs
    pub partition_spec: std::collections::HashMap<String, String>,
}

// === Gateway Error ===

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("fluss error: {0}")]
    FlussError(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("payload too large: request body exceeds the {limit} byte limit")]
    BodyLimitTooLarge { limit: usize },
}

impl GatewayError {
    pub fn status_code(&self) -> u16 {
        match self {
            GatewayError::BadRequest(_) => 400,
            GatewayError::InvalidOperation(_) => 422,
            GatewayError::FlussError(_) => 500,
            GatewayError::Internal(_) => 500,
            GatewayError::Unauthorized(_) => 401,
            GatewayError::BodyLimitTooLarge { .. } => 413,
        }
    }

    pub fn error_code(&self) -> u16 {
        match self {
            GatewayError::BadRequest(_) => 40001,
            GatewayError::InvalidOperation(_) => 42205,
            GatewayError::FlussError(_) => 50001,
            GatewayError::Internal(_) => 50001,
            GatewayError::Unauthorized(_) => 40101,
            GatewayError::BodyLimitTooLarge { .. } => 41301,
        }
    }
}

// === JSON -> Datum conversion ===

pub fn json_to_datum(
    value: &serde_json::Value,
    data_type: &fluss::metadata::DataType,
) -> Result<Datum<'static>, String> {
    use fluss::metadata::DataType;

    match (value, data_type) {
        (serde_json::Value::Null, _) => Ok(Datum::Null),
        (serde_json::Value::Bool(v), DataType::Boolean(_)) => Ok(Datum::Bool(*v)),
        (serde_json::Value::Number(n), DataType::TinyInt(_)) => n
            .as_i64()
            .map(|v| Datum::Int8(v as i8))
            .ok_or_else(|| format!("expected tinyint, got {}", n)),
        (serde_json::Value::Number(n), DataType::SmallInt(_)) => n
            .as_i64()
            .map(|v| Datum::Int16(v as i16))
            .ok_or_else(|| format!("expected smallint, got {}", n)),
        (serde_json::Value::Number(n), DataType::Int(_)) => n
            .as_i64()
            .map(|v| Datum::Int32(v as i32))
            .ok_or_else(|| format!("expected int, got {}", n)),
        (serde_json::Value::Number(n), DataType::BigInt(_)) => n
            .as_i64()
            .map(Datum::Int64)
            .ok_or_else(|| format!("expected bigint, got {}", n)),
        (serde_json::Value::Number(n), DataType::Float(_)) => n
            .as_f64()
            .map(|v| Datum::Float32(OrderedFloat(v as f32)))
            .ok_or_else(|| format!("expected float, got {}", n)),
        (serde_json::Value::Number(n), DataType::Double(_)) => n
            .as_f64()
            .map(|v| Datum::Float64(OrderedFloat(v)))
            .ok_or_else(|| format!("expected double, got {}", n)),
        (serde_json::Value::String(s), DataType::String(_) | DataType::Char(_)) => {
            Ok(Datum::String(s.clone().into()))
        }
        (serde_json::Value::String(s), DataType::Bytes(_) | DataType::Binary(_)) => {
            Ok(Datum::Blob(s.as_bytes().to_vec().into()))
        }
        (serde_json::Value::String(s), DataType::Int(_)) => s
            .parse::<i32>()
            .map(Datum::Int32)
            .map_err(|e| format!("invalid int '{}': {}", s, e)),
        (serde_json::Value::String(s), DataType::BigInt(_)) => s
            .parse::<i64>()
            .map(Datum::Int64)
            .map_err(|e| format!("invalid bigint '{}': {}", s, e)),
        (serde_json::Value::String(s), DataType::Boolean(_)) => s
            .parse::<bool>()
            .map(Datum::Bool)
            .map_err(|e| format!("invalid bool '{}': {}", s, e)),
        (serde_json::Value::String(s), DataType::Float(_)) => s
            .parse::<f32>()
            .map(|v| Datum::Float32(OrderedFloat(v)))
            .map_err(|e| format!("invalid float '{}': {}", s, e)),
        (serde_json::Value::String(s), DataType::Double(_)) => s
            .parse::<f64>()
            .map(|v| Datum::Float64(OrderedFloat(v)))
            .map_err(|e| format!("invalid double '{}': {}", s, e)),
        (v, dt) => Err(format!("cannot convert {:?} to {:?}", v, dt)),
    }
}

// === Unit tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(GatewayError::BadRequest("x".into()).status_code(), 400);
        assert_eq!(
            GatewayError::InvalidOperation("x".into()).status_code(),
            422
        );
        assert_eq!(GatewayError::FlussError("x".into()).status_code(), 500);
        assert_eq!(GatewayError::Internal("x".into()).status_code(), 500);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(GatewayError::BadRequest("x".into()).error_code(), 40001);
        assert_eq!(
            GatewayError::InvalidOperation("x".into()).error_code(),
            42205
        );
        assert_eq!(GatewayError::FlussError("x".into()).error_code(), 50001);
    }

    #[test]
    fn test_json_to_datum_int() {
        use fluss::metadata::DataTypes;
        let dt = DataTypes::int();
        let d = json_to_datum(&serde_json::json!(42), &dt).unwrap();
        assert!(matches!(d, Datum::Int32(42)));

        // String coercion
        let d = json_to_datum(&serde_json::json!("42"), &dt).unwrap();
        assert!(matches!(d, Datum::Int32(42)));
    }

    #[test]
    fn test_json_to_datum_bigint() {
        use fluss::metadata::DataTypes;
        let dt = DataTypes::bigint();
        let d = json_to_datum(&serde_json::json!(9999999999i64), &dt).unwrap();
        assert!(matches!(d, Datum::Int64(9999999999)));
    }

    #[test]
    fn test_json_to_datum_string() {
        use fluss::metadata::DataTypes;
        let dt = DataTypes::string();
        let d = json_to_datum(&serde_json::json!("hello"), &dt).unwrap();
        assert!(matches!(d, Datum::String(_)));
    }

    #[test]
    fn test_json_to_datum_null() {
        use fluss::metadata::DataTypes;
        let dt = DataTypes::int();
        let d = json_to_datum(&serde_json::Value::Null, &dt).unwrap();
        assert!(matches!(d, Datum::Null));
    }

    #[test]
    fn test_json_to_datum_type_mismatch() {
        use fluss::metadata::DataTypes;
        let dt = DataTypes::int();
        let err = json_to_datum(&serde_json::json!(["array"]), &dt);
        assert!(err.is_err());
    }

    #[test]
    fn test_lookup_params() {
        let mut map = HashMap::new();
        map.insert("id".to_string(), "42".to_string());
        let params = LookupParams::new(map);
        assert_eq!(params.get("id"), Some("42"));
        assert_eq!(params.get("name"), None);
    }
}
