# Fluss Gateway Project Progress

> Last updated: 2026-04-05 10:00

## Overall Progress

| Phase | Content | Status | Notes |
|-------|---------|--------|-------|
| Phase 1 | Basic Framework + Read | ✅ Complete | All read endpoints implemented |
| Phase 2 | Write Path | ✅ Complete | append/upsert/delete implemented |
| Phase 3a | Fix integration tests + verify write | ✅ Complete | 12/12 integration tests passing |
| Phase 3b | Auth identity penetration refactor | ✅ Complete | moka connection cache + config file parsing |
| Phase 4 | Deployment improvements | ✅ Complete | Docker + systemd + ops scripts |
| Phase 5 | Ops CLI + graceful shutdown | ✅ Complete | `serve` subcommand + shell script lifecycle management |
| Phase 6 | Chinese documentation | ✅ Complete | Chinese README + API docs + deployment docs |
| Phase 7 | Metadata management (core requirement) | ✅ Complete | Database/Table CRUD, partition management, offset queries |
| Phase 8 | Pre-built release packages | 🔲 Pending | GitHub Releases binaries, Docker Hub images |
| Phase 9 | Monitoring & observability | 🔲 Pending | Prometheus metrics, audit logging, structured logging |
| Phase 10 | Streaming consumption | 🔲 Pending | SSE/WebSocket streaming consumption, offset management |

---

## Implemented Feature List

### Backend Layer (`src/backend/mod.rs`)

| Method | Type | Status |
|--------|------|--------|
| `list_databases` | Metadata read | ✅ |
| `list_tables` | Metadata read | ✅ |
| `get_table_info` | Metadata read | ✅ |
| `create_database` | Metadata write | ✅ |
| `drop_database` | Metadata write | ✅ |
| `create_table` | Metadata write | ✅ |
| `drop_table` | Metadata write | ✅ |
| `alter_table` | Metadata write | ❌ fluss-rust not supported |
| `list_offsets` | Metadata read | ✅ |
| `list_partitions` | Metadata read | ✅ |
| `lookup` | KV point lookup | ✅ |
| `scan` | Log scan | ✅ |
| `append_rows` | Write (Log table) | ✅ |
| `upsert_rows` | Write (PK table) | ✅ |
| `delete_rows` | Delete (PK table) | ✅ |
| `prefix_lookup` | KV prefix scan | ❌ stub (returns 500) |

### REST Endpoints

| Method | Path | Status |
|--------|------|--------|
| GET | `/health` | ✅ |
| GET | `/v1/_databases` | ✅ |
| GET | `/v1/{db}/_tables` | ✅ |
| GET | `/v1/{db}/{table}/_info` | ✅ |
| GET | `/v1/{db}/{table}?pk.col=val` | ✅ |
| GET | `/v1/{db}/{table}/prefix` | ❌ stub |
| POST | `/v1/{db}/{table}/batch` | ✅ |
| POST | `/v1/{db}/{table}/scan` | ✅ |
| POST | `/v1/{db}/{table}/rows` | ✅ |
| POST | `/v1/_databases` | ✅ Create database |
| DELETE | `/v1/_databases/{db}` | ✅ Delete database |
| POST | `/v1/{db}/_tables` | ✅ Create table |
| PUT | `/v1/{db}/_tables/{table}` | ❌ fluss-rust not supported |
| DELETE | `/v1/{db}/_tables/{table}` | ✅ Delete table |
| POST | `/v1/{db}/{table}/offsets` | ✅ Query offset |
| GET | `/v1/{db}/{table}/partitions` | ✅ Query partitions |

### Others

- `GatewayError` type system + HTTP/business error codes ✅
- `json_to_datum` / `datum_to_json` bidirectional conversion ✅
- HTTP Basic Auth parsing middleware ✅ (`src/server/auth.rs`)
- Docker Compose integration test framework ✅ (`tests/setup.rs` + `tests/integration.rs` + `tests/teardown.rs`)
- Ops lifecycle scripts ✅ (`bin/fluss-gateway.sh`)
- `serve` CLI subcommand ✅ (`clap::Subcommand`)
- Graceful shutdown (SIGTERM/SIGINT + `with_graceful_shutdown`) ✅

---

## Integration Test Flow

Cluster lifecycle is decoupled from tests, managed through three separate files:

```bash
cargo test --test setup          # Start Fluss cluster + Gateway binary
cargo test --test integration     # Run 19 parallel integration tests
cargo test --test teardown        # Shutdown cluster, clean containers and processes
```

| File | Responsibility |
|------|----------------|
| `tests/setup.rs` | Start podman compose (ZooKeeper + Coordinator + TabletServer), start gateway binary (nohup), wait for ready |
| `tests/integration.rs` | Pure tests, assumes cluster is running, no lifecycle management |
| `tests/teardown.rs` | Kill gateway process + compose down + clean up orphaned containers |
| `tests/common.rs` | `GatewayClient` wrapper, test table creation utilities |

Manual cleanup (e.g., after test interruption):
```bash
cargo test --test teardown
# Or manually:
kill $(cat /tmp/fluss-gateway-test.pid) 2>/dev/null
podman compose -f deploy/docker/docker-compose.dev.yml --project-name fluss-gateway down
```

