# Fluss Gateway Architecture Design

> Extended design based on FIP-32 (Multi-Protocol Query Gateway), adding write capabilities and lessons from Kafka REST Proxy engineering.

## Background & Positioning

### FIP-32 Overview

FIP-32 proposes a **multi-protocol query gateway**: a read-only Rust service based on `fluss-rust` + DataFusion, exposing Fluss tables through four protocols:
- Arrow Flight SQL (9093)
- PostgreSQL Wire Protocol (5432)
- gRPC API (9094)
- REST/HTTP API (8080)

### Relationship Between This Solution and FIP-32

| Dimension | FIP-32 (Query Gateway) | This Solution (REST Gateway Extension) |
|-----------|------------------------|----------------------------------------|
| Scope | Read-only | Read + **Write** |
| REST API | Simple paths `/v1/{db}/{table}` | Full CRUD + streaming consumption |
| Streaming | Not covered | WebSocket/SSE real-time push |
| Rate Limiting & Security | Not covered | Two-layer rate limiting + authentication + audit |
| Write | Deferred to future FIP | **Core goal of this solution** |

**This solution complements FIP-32, not replaces it**.

## Overall Architecture

```
+------------------------------------------------------------------+
|                        Fluss Gateway (Rust)                       |
|                                                                   |
|  +-----------+  +-----------+  +-----------+  +----------------+ |
|  | Flight SQL|  | PostgreSQL|  |   gRPC    |  |   HTTP REST    | |
|  |  (read)    |  |  (read)    |  | (r/w)     |  |  (r/w+stream)  | |
|  +-----+-----+  +-----+-----+  +-----+-----+  +-------+--------+ |
|        |              |              |                |           |
|  +-----+--------------+--------------+----------------+--------+ |
|  |              DataFusion (SQL Query Engine, read path)         | |
|  +-----+--------------+--------------+----------------+--------+ |
|        |              |              |                |           |
|  +-----+--------------+--------------+----------------+--------+ |
|  |              Service / Controller Layer                       | |
|  |  AdminService | ProduceService | ConsumeService | Streaming  | |
|  +-----+--------------+--------------+----------------+--------+ |
|  |              Middleware Layer                                 | |
|  |  Auth | RateLimiter | Metrics | AuditLog | EndpointACL      | |
|  +-----+--------------+--------------+----------------+--------+ |
|  |              Fluss Backend Layer                              | |
|  |  FlussBackend trait + ConnectionPool                          | |
|  +-----+--------------+--------------+----------------+--------+ |
|        |              |              |                |           |
+--------+--------------+--------------+----------------+-----------+
         v              v              v                v
  +-------------+  +-------------+  +-------------+  +-----------+
  | Coordinator |  | TabletServer|  | Schema      |  | Lake      |
  | (Admin GW)  |  | (Data GW)   |  | Registry    |  | Storage   |
  +-------------+  +-------------+  +-------------+  +-----------+
```

## Four-Layer Architecture

1. **Protocol Frontend Layer** - FIP-32 already has four protocols; this solution focuses on extending REST/HTTP and gRPC
2. **Query Engine Layer** (DataFusion) - FIP-32 already has this, handles SQL queries
3. **Service Layer** - Added by this solution, handles write, streaming consumption, and governance logic
4. **Backend Layer** - FIP-32 already has `FlussBackend` trait; this solution extends write methods

## Project Structure

```
fluss-query-gateway/
├── Cargo.toml
├── pom.xml                          # Maven integration
├── build.rs                         # Proto compilation
├── proto/
│   └── gateway.proto                # gRPC service definition
├── src/
│   ├── backend/                     # FIP-32 existing, needs write extension
│   │   ├── mod.rs                   # FlussBackend trait
│   │   ├── native.rs                # NativeFlussBackend
│   │   ├── mock.rs                  # MockFlussBackend
│   │   └── connection_pool.rs       # Connection pool management
│   ├── types/                       # FIP-32 existing
│   │   └── mod.rs                   # Fluss <-> Arrow type mapping
│   ├── catalog/                     # FIP-32 existing
│   │   ├── mod.rs
│   │   ├── catalog_provider.rs      # FlussCatalogProvider
│   │   ├── schema_provider.rs       # FlussSchemaProvider
│   │   ├── kv_table_provider.rs     # FlussKvTableProvider
│   │   └── log_table_provider.rs    # FlussLogTableProvider
│   ├── execution/                   # FIP-32 existing
│   │   ├── mod.rs
│   │   ├── kv_lookup_exec.rs        # FlussKvLookupExec
│   │   ├── kv_prefix_exec.rs        # FlussKvPrefixExec
│   │   └── log_scan_exec.rs         # FlussLogScanExec
│   ├── server/                      # FIP-32 existing + this solution extension
│   │   ├── mod.rs
│   │   ├── flight_sql.rs            # Arrow Flight SQL server
│   │   ├── postgres.rs              # PostgreSQL wire protocol server
│   │   ├── grpc.rs                  # gRPC server (write extension)
│   │   └── rest/                    # REST server (key extension)
│   │       ├── mod.rs               # Route assembly
│   │       ├── health.rs            # /health
│   │       ├── metadata.rs          # /v1/_databases, /v1/{db}/_tables
│   │       ├── lookup.rs            # GET /v1/{db}/{table}?pk.{col}={val}
│   │       ├── scan.rs              # POST /v1/{db}/{table}/scan
│   │       ├── produce.rs           # POST /v1/{db}/{table}/rows (new)
│   │       ├── subscribe.rs         # WS/SSE streaming subscription (new)
│   │       └── admin.rs             # Table/Database CRUD (new)
│   ├── service/                     # Added by this solution
│   │   ├── mod.rs
│   │   ├── produce_service.rs       # Write service (Append/Upsert)
│   │   ├── streaming_service.rs     # Streaming consumption service
│   │   └── admin_service.rs         # Metadata management service
│   ├── middleware/                   # Added by this solution
│   │   ├── mod.rs
│   │   ├── auth.rs                  # Authentication middleware
│   │   ├── rate_limit.rs            # Rate limiting middleware
│   │   ├── metrics.rs               # Prometheus metrics
│   │   └── audit_log.rs             # Audit log
│   ├── harness/                     # FIP-32 existing
│   │   └── mod.rs                   # Runtime utilities
│   └── testing/                     # FIP-32 existing
│       └── mod.rs                   # Test fixtures
├── tests/                           # Integration tests
├── benches/                         # Performance benchmarks
└── docker/                          # Container build
```

