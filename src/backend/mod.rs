use fluss::client::FlussConnection;
use fluss::metadata::TablePath;
use fluss::row::GenericRow;
use std::sync::Arc;

use crate::config::AuthConfig;
use crate::pool::ConnectionPool;
use crate::server::auth::BasicAuthCredentials;
use crate::types::{GatewayError, LookupParams, ScanParams, WriteResult, json_to_datum};

/// FlussBackend wraps a ConnectionPool and exposes high-level operations
/// for the REST Gateway. Each method takes per-request credentials to
/// retrieve the appropriate FlussConnection from the pool.
pub struct FlussBackend {
    pool: Arc<ConnectionPool>,
    auth: AuthConfig,
}

impl FlussBackend {
    pub fn pool(&self) -> &Arc<ConnectionPool> {
        &self.pool
    }
    pub async fn new(
        coordinator: &str,
        auth: AuthConfig,
        pool_config: crate::config::PoolConfig,
    ) -> Result<Self, GatewayError> {
        let pool = ConnectionPool::new(coordinator, auth.clone(), pool_config);
        // Warm up: create the default connection using startup credentials
        pool.get_or_create(None).await;
        Ok(Self {
            pool: Arc::new(pool),
            auth,
        })
    }

    /// Get a FlussConnection for the given credentials. When `creds` is `None`,
    /// uses the startup credentials configured for the backend.
    async fn conn(
        &self,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Arc<FlussConnection>, GatewayError> {
        let (username, password) = match creds {
            Some(c) => (c.username.clone(), c.password.clone()),
            None => (self.auth.startup_username.clone(), self.auth.startup_password.clone()),
        };
        Ok(self.pool.get_or_create(Some((&username, &password))).await)
    }

    // === Metadata operations ===

    pub async fn list_databases(
        &self,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Vec<String>, GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        admin.list_databases().await.map_err(fluss_err)
    }

    pub async fn list_tables(
        &self,
        db: &str,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Vec<String>, GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        admin.list_tables(db).await.map_err(fluss_err)
    }

    pub async fn get_table_info(
        &self,
        db: &str,
        table: &str,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<fluss::metadata::TableInfo, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;
        Ok(fluss_table.get_table_info().clone())
    }

    // === Metadata management operations ===

    pub async fn create_database(
        &self,
        db_name: &str,
        comment: Option<&str>,
        custom_properties: &std::collections::HashMap<String, String>,
        ignore_if_exists: bool,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<(), GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;

        let mut builder = fluss::metadata::DatabaseDescriptor::builder();
        if let Some(c) = comment {
            builder = builder.comment(c);
        }
        if !custom_properties.is_empty() {
            builder = builder.custom_properties(custom_properties.clone());
        }
        let descriptor = builder.build();

        admin
            .create_database(db_name, Some(&descriptor), ignore_if_exists)
            .await
            .map_err(fluss_err)
    }

    pub async fn drop_database(
        &self,
        db_name: &str,
        ignore_if_not_exists: bool,
        cascade: bool,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<(), GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        admin
            .drop_database(db_name, ignore_if_not_exists, cascade)
            .await
            .map_err(fluss_err)
    }

    pub async fn create_table(
        &self,
        db: &str,
        table: &str,
        schema: fluss::metadata::Schema,
        partition_keys: Vec<String>,
        bucket_count: Option<i32>,
        bucket_keys: Vec<String>,
        properties: std::collections::HashMap<String, String>,
        comment: Option<String>,
        ignore_if_exists: bool,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<(), GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        let table_path = TablePath::new(db, table);

        let mut builder = fluss::metadata::TableDescriptor::builder()
            .schema(schema)
            .properties(properties);

        if !partition_keys.is_empty() {
            builder = builder.partitioned_by(partition_keys);
        }

        builder = builder.distributed_by(bucket_count, bucket_keys);

        if let Some(c) = comment {
            builder = builder.comment(c);
        }

        let descriptor = builder.build().map_err(fluss_err)?;
        admin
            .create_table(&table_path, &descriptor, ignore_if_exists)
            .await
            .map_err(fluss_err)
    }

    pub async fn drop_table(
        &self,
        db: &str,
        table: &str,
        ignore_if_not_exists: bool,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<(), GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        let table_path = TablePath::new(db, table);
        admin
            .drop_table(&table_path, ignore_if_not_exists)
            .await
            .map_err(fluss_err)
    }

    pub async fn list_offsets(
        &self,
        db: &str,
        table: &str,
        buckets: &[i32],
        spec: fluss::rpc::message::OffsetSpec,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<std::collections::HashMap<i32, i64>, GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        let table_path = TablePath::new(db, table);
        admin
            .list_offsets(&table_path, buckets, spec)
            .await
            .map_err(fluss_err)
    }

    pub async fn list_partitions(
        &self,
        db: &str,
        table: &str,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Vec<fluss::metadata::PartitionInfo>, GatewayError> {
        let conn = self.conn(creds).await?;
        let admin = conn.get_admin().map_err(fluss_err)?;
        let table_path = TablePath::new(db, table);
        admin
            .list_partition_infos(&table_path)
            .await
            .map_err(fluss_err)
    }

    // === KV Lookup (point query on PK table) ===
    pub async fn lookup(
        &self,
        db: &str,
        table: &str,
        params: &LookupParams,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Vec<serde_json::Value>, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;

        if !fluss_table.has_primary_key() {
            return Err(GatewayError::InvalidOperation(
                "lookup requires a primary key table".into(),
            ));
        }

        let table_info = fluss_table.get_table_info();
        let schema = &table_info.schema;
        let columns = schema.columns();
        let pk = schema
            .primary_key()
            .ok_or_else(|| GatewayError::InvalidOperation("no primary key defined".into()))?;

        let pk_col_names = pk.column_names();
        let mut row = GenericRow::new(columns.len());

        for col_name in pk_col_names {
            let value = params.get(col_name).ok_or_else(|| {
                GatewayError::BadRequest(format!("missing pk column: {}", col_name))
            })?;
            let col = columns.iter().find(|c| c.name() == col_name).ok_or_else(|| {
                GatewayError::BadRequest(format!("unknown column: {}", col_name))
            })?;
            let datum = json_to_datum(
                &serde_json::Value::String(value.to_string()),
                col.data_type(),
            )
            .map_err(GatewayError::BadRequest)?;
            let field_idx = columns.iter().position(|c| c.name() == col_name).unwrap();
            row.set_field(field_idx, datum);
        }

        let mut lookuper = fluss_table
            .new_lookup()
            .map_err(fluss_err)?
            .create_lookuper()
            .map_err(fluss_err)?;

        let result = lookuper.lookup(&row).await.map_err(fluss_err)?;
        let batch = result.to_record_batch().map_err(fluss_err)?;

        Ok(record_batch_to_json(&batch))
    }

    // === Log Scan ===
    pub async fn scan(
        &self,
        db: &str,
        table: &str,
        params: &ScanParams,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<Vec<serde_json::Value>, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;

        let mut table_scan = fluss_table.new_scan();

        if let Some(projection) = &params.projection {
            table_scan = table_scan.project(projection).map_err(fluss_err)?;
        }

        let scanner = table_scan.create_record_batch_log_scanner().map_err(fluss_err)?;

        // Subscribe to all buckets from earliest offset
        let table_info = fluss_table.get_table_info();
        let num_buckets = table_info.num_buckets;
        for bucket_id in 0..num_buckets {
            scanner
                .subscribe(bucket_id, fluss::client::EARLIEST_OFFSET)
                .await
                .map_err(fluss_err)?;
        }

        let timeout = std::time::Duration::from_millis(params.timeout_ms.unwrap_or(5000));
        let batches = scanner.poll(timeout).await.map_err(fluss_err)?;

        let mut output = Vec::new();
        for scan_batch in batches {
            let batch = scan_batch.into_batch();
            let mut rows = record_batch_to_json(&batch);
            output.append(&mut rows);
        }

        if let Some(limit) = params.limit {
            output.truncate(limit);
        }

        Ok(output)
    }

    // === Write operations ===

    pub async fn append_rows(
        &self,
        db: &str,
        table: &str,
        rows: Vec<GenericRow<'_>>,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<WriteResult, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;

        if fluss_table.has_primary_key() {
            return Err(GatewayError::InvalidOperation(
                "cannot append to a primary key table, use upsert instead".into(),
            ));
        }

        let appender = fluss_table
            .new_append()
            .map_err(fluss_err)?
            .create_writer()
            .map_err(fluss_err)?;

        for row in &rows {
            appender.append(row).map_err(fluss_err)?;
        }
        appender.flush().await.map_err(fluss_err)?;

        Ok(WriteResult {
            row_count: rows.len(),
        })
    }

    pub async fn upsert_rows(
        &self,
        db: &str,
        table: &str,
        rows: Vec<GenericRow<'_>>,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<WriteResult, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;

        if !fluss_table.has_primary_key() {
            return Err(GatewayError::InvalidOperation(
                "cannot upsert to a log table, use append instead".into(),
            ));
        }

        let upserter = fluss_table
            .new_upsert()
            .map_err(fluss_err)?
            .create_writer()
            .map_err(fluss_err)?;

        for row in &rows {
            upserter.upsert(row).map_err(fluss_err)?;
        }
        upserter.flush().await.map_err(fluss_err)?;

        Ok(WriteResult {
            row_count: rows.len(),
        })
    }

    pub async fn delete_rows(
        &self,
        db: &str,
        table: &str,
        rows: Vec<GenericRow<'_>>,
        creds: Option<&BasicAuthCredentials>,
    ) -> Result<WriteResult, GatewayError> {
        let conn = self.conn(creds).await?;
        let table_path = TablePath::new(db, table);
        let fluss_table = conn.get_table(&table_path).await.map_err(fluss_err)?;

        if !fluss_table.has_primary_key() {
            return Err(GatewayError::InvalidOperation(
                "cannot delete from a log table".into(),
            ));
        }

        let upserter = fluss_table
            .new_upsert()
            .map_err(fluss_err)?
            .create_writer()
            .map_err(fluss_err)?;

        for row in &rows {
            upserter.delete(row).map_err(fluss_err)?;
        }
        upserter.flush().await.map_err(fluss_err)?;

        Ok(WriteResult {
            row_count: rows.len(),
        })
    }
}

// === Arrow RecordBatch -> JSON ===

fn record_batch_to_json(batch: &arrow::array::RecordBatch) -> Vec<serde_json::Value> {
    let column_names: Vec<String> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();

    let mut output = Vec::with_capacity(batch.num_rows());
    for row_idx in 0..batch.num_rows() {
        let mut map = serde_json::Map::with_capacity(column_names.len());
        for (col_idx, name) in column_names.iter().enumerate() {
            let col = batch.column(col_idx);
            let val = arrow_value_to_json(col.as_ref(), row_idx);
            map.insert(name.clone(), val);
        }
        output.push(serde_json::Value::Object(map));
    }
    output
}

fn arrow_value_to_json(array: &dyn arrow::array::Array, idx: usize) -> serde_json::Value {
    use arrow::array::*;
    use arrow::datatypes::DataType as ArrowDataType;

    if array.is_null(idx) {
        return serde_json::Value::Null;
    }

    match array.data_type() {
        ArrowDataType::Boolean => serde_json::Value::Bool(
            array.as_any().downcast_ref::<BooleanArray>().unwrap().value(idx),
        ),
        ArrowDataType::Int8 => serde_json::Value::Number(
            array.as_any().downcast_ref::<Int8Array>().unwrap().value(idx).into(),
        ),
        ArrowDataType::Int16 => serde_json::Value::Number(
            array.as_any().downcast_ref::<Int16Array>().unwrap().value(idx).into(),
        ),
        ArrowDataType::Int32 => serde_json::Value::Number(
            array.as_any().downcast_ref::<Int32Array>().unwrap().value(idx).into(),
        ),
        ArrowDataType::Int64 => serde_json::Value::Number(
            array.as_any().downcast_ref::<Int64Array>().unwrap().value(idx).into(),
        ),
        ArrowDataType::Float32 => serde_json::Value::Number(
            serde_json::Number::from_f64(
                array.as_any().downcast_ref::<Float32Array>().unwrap().value(idx) as f64,
            )
            .unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        ArrowDataType::Float64 => serde_json::Value::Number(
            serde_json::Number::from_f64(
                array.as_any().downcast_ref::<Float64Array>().unwrap().value(idx),
            )
            .unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => {
            let s = match array.data_type() {
                ArrowDataType::Utf8 => array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(idx),
                ArrowDataType::LargeUtf8 => array
                    .as_any()
                    .downcast_ref::<LargeStringArray>()
                    .unwrap()
                    .value(idx),
                _ => unreachable!(),
            };
            serde_json::Value::String(s.to_string())
        }
        ArrowDataType::Binary | ArrowDataType::LargeBinary => {
            let bytes = match array.data_type() {
                ArrowDataType::Binary => array
                    .as_any()
                    .downcast_ref::<BinaryArray>()
                    .unwrap()
                    .value(idx),
                ArrowDataType::LargeBinary => array
                    .as_any()
                    .downcast_ref::<LargeBinaryArray>()
                    .unwrap()
                    .value(idx),
                _ => unreachable!(),
            };
            serde_json::Value::String(hex_encode(bytes))
        }
        _ => serde_json::Value::String(format!("{:?}", array)),
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn fluss_err(e: fluss::error::Error) -> GatewayError {
    GatewayError::FlussError(e.to_string())
}

// === Unit tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fluss_err_mapping() {
        let e = fluss::error::Error::IllegalArgument {
            message: "test".into(),
        };
        let ge = fluss_err(e);
        assert!(matches!(ge, GatewayError::FlussError(_)));
        assert_eq!(ge.status_code(), 500);
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0xde, 0xad]), "dead");
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_record_batch_to_json_empty() {
        use arrow::array::Int32Array;
        use arrow::datatypes::{DataType, Field, Schema};
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, true)]));
        let batch = arrow::array::RecordBatch::new_empty(schema);
        let result = record_batch_to_json(&batch);
        assert!(result.is_empty());
    }

    #[test]
    fn test_record_batch_to_json_with_data() {
        use arrow::array::{Int32Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, true),
            Field::new("name", DataType::Utf8, true),
        ]));
        let id_array = Int32Array::from(vec![1, 2]);
        let name_array = StringArray::from(vec!["Alice", "Bob"]);
        let batch = arrow::array::RecordBatch::try_new(
            schema,
            vec![Arc::new(id_array), Arc::new(name_array)],
        )
        .unwrap();
        let result = record_batch_to_json(&batch);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["id"], serde_json::json!(1));
        assert_eq!(result[0]["name"], serde_json::json!("Alice"));
        assert_eq!(result[1]["id"], serde_json::json!(2));
        assert_eq!(result[1]["name"], serde_json::json!("Bob"));
    }
}
