# Fluss Gateway 架构设计

> 基于 FIP-32(多协议查询网关)的扩展设计,补充写入能力和 Kafka REST Proxy 工程经验。

## 背景与定位

### FIP-32 概述

FIP-32 提出了一个**多协议查询网关**:基于 `fluss-rust` + DataFusion 的只读 Rust 服务,通过四种协议暴露 Fluss 表:
- Arrow Flight SQL (9093)
- PostgreSQL 有线协议 (5432)
- gRPC API (9094)
- REST/HTTP API (8080)

### 本方案与 FIP-32 的关系

| 维度 | FIP-32(查询网关) | 本方案(REST Gateway 扩展) |
|------|-------------------|--------------------------|
| 范围 | 只读 | 只读 + **写入** |
| REST API | 简单路径 `/v1/{db}/{table}` | 完整 CRUD + 流式消费 |
| 流式消费 | 未涉及 | WebSocket/SSE 实时推送 |
| 限流安全 | 未涉及 | 两层限流 + 认证 + 审计 |
| 写入 | 明确推迟到未来 FIP | **本方案核心目标** |

**本方案是 FIP-32 的补充而非替代**。

## 整体架构

```
+------------------------------------------------------------------+
|                       Fluss Gateway (Rust)                        |
|                                                                    |
|  +-----------+  +-----------+  +-----------+  +----------------+  |
|  | Flight SQL|  | PostgreSQL|  |   gRPC    |  |   HTTP REST    |  |
|  |  (只读)    |  |  (只读)    |  | (读写)    |  |  (读写+流式)    |  |
|  +-----+-----+  +-----+-----+  +-----+-----+  +-------+--------+  |
|        |              |              |                |            |
|  +-----+--------------+--------------+----------------+---------+ |
|  |              DataFusion (SQL 查询引擎, 只读路径)               | |
|  +-----+--------------+--------------+----------------+---------+ |
|        |              |              |                |            |
|  +-----+--------------+--------------+----------------+---------+ |
|  |              Service / Controller Layer                        | |
|  |  AdminService | ProduceService | ConsumeService | Streaming   | |
|  +-----+--------------+--------------+----------------+---------+ |
|  |              Middleware Layer                                  | |
|  |  Auth | RateLimiter | Metrics | AuditLog | EndpointACL        | |
|  +-----+--------------+--------------+----------------+---------+ |
|  |              Fluss Backend Layer                               | |
|  |  FlussBackend trait + ConnectionPool                           | |
|  +-----+--------------+--------------+----------------+---------+ |
|        |              |              |                |            |
+--------+--------------+--------------+----------------+------------+
         v              v              v                v
  +-------------+  +-------------+  +-------------+  +-----------+
  | Coordinator |  | TabletServer|  | Schema      |  | Lake      |
  | (Admin GW)  |  | (Data GW)   |  | Registry    |  | Storage   |
  +-------------+  +-------------+  +-------------+  +-----------+
```

## 四层架构

1. **协议前端层** - FIP-32 已有四种协议,本方案重点扩展 REST/HTTP 和 gRPC
2. **查询引擎层** (DataFusion) - FIP-32 已有,处理 SQL 查询
3. **Service 层** - 本方案新增,处理写入、流式消费、治理逻辑
4. **Backend 层** - FIP-32 已有 `FlussBackend` trait,本方案扩展写入方法

## 项目结构