---

## Phase 5: Ops CLI + Graceful Shutdown Design

### Architecture

```
fluss-gateway serve [OPTIONS]       # Rust subcommand: foreground start
bin/fluss-gateway.sh start [OPTS]   # Shell script: nohup + PID file
bin/fluss-gateway.sh stop            # Shell script: read PID, send SIGTERM
bin/fluss-gateway.sh restart         # Shell script: stop + start
bin/fluss-gateway.sh status          # Shell script: check PID + health check
```

### Graceful Shutdown Implementation

1. `src/server/mod.rs`: `run()` adds `shutdown_signal` async future
2. Listen for `SIGINT` (Ctrl+C) and `SIGTERM` (kill)
3. Use `axum::serve().with_graceful_shutdown(shutdown_signal)` to wait for existing requests to complete
4. After shutdown, call `ConnectionPool::close()` to clear moka cache

---

## Phase 3a: Integration Test Fix ✅ (Complete)

### Fix Content

1. `tests/common.rs`: `is_gateway_ready()` changed to async, using `reqwest::Client` (blocking client panics inside tokio)
2. `tests/common.rs`: `start_cluster()` in `is_gateway_ready_async()` → `is_gateway_ready().await`
3. `tests/integration.rs`: `setup()` in `start_cluster().expect(...)` → `.await.expect(...)`
4. `tests/common.rs`: `table_info()` added HTTP status code check (5xx no longer treated as Ok)
5. `docker-compose.yml`: image tag `0.9.0` → `0.9.0-incubating` (locally available image)
6. `docker-compose.yml`: added `FLUSS://` internal listener (0.9.0-incubating requires both protocols)
7. `docker-compose.yml`: `advertised.listeners` CLIENT changed to `localhost` (resolvable from host)

**Result: 12/12 integration tests all passing**

---

## Phase 3b: Auth Identity Penetration Refactor Design (Confirmed)

### Background Conclusions

Researched Kafka REST Proxy source code: **Kafka REST does not do per-user connection pooling**, uses a single global Producer (service account model). This is a clearly identified design flaw that we will not replicate.

Fluss Gateway targets multi-tenant scenarios where different users have different Fluss ACLs, so true identity penetration is required.

### Current Architecture Problem

```
HTTP Request (user:pass)
  -> auth_middleware (only extracts credentials into extensions, actually unused)
  -> FlussBackend (always uses a single FlussConnection with static credentials at startup)
```

All users share the same Fluss connection, making ACLs completely ineffective.

### Target Architecture

```
HTTP Request (Authorization: Basic user:pass)
  -> auth_middleware
       ├── auth_type = "none"        -> inject None, use default static connection
       └── auth_type = "passthrough" -> extract credentials, inject Some(Credentials)
  -> Handler gets Credentials from Extension
  -> FlussBackend.get_conn(credentials)
       -> ConnectionCache.get_or_insert(key, || FlussConnection::new_with_sasl(...))
  -> Operate on Fluss (Fluss executes ACL authorization based on FlussPrincipal)
```

### Technology Choice: moka (Confirmed)

**Why not deadpool/bb8**: Both are designed for homogeneous connection pools (N connections to the same endpoint), not supporting multi-key scenarios. Using them for per-credential pooling requires a `HashMap<Key, Pool>` two-layer nesting, which is more complex.

**Why moka**: Essentially a **connection cache** (1 connection per credential), not a traditional connection pool. moka is the Rust async version of Caffeine, natively supporting:
- `max_capacity`: global connection limit
- `time_to_idle`: idle timeout for automatic eviction
- Concurrency safety: only builds connection once for concurrent initialization of the same key (built-in coalescing)
- No need to write custom background cleanup threads

```toml
# Cargo.toml addition
moka = { version = "0.12", features = ["future"] }
```

```rust
// Connection cache structure
type CredentialKey = (String, [u8; 32]);  // (username, SHA-256(password))

let cache: Cache<CredentialKey, Arc<FlussConnection>> = Cache::builder()
    .max_capacity(500)                           // global limit, configurable
    .time_to_idle(Duration::from_secs(600))      // 10 minute idle eviction
    .build();
```

### Configuration Parameters (Confirmed)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `auth.type` | `"none"` | `"none"` or `"passthrough"` |
| `pool.max_connections` | `500` | Global max FlussConnection count |
| `pool.idle_timeout_secs` | `600` | 10 minutes, upper limit for old connections after password change |

### Implementation Steps

1. Add `moka` dependency in `Cargo.toml`
2. New `src/config.rs`: `GatewayConfig` struct, supports `gateway.toml` + CLI args (CLI takes priority)
3. New `src/pool.rs`: `ConnectionPool` wrapping moka cache, `get_or_create(credentials)` method
4. Refactor `src/backend/mod.rs`:
   - `FlussBackend` now holds `Arc<ConnectionPool>` + `AuthConfig`
   - Each method adds `conn: Arc<FlussConnection>` parameter (or calls pool internally)
