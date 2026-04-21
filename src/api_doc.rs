use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

/// Fluss Gateway OpenAPI documentation.
///
/// This API provides a REST interface to Apache Fluss, supporting:
/// - Database and table metadata management
/// - Key-value lookups and batch lookups
/// - Log table scanning
/// - Row production (insert/upsert/delete)
/// - Offset and partition management
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Fluss Gateway API",
        description = "REST API Gateway for Apache Fluss - provides HTTP access to Fluss tables for data operations and metadata management",
        contact(name = "Fluss Gateway", url = "https://github.com/boyu/fluss-gateway"),
        version = "0.1.0"
    ),
    paths(
        crate::server::rest::health,
        crate::server::rest::list_databases,
        crate::server::rest::create_database,
        crate::server::rest::drop_database,
        crate::server::rest::list_tables,
        crate::server::rest::create_table,
        crate::server::rest::table_info_put,
        crate::server::rest::table_info_get,
        crate::server::rest::lookup,
        crate::server::rest::prefix_scan,
        crate::server::rest::batch_lookup,
        crate::server::rest::scan,
        crate::server::rest::produce,
        crate::server::rest::list_offsets,
        crate::server::rest::list_partitions,
        crate::server::rest::drop_table,
    ),
    components(
        schemas(
            crate::types::ScanParams,
            crate::types::WriteResult,
            crate::types::ProduceRequest,
            crate::types::ProduceRow,
            crate::types::CreateDatabaseRequest,
            crate::types::DropDatabaseRequest,
            crate::types::ColumnSpec,
            crate::types::PrimaryKeySpec,
            crate::types::CreateTableRequest,
            crate::types::DropTableRequest,
            crate::types::BucketOffset,
            crate::types::ListOffsetsResponse,
            crate::types::ListPartitionsResponse,
            crate::types::PartitionInfo,
            crate::server::rest::TableInfoResponse,
            crate::server::rest::ColumnInfo,
            crate::server::rest::BatchLookupRequest,
            crate::server::rest::ListOffsetsRequest,
        )
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "databases", description = "Database CRUD operations"),
        (name = "tables", description = "Table CRUD and metadata operations"),
        (name = "lookup", description = "Key-value lookup operations"),
        (name = "scan", description = "Table scan operations"),
        (name = "produce", description = "Data write operations (insert/upsert/delete)"),
        (name = "metadata", description = "Offset and partition metadata operations"),
    ),
    servers(
        (url = "http://localhost:8081", description = "Local development server"),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "basic_auth",
                SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Basic).build()),
            );
        }
    }
}
