use fluss::row::Datum;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Clone, Deserialize)]
pub struct ScanParams {
    pub projection: Option<Vec<usize>>,
    pub limit: Option<usize>,
    pub timeout_ms: Option<u64>,
}

/// Write result response
#[derive(Debug, Serialize)]
pub struct WriteResult {
    pub row_count: usize,
}

/// Produce request body
#[derive(Debug, Deserialize)]
pub struct ProduceRequest {
    pub format: Option<String>,
    pub rows: Vec<ProduceRow>,
}

#[derive(Debug, Deserialize)]
pub struct ProduceRow {
    pub values: Vec<serde_json::Value>,
    pub change_type: Option<String>,
}

// === Gateway Error ===

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("fluss error: {0}")]
    FlussError(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl GatewayError {
    pub fn status_code(&self) -> u16 {
        match self {
            GatewayError::BadRequest(_) => 400,
            GatewayError::NotFound(_) => 404,
            GatewayError::InvalidOperation(_) => 422,
            GatewayError::ConnectionFailed(_) => 502,
            GatewayError::FlussError(_) => 500,
            GatewayError::Internal(_) => 500,
        }
    }

    pub fn error_code(&self) -> u16 {
        match self {
            GatewayError::BadRequest(_) => 40001,
            GatewayError::NotFound(_) => 40401,
            GatewayError::InvalidOperation(_) => 42205,
            GatewayError::ConnectionFailed(_) => 50002,
            GatewayError::FlussError(_) => 50001,
            GatewayError::Internal(_) => 50001,
        }
    }
}

// === Datum <-> JSON conversion ===

pub fn datum_to_json(datum: &Datum) -> serde_json::Value {
    match datum {
        Datum::Null => serde_json::Value::Null,
        Datum::Bool(v) => serde_json::Value::Bool(*v),
        Datum::Int8(v) => serde_json::Value::Number((*v).into()),
        Datum::Int16(v) => serde_json::Value::Number((*v).into()),
        Datum::Int32(v) => serde_json::Value::Number((*v).into()),
        Datum::Int64(v) => serde_json::Value::Number((*v).into()),
        Datum::Float32(v) => serde_json::Value::Number(
            serde_json::Number::from_f64(v.0 as f64).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Datum::Float64(v) => serde_json::Value::Number(
            serde_json::Number::from_f64(v.0).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Datum::String(v) => serde_json::Value::String(v.to_string()),
        Datum::Blob(v) => serde_json::Value::String(base64_encode(v)),
        _ => serde_json::Value::String(format!("{:?}", datum)),
    }
}

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

fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let chunks = data.chunks_exact(3);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        s.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        s.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        s.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        s.push(CHARS[(n & 0x3F) as usize] as char);
    }
    match remainder.len() {
        2 => {
            let n = ((remainder[0] as u32) << 8) | (remainder[1] as u32);
            s.push(CHARS[((n >> 10) & 0x3F) as usize] as char);
            s.push(CHARS[((n >> 4) & 0x3F) as usize] as char);
            s.push(CHARS[((n << 2) & 0x3F) as usize] as char);
            s.push('=');
        }
        1 => {
            let n = remainder[0] as u32;
            s.push(CHARS[((n >> 2) & 0x3F) as usize] as char);
            s.push(CHARS[((n << 4) & 0x3F) as usize] as char);
            s.push_str("==");
        }
        _ => {}
    }
    s
}

// === Unit tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(GatewayError::BadRequest("x".into()).status_code(), 400);
        assert_eq!(GatewayError::NotFound("x".into()).status_code(), 404);
        assert_eq!(GatewayError::InvalidOperation("x".into()).status_code(), 422);
        assert_eq!(GatewayError::ConnectionFailed("x".into()).status_code(), 502);
        assert_eq!(GatewayError::FlussError("x".into()).status_code(), 500);
        assert_eq!(GatewayError::Internal("x".into()).status_code(), 500);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(GatewayError::BadRequest("x".into()).error_code(), 40001);
        assert_eq!(GatewayError::NotFound("x".into()).error_code(), 40401);
        assert_eq!(GatewayError::InvalidOperation("x".into()).error_code(), 42205);
        assert_eq!(GatewayError::ConnectionFailed("x".into()).error_code(), 50002);
        assert_eq!(GatewayError::FlussError("x".into()).error_code(), 50001);
    }

    #[test]
    fn test_datum_to_json_primitives() {
        assert_eq!(datum_to_json(&Datum::Null), serde_json::Value::Null);
        assert_eq!(datum_to_json(&Datum::Bool(true)), serde_json::Value::Bool(true));
        assert_eq!(datum_to_json(&Datum::Int32(42)), serde_json::json!(42));
        assert_eq!(datum_to_json(&Datum::Int64(999)), serde_json::json!(999));
        assert_eq!(
            datum_to_json(&Datum::String("hello".into())),
            serde_json::json!("hello")
        );
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