```
fluss-query-gateway/
├── Cargo.toml
├── pom.xml                          # Maven 集成
├── build.rs                         # Proto 编译
├── proto/
│   └── gateway.proto                # gRPC 服务定义
├── src/
│   ├── backend/                     # FIP-32 已有,需扩展写入
│   │   ├── mod.rs                   # FlussBackend trait
│   │   ├── native.rs                # NativeFlussBackend
│   │   ├── mock.rs                  # MockFlussBackend
│   │   └── connection_pool.rs       # 连接池管理
│   ├── types/                       # FIP-32 已有
│   │   └── mod.rs                   # Fluss <-> Arrow 类型映射
│   ├── catalog/                     # FIP-32 已有
│   │   ├── mod.rs
│   │   ├── catalog_provider.rs      # FlussCatalogProvider
│   │   ├── schema_provider.rs       # FlussSchemaProvider
│   │   ├── kv_table_provider.rs     # FlussKvTableProvider
│   │   └── log_table_provider.rs    # FlussLogTableProvider
│   ├── execution/                   # FIP-32 已有
│   │   ├── mod.rs
│   │   ├── kv_lookup_exec.rs        # FlussKvLookupExec
│   │   ├── kv_prefix_exec.rs        # FlussKvPrefixExec
│   │   └── log_scan_exec.rs         # FlussLogScanExec
│   ├── server/                      # FIP-32 已有 + 本方案扩展
│   │   ├── mod.rs
│   │   ├── flight_sql.rs            # Arrow Flight SQL 服务器
│   │   ├── postgres.rs              # PostgreSQL 有线协议服务器
│   │   ├── grpc.rs                  # gRPC 服务器(扩展写入)
│   │   └── rest/                    # REST 服务器(重点扩展)
│   │       ├── mod.rs               # 路由组装
│   │       ├── health.rs            # /health
│   │       ├── metadata.rs          # /v1/_databases, /v1/{db}/_tables
│   │       ├── lookup.rs            # GET /v1/{db}/{table}?pk.{col}={val}
│   │       ├── scan.rs              # POST /v1/{db}/{table}/scan
│   │       ├── produce.rs           # POST /v1/{db}/{table}/rows (新增)
│   │       ├── subscribe.rs         # WS/SSE 流式订阅 (新增)
│   │       └── admin.rs             # 表/数据库 CRUD (新增)
│   ├── service/                     # 本方案新增
│   │   ├── mod.rs
│   │   ├── produce_service.rs       # 写入服务 (Append/Upsert)
│   │   ├── streaming_service.rs     # 流式消费服务
│   │   └── admin_service.rs         # 元数据管理服务
│   ├── middleware/                   # 本方案新增
│   │   ├── mod.rs
│   │   ├── auth.rs                  # 认证中间件
│   │   ├── rate_limit.rs            # 限流中间件
│   │   ├── metrics.rs               # Prometheus 指标
│   │   └── audit_log.rs             # 审计日志
│   ├── harness/                     # FIP-32 已有
│   │   └── mod.rs                   # 运行时工具
│   └── testing/                     # FIP-32 已有
│       └── mod.rs                   # 测试夹具
├── tests/                           # 集成测试
├── benches/                         # 性能基准测试
└── docker/                          # 容器构建
```

## 核心设计要点

### FlussBackend Trait 扩展

```rust
#[async_trait]
pub trait FlussBackend: Send + Sync {
    // === 只读操作 (FIP-32 已有) ===
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

    // === 写入操作 (本方案新增) ===
    async fn append_rows(&self, table: &str, rows: &[Row])
        -> Result<ProduceResult, BackendError>;
    async fn upsert_rows(&self, table: &str, rows: &[RowWithChange])
        -> Result<ProduceResult, BackendError>;

    // === 元数据管理 (本方案新增) ===
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

    // === 流式消费 (本方案新增) ===
    async fn subscribe_log(&self, table: &str, offset: i64)
        -> Result<RecordStream, BackendError>;
}
```

### 限流策略

**两层限流** (参考 Kafka REST 经验):
- **Tier 1**: 通用 API 限流 (Tower + governor token bucket) - 所有端点
- **Tier 2**: Produce 专属四维限流 - 全局请求数、全局字节数、每 DB 请求数、每 DB 字节数

### 错误码体系

遵循 `HTTP状态码 + 2位后缀` 模式:

| 错误码 | 含义 |
|--------|------|
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

### Kafka REST 的坑及规避

| Kafka 的问题 | Fluss Gateway 的规避方案 |
|-------------|------------------------|
| 有状态消费者 | 无状态 Scan API + 长连接流式消费 |
| 连接不池化 | FlussConnection 连接池 (DashMap<NodeId, Connection>) |
| 消费者内存泄漏 | 无持久消费者注册表,WS 断开自动清理 |
| 限流覆盖不全 | 全端点 Tower 限流 + Produce 四维限流 |
| 错误处理不一致 | 统一 GatewayError 类型,实现 IntoResponse |
| 单点 Producer | 使用 fluss-rust WriterClient (内部已有 Sender + RecordAccumulator) |

## 分阶段实施计划

### Phase 1: 基础框架 + 读取 (当前)
- 在 `fluss-query-gateway/` 中搭建项目结构
- 复用 FIP-32 已有的 `FlussBackend` trait(只读方法已实现)
- 实现连接池管理 `ConnectionPool`
- 复用 FIP-32 已有的 REST 读取端点(KV 点查、前缀扫描、日志表扫描)
- **交付物**: 可编译,支持通过 REST API 读取 Fluss 数据