## Core Design Points

### FlussBackend Trait Extension

```rust
#[async_trait]
pub trait FlussBackend: Send + Sync {
    // === Read operations (FIP-32 existing) ===
    async fn lookup(&self, table: &str, key_values: &[ScalarValue])
        -> Result<RecordBatch, BackendError>;
    async fn batch_lookup(&self, table: &str, keys: &[Vec<ScalarValue>])
        -> Result<RecordBatch, BackendError>;
    async fn prefix_lookup(&self, table: &str, prefix: &[u8], limit: Option<usize>)
        -> Result<RecordBatch, BackendError>;
    async fn log_scan(&self, table: &str, params: &LogSCANParams)
        -> Result<RecordBatch, BackendError>;
    async fn get_schema(&self, database: &str, table: &str)
        -> Result<Schema, BackendError>;
    async fn list_databases(&self) -> Result<Vec<String>, BackendError>;
    async fn list_tables(&self, database: &str) -> Result<Vec<String>, BackendError>;
    async fn health_check(&self) -> Result<bool, BackendError>;
    async fn get_table_type(&self, database: &str, table: &str)
        -> Result<FlussTableType, BackendError>;

    // === Write operations (added by this solution) ===
    async fn append_rows(&self, table: &str, rows: &[Row])
        -> Result<ProduceResult, BackendError>;
    async fn upsert_rows(&self, table: &str, rows: &[RowWithChange])
        -> Result<ProduceResult, BackendError>;

    // === Metadata management (added by this solution) ===
    async fn create_database(&self, name: &str, descriptor: &DatabaseDescriptor)
        -> Result<(), BackendError>;
    async fn drop_database(&self, name: &str, cascade: bool)
        -> Result<(), BackendError>;
    async fn create_table(&self, path: &str, descriptor: &TableDescriptor)
        -> Result<(), BackendError>;
    async fn drop_table(&self, path: &str)
        -> Result<(), BackendError>;
    async fn alter_table(&self, path: &str, changes: &[TableChange])
        -> Result<(), BackendError>;

    // === Streaming consumption (added by this solution) ===
    async fn subscribe_log(&self, table: &str, offset: i64)
        -> Result<RecordStream, BackendError>;
}
```

### Rate Limiting Strategy

**Two-layer rate limiting** (referencing Kafka REST experience):
- **Tier 1**: General API rate limiting (Tower + governor token bucket) - all endpoints
- **Tier 2**: Produce-specific four-dimensional rate limiting - global request count, global byte count, per-DB request count, per-DB byte count

### Error Code System

Follows `HTTP Status Code + 2-digit suffix` pattern:

| Error Code | Meaning |
|------------|---------|
| 40401 | Table not found |
| 40402 | Database not found |
| 40901 | Table already exists |
| 40902 | Database already exists |
| 42201 | Invalid request payload |
| 42202 | Schema validation failed |
| 42205 | Operation not allowed |
| 42901 | Rate limit exceeded |
| 50001 | Fluss Internal error |
| 50002 | Connection error |

### Kafka REST Pitfalls & Mitigations

| Kafka's Problem | Fluss Gateway's Mitigation |
|-----------------|---------------------------|
| Stateful consumers | Stateless Scan API + long-lived streaming connections |
| Non-pooled connections | FlussConnection connection pool (DashMap<NodeId, Connection>) |
| Consumer memory leaks | No persistent consumer registry, WS disconnect auto-cleanup |
| Incomplete rate limiting | Full-endpoint Tower rate limiting + Produce four-dimensional limiting |
| Inconsistent error handling | Unified GatewayError type, implements IntoResponse |
| Single Producer | Uses fluss-rust WriterClient (internal Sender + RecordAccumulator) |