5. Refactor `src/server/mod.rs` + all handlers:
   - All endpoints add `Extension(Option<Credentials>)` parameter
   - `passthrough` mode returns 401 if credentials are missing
6. Clean up redundant `AuthLayer`/`AuthService` in `auth.rs` (merge with standalone `auth_middleware`)

---

## Phase 4: Deployment Improvement Design (Confirmed)

### Directory Structure Adjustment

Currently `Dockerfile` and `docker-compose.yml` are in the project root, need to migrate:

```
deploy/
├── docker/
│   ├── Dockerfile                  # Moved from root
│   ├── docker-compose.dev.yml      # Local dev/integration tests (with Fluss cluster)
│   └── docker-compose.prod.yml     # Production (Gateway only, connects to external Fluss cluster)
├── systemd/
│   └── fluss-gateway.service       # systemd unit file template
└── config/
    └── gateway.toml.example        # Configuration file example
```

Integration test `common.rs` needs to update `COMPOSE_FILE` path constant accordingly.

### Configuration File `gateway.toml` (New)

Currently only supports CLI args, not production-friendly. Need to support configuration files. CLI args take priority over config file.

```toml
[server]
host = "0.0.0.0"
port = 8080

[fluss]
coordinator = "localhost:9123"

[auth]
type = "none"           # "none" | "passthrough"

[pool]
max_connections = 500
idle_timeout_secs = 600

[log]
level = "info"
```

### Bare-Metal Deployment Flow

```bash
# Direct binary run (CLI args)
./fluss-gateway --fluss-coordinator=coordinator:9123 --port=8080

# Configuration file method
./fluss-gateway --config=/etc/fluss-gateway/gateway.toml

# systemd method
systemctl enable fluss-gateway
systemctl start fluss-gateway
```

---

## Technical Debt (Deferred)

| Item | Priority | Description |
|------|----------|-------------|
| `prefix_scan` implementation | Medium | Need to research fluss-rust prefix scan API |
| Rate limiting (Tier 1 + Tier 2) | Medium | Reference Kafka REST `ProduceRateLimiters` four-dimensional limiting |

---

## Development Standards

- **After feature development completes, corresponding user documentation must be updated simultaneously** (README, API docs, deployment docs, etc.), ensuring code and documentation changes are in the same commit.

---

## Next Session Task List

Execute in order:

### Step 5: Ops CLI + Graceful Shutdown (Phase 5) ✅

- [x] Refactor `src/main.rs` to clap Subcommand (`serve` subcommand)
- [x] Add graceful shutdown to `src/server/mod.rs` (SIGTERM/SIGINT + with_graceful_shutdown)
- [x] Add `close()` method to `src/pool.rs`
- [x] Ops scripts `bin/fluss-gateway.sh` (start/stop/restart/status)
- [x] Clean up deprecated `GatewayCliArgs` / `apply_cli_args`

### Step 6: Chinese Documentation (Phase 6) ✅

- [x] Chinese README (project intro, features, architecture, quick start)
- [x] Chinese API docs (`docs/cn/API.md`, endpoint/request/response format)
- [x] Chinese deployment docs (`docs/cn/DEPLOY.md`, Docker/bare-metal/systemd)
- [x] Chinese ops manual (start/stop/restart/status usage)

### Step 7: Metadata Management (Phase 7 — Core Requirement)

- [ ] Add write methods to `src/backend/mod.rs`: `create_database`, `drop_database`, `create_table`, `drop_table`, `alter_table`
- [ ] Add read methods to `src/backend/mod.rs`: `list_offsets`, `list_partitions`
- [ ] Add DTOs to `src/types/mod.rs`: `CreateDatabaseRequest`, `CreateTableRequest`, `AlterTableRequest`, `OffsetInfo`, `PartitionInfo`
- [ ] Implement handlers in `src/server/rest/mod.rs`: `create_database`, `drop_database`, `create_table`, `drop_table`, `alter_table`, `list_offsets`
- [ ] Register new routes in `src/server/mod.rs`
- [ ] Update API documentation (Chinese and English)
- [ ] Integration tests

### Step 8: Pre-built Release Packages (Phase 8)

Goal: Users can download and deploy without compiling from source.

- [ ] GitHub Actions CI: Linux x86_64/aarch64, macOS x86_64/aarch64 cross-compilation
- [ ] `cargo-dist` packaging: binary + `gateway.toml.example` + ops scripts + systemd unit
- [ ] GitHub Releases auto-publish `.tar.gz` release packages
- [ ] Docker Hub auto-build multi-arch images (amd64/arm64)
- [ ] Provide one-click install script `curl ... | sh`

### Step 9: Monitoring & Observability (Phase 9)

- [ ] Prometheus `/metrics` endpoint (request count, latency, error rate, connection pool status)
- [ ] Structured logging (JSON format, supports ELK/Loki collection)
- [ ] Audit logging (records user, operation, result for each request)
- [ ] Grafana dashboard template

### Step 10: Streaming Consumption (Phase 10)

- [ ] SSE (Server-Sent Events) streaming consumption endpoint
- [ ] WebSocket streaming consumption endpoint (optional)
- [ ] Offset commit/query/reset
- [ ] Consumer group management