### Phase 2: 写入路径 (核心)
- 扩展 `FlussBackend` trait 增加 `append_rows` / `upsert_rows` 方法
- 实现 `NativeFlussBackend` 的写入路径(基于 fluss-rust WriterClient)
- 实现 `ProduceService`(写入服务,封装 WriterClient)
- JSON 反序列化器(values -> Fluss Row)
- Arrow IPC 反序列化
- 根据表类型自动路由(LOG 表 -> append,PK 表 -> upsert)
- REST 端点:`POST /v1/{db}/{table}/rows`
- **交付物**: 完整的写入 API

### Phase 3: 认证 - 身份穿透 (安全)
- Gateway 作为协议桥接器,不维护独立认证体系
- 客户端通过 HTTP Basic Auth 传递用户名/密码
- Gateway 用该凭据通过 SASL/PLAIN 认证连接 Fluss
- 权限控制完全在 Fluss 端(ZooKeeper ACL)
- 配置:`[auth] type = "none" | "http_basic"`
- **交付物**: 身份穿透中间件 + 配置

### Phase 4: 完善与测试
- 使用 `MockFlussBackend` 补充写入单元测试
- 集成测试(读写端到端验证)
- 错误码体系完善
- **交付物**: 可发布的读写 + 认证 Gateway

---

## 延后事项(后续迭代)

| 事项 | 原因 |
|------|------|
| 流式消费(WebSocket/SSE) | 非核心需求,读写优先 |
| 元数据管理(DB/Table CRUD) | 非核心需求 |
| 限流(Tier 1 + Tier 2) | 后续生产环境需要时添加 |
| Prometheus 指标 | 后续监控需要时添加 |
| 审计日志 | 后续合规需要时添加 |
| gRPC 写入扩展 | REST 优先,gRPC 后续 |

---

## 相关代码库参考

### Fluss (Java)

| 模块 | 关键路径 | 用途 |
|------|---------|------|
| fluss-rpc | `fluss-rpc/src/main/proto/FlussApi.proto` | RPC 协议定义(含 AuthenticateRequest/Response) |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/server/NettyServerHandler.java` | 认证握手流程 |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/client/NettyClient.java` | 客户端认证流程 |
| fluss-rpc | `fluss-rpc/src/main/java/.../rpc/netty/server/Session.java` | Session 封装 FlussPrincipal |
| fluss-common | `fluss-common/src/main/java/.../security/auth/` | 认证插件体系(SPI) |
| fluss-common | `fluss-common/src/main/java/.../security/auth/sasl/authenticator/` | SASL/PLAIN 实现 |
| fluss-common | `fluss-common/src/main/java/.../security/acl/FlussPrincipal.java` | Principal 定义 |
| fluss-server | `fluss-server/src/main/java/.../server/authorizer/` | ACL 鉴权(DefaultAuthorizer) |
| fluss-client | `fluss-client/src/main/java/.../client/` | Java 客户端入口 |
| fluss-client | `fluss-client/src/main/java/.../client/connection.rs` | FlussConnection(Rust 对应参考) |
| fluss-client | `fluss-client/src/main/java/.../client/admin.rs` | FlussAdmin 元数据操作 |
| fluss-client | `fluss-client/src/main/java/.../client/table/scanner.rs` | LogScanner 消费原语 |
| fluss-client | `fluss-client/src/main/java/.../client/write/` | WriterClient 写入路径 |

### fluss-rust (Rust)

| 模块 | 关键路径 | 用途 |
|------|---------|------|
| fluss | `crates/fluss/src/client/` | Rust 客户端入口 |
| fluss | `crates/fluss/src/client/connection.rs` | FlussConnection 入口点 |
| fluss | `crates/fluss/src/client/admin.rs` | FlussAdmin 元数据操作 |
| fluss | `crates/fluss/src/client/table/scanner.rs` | LogScanner 消费原语 |
| fluss | `crates/fluss/src/client/table/mod.rs` | FlussTable 入口 |
| fluss | `crates/fluss/src/client/write/` | WriterClient 写入路径 |

### DataFusion (Rust)

| 模块 | 关键路径 | 用途 |
|------|---------|------|
| core | `datafusion/core/` | DataFusion 核心引擎 |
| catalog | `datafusion/core/src/catalog/` | Catalog 抽象(FIP-32 用此扩展 FlussCatalog) |
| execution | `datafusion/core/src/execution/` | 执行引擎 |
| physical-plan | `datafusion/core/src/physical_plan/` | 物理执行计划(FIP-32 自定义 ExecutionPlan 参考) |
| examples | `datafusion-examples/` | 使用示例 |