## Phased Implementation Plan

### Phase 1: Basic Framework + Read (Current)
- Set up project structure in `fluss-query-gateway/`
- Reuse FIP-32's `FlussBackend` trait (read methods already implemented)
- Implement connection pool management `ConnectionPool`
- Reuse FIP-32's REST read endpoints (KV point lookup, prefix scan, log table scan)
- **Deliverable**: Compilable, supports reading Fluss data via REST API

### Phase 2: Write Path (Core)
- Extend `FlussBackend` trait with `append_rows` / `upsert_rows` methods
- Implement `NativeFlussBackend` write path (based on fluss-rust WriterClient)
- Implement `ProduceService` (write service, encapsulates WriterClient)
- JSON deserializer (values -> Fluss Row)
- Arrow IPC deserializer
- Auto-route by table type (LOG table -> append, PK table -> upsert)
- REST endpoint: `POST /v1/{db}/{table}/rows`
- **Deliverable**: Complete write API

### Phase 3: Authentication - Identity Penetration (Security)
- Gateway acts as protocol bridge, no independent auth system
- Clients pass username/password via HTTP Basic Auth
- Gateway uses these credentials to connect to Fluss via SASL/PLAIN
- Permission control entirely on Fluss side (ZooKeeper ACL)
- Configuration: `[auth] type = "none" | "http_basic"`
- **Deliverable**: Identity penetration middleware + configuration

### Phase 4: Refinement & Testing
- Supplement write unit tests using `MockFlussBackend`
- Integration tests (end-to-end read/write verification)
- Error code system refinement
- **Deliverable**: Publishable read/write + auth Gateway

---

## Deferred Items (Future Iterations)

| Item | Reason |
|------|--------|
| Streaming consumption (WebSocket/SSE) | Non-core requirement, read/write first |
| Metadata management (DB/Table CRUD) | Non-core requirement |
| Rate limiting (Tier 1 + Tier 2) | Added when production environment needs it |
| Prometheus metrics | Added when monitoring needs it |
| Audit logging | Added when compliance needs it |
| gRPC write extension | REST first, gRPC later |

---

## Related Codebase References

### Fluss (Java) - `/Users/boyu/IdeaProjects/fluss-community/`

| Module | Key Path | Purpose |
|--------|----------|---------|
| fluss-rpc | `fluss-rpc/src/main/proto/FlussApi.proto` | RPC protocol definition (includes AuthenticateRequest/Response) |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/server/NettyServerHandler.java` | Auth handshake flow |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/client/NettyClient.java` | Client auth flow |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/server/Session.java` | Session encapsulates FlussPrincipal |
| fluss-common | `fluss-common/src/main/java/.../security/auth/` | Auth plugin system (SPI) |
| fluss-common | `fluss-common/src/main/java/.../security/auth/sasl/authenticator/` | SASL/PLAIN implementation |
| fluss-common | `fluss-common/src/main/java/.../security/acl/FlussPrincipal.java` | Principal definition |
| fluss-server | `fluss-server/src/main/java/.../server/authorizer/` | ACL authorization (DefaultAuthorizer) |
| fluss-client | `fluss-client/src/main/java/.../client/` | Java client entry point |
| fluss-client | `fluss-client/src/main/java/.../client/connection.rs` | FlussConnection (Rust reference) |
| fluss-client | `fluss-client/src/main/java/.../client/admin.rs` | FlussAdmin metadata operations |
| fluss-client | `fluss-client/src/main/java/.../client/table/scanner.rs` | LogScanner consumption primitives |
| fluss-client | `fluss-client/src/main/java/.../client/write/` | WriterClient write path |

### fluss-rust (Rust) - `/Users/boyu/VscodeProjects/fluss-rust/`

| Module | Key Path | Purpose |
|--------|----------|---------|
| fluss | `crates/fluss/src/client/` | Rust client entry point |
| fluss | `crates/fluss/src/client/connection.rs` | FlussConnection entry point |
| fluss | `crates/fluss/src/client/admin.rs` | FlussAdmin metadata operations |
| fluss | `crates/fluss/src/client/table/scanner.rs` | LogScanner consumption primitives |
| fluss | `crates/fluss/src/client/table/mod.rs` | FlussTable entry |
| fluss | `crates/fluss/src/client/write/` | WriterClient write path |

### DataFusion (Rust) - `/Users/boyu/VscodeProjects/datafusion/`

| Module | Key Path | Purpose |
|--------|----------|---------|
| core | `datafusion/core/` | DataFusion core engine |
| catalog | `datafusion/core/src/catalog/` | Catalog abstraction (FIP-32 extends FlussCatalog here) |
| execution | `datafusion/core/src/execution/` | Execution engine |
| physical-plan | `datafusion/core/src/physical_plan/` | Physical execution plan (reference for FIP-32 custom ExecutionPlan) |
| examples | `datafusion-examples/` | Usage examples |
