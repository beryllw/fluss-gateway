# Fluss Gateway 项目进度

> 最后更新：2026-04-04

## 整体进度

| Phase | 内容 | 状态 | 备注 |
|-------|------|------|------|
| Phase 1 | 基础框架 + 读取 | ✅ 完成 | 读取端点全部实现 |
| Phase 2 | 写入路径 | ✅ 完成 | append/upsert/delete 已实现 |
| Phase 3a | 修复集成测试 + 验证写入 | ✅ 完成 | 12/12 集成测试通过 |
| Phase 3b | 认证身份穿透重构 | 🔜 下一步 | 方案已确定，见下 |
| Phase 4 | 部署完善 | 📋 已规划 | Docker + 物理机方案已确定 |

---

## 已实现功能清单

### 后端层 (`src/backend/mod.rs`)

| 方法 | 类型 | 状态 |
|------|------|------|
| `list_databases` | 元数据读 | ✅ |
| `list_tables` | 元数据读 | ✅ |
| `get_table_info` | 元数据读 | ✅ |
| `lookup` | KV 点查 | ✅ |
| `scan` | 日志扫描 | ✅ |
| `append_rows` | 写入（Log 表） | ✅ |
| `upsert_rows` | 写入（PK 表） | ✅ |
| `delete_rows` | 删除（PK 表） | ✅ |
| `prefix_lookup` | KV 前缀扫描 | ❌ stub（返回 500） |

### REST 端点

| 方法 | 路径 | 状态 |
|------|------|------|
| GET | `/health` | ✅ |
| GET | `/v1/_databases` | ✅ |
| GET | `/v1/{db}/_tables` | ✅ |
| GET | `/v1/{db}/{table}/_info` | ✅ |
| GET | `/v1/{db}/{table}?pk.col=val` | ✅ |
| GET | `/v1/{db}/{table}/prefix` | ❌ stub |
| POST | `/v1/{db}/{table}/batch` | ✅ |
| POST | `/v1/{db}/{table}/scan` | ✅ |
| POST | `/v1/{db}/{table}/rows` | ✅ |

### 其他

- `GatewayError` 类型体系 + HTTP/业务错误码 ✅
- `json_to_datum` / `datum_to_json` 双向转换 ✅
- HTTP Basic Auth 解析中间件 ✅（`src/server/auth.rs`）
- Docker Compose 集成测试框架 ✅（`tests/integration.rs` + `tests/common.rs`）

---

## Phase 3a：集成测试修复 ✅（已完成）

### 修复内容

1. `tests/common.rs`：`is_gateway_ready()` 改为 async，用 `reqwest::Client`（blocking client 在 tokio 内 panic）
2. `tests/common.rs`：`start_cluster()` 中 `is_gateway_ready_async()` → `is_gateway_ready().await`
3. `tests/integration.rs`：`setup()` 中 `start_cluster().expect(...)` → `.await.expect(...)`
4. `tests/common.rs`：`table_info()` 增加 HTTP 状态码检查（5xx 不再被当成 Ok）
5. `docker-compose.yml`：镜像 tag `0.9.0` → `0.9.0-incubating`（本地可用镜像）
6. `docker-compose.yml`：添加 `FLUSS://` 内部监听器（0.9.0-incubating 必须同时配置两种协议）
7. `docker-compose.yml`：`advertised.listeners` CLIENT 改为 `localhost`（宿主机可解析）

**结果：12/12 集成测试全部通过**

---

## Phase 3b：认证身份穿透重构方案（已确定）

### 背景结论

调研了 Kafka REST Proxy 源码：**Kafka REST 不做 per-user 连接池**，使用全局单一 Producer（服务账号模式）。这是被明确识别的设计缺陷，我们不复制。

Fluss Gateway 面向多租户场景，不同用户有不同的 Fluss ACL，必须做真正的身份穿透。

### 当前架构问题

```
HTTP 请求 (user:pass)
  -> auth_middleware（仅提取凭据存入 extensions，实际未用）
  -> FlussBackend（始终用启动时静态凭据的单一 FlussConnection）
```

所有用户共享同一 Fluss 连接，ACL 完全失效。

### 目标架构

```
HTTP 请求 (Authorization: Basic user:pass)
  -> auth_middleware
       ├── auth_type = "none"        -> 注入 None，使用默认静态连接
       └── auth_type = "passthrough" -> 提取凭据，注入 Some(Credentials)
  -> Handler 从 Extension 取 Credentials
  -> FlussBackend.get_conn(credentials)
       -> ConnectionCache.get_or_insert(key, || FlussConnection::new_with_sasl(...))
  -> 操作 Fluss（Fluss 端根据 FlussPrincipal 执行 ACL 鉴权）
```

### 技术选型：moka（确定）

**不用 deadpool/bb8** 的原因：两者设计用于同质连接池（N 个连接到同一端点），不支持多 key 场景。用于 per-credential 池需要 `HashMap<Key, Pool>` 两层嵌套，反而更复杂。

**用 `moka`** 的原因：本质上是一个 **连接缓存**（每凭据 1 个连接），而非传统连接池。moka 是 Rust 异步版 Caffeine，天然支持：
- `max_capacity`：全局连接上限
- `time_to_idle`：空闲超时自动淘汰
- 并发安全：同一 key 并发初始化时只建一次连接（内置 coalescing）
- 无需自己写后台清理线程

```toml
# Cargo.toml 新增
moka = { version = "0.12", features = ["future"] }
```

```rust
// 连接缓存结构
type CredentialKey = (String, [u8; 32]);  // (username, SHA-256(password))

let cache: Cache<CredentialKey, Arc<FlussConnection>> = Cache::builder()
    .max_capacity(500)                           // 全局上限，可配置
    .time_to_idle(Duration::from_secs(600))      // 10 分钟空闲淘汰
    .build();
```

### 配置参数（确定）

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `auth.type` | `"none"` | `"none"` 或 `"passthrough"` |
| `pool.max_connections` | `500` | 全局最大 FlussConnection 数 |
| `pool.idle_timeout_secs` | `600` | 10 分钟，用户改密码后旧连接存活上限 |

### 实现步骤

1. `Cargo.toml` 添加 `moka` 依赖
2. 新增 `src/config.rs`：`GatewayConfig` 结构体，支持 `gateway.toml` + CLI 参数（CLI 优先）
3. 新增 `src/pool.rs`：`ConnectionPool` 封装 moka cache，`get_or_create(credentials)` 方法
4. 重构 `src/backend/mod.rs`：
   - `FlussBackend` 改为持有 `Arc<ConnectionPool>` + `AuthConfig`
   - 每个方法增加 `conn: Arc<FlussConnection>` 参数（或内部调用 pool）
5. 重构 `src/server/mod.rs` + 所有 handler：
   - 所有端点增加 `Extension(Option<Credentials>)` 参数
   - `passthrough` 模式下缺少凭据返回 401
6. 清理 `auth.rs` 中的冗余 `AuthLayer`/`AuthService`（与独立 `auth_middleware` 合并）

---

## Phase 4：部署完善方案（已确定）

### 目录结构调整

当前 `Dockerfile` 和 `docker-compose.yml` 在项目根目录，需迁移：

```
deploy/
├── docker/
│   ├── Dockerfile                  # 从根目录移入
│   ├── docker-compose.dev.yml      # 本地开发/集成测试（含 Fluss 集群）
│   └── docker-compose.prod.yml     # 生产（仅 Gateway，对接外部 Fluss 集群）
├── systemd/
│   └── fluss-gateway.service       # systemd 单元文件模板
└── config/
    └── gateway.toml.example        # 配置文件示例
```

集成测试的 `common.rs` 需同步更新 `COMPOSE_FILE` 路径常量。

### 配置文件 `gateway.toml`（新增）

当前只支持 CLI 参数，生产环境不友好，需支持配置文件。CLI 参数优先级高于配置文件。

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

### 物理机部署流程

```bash
# 直接二进制运行（CLI 参数）
./fluss-gateway --fluss-coordinator=coordinator:9123 --port=8080

# 配置文件方式
./fluss-gateway --config=/etc/fluss-gateway/gateway.toml

# systemd 方式
systemctl enable fluss-gateway
systemctl start fluss-gateway
```

---

## 技术债（延后处理）

| 事项 | 优先级 | 说明 |
|------|--------|------|
| `prefix_scan` 实现 | 中 | 需调研 fluss-rust 前缀扫描 API |
| 流式消费（WebSocket/SSE） | 低 | 非核心需求，后续迭代 |
| 元数据管理（DB/Table CRUD） | 低 | 非核心需求 |
| 限流（Tier 1 + Tier 2） | 中 | 参考 Kafka REST `ProduceRateLimiters` 四维限流 |
| Prometheus 指标 | 低 | 后续监控需要 |
| 审计日志 | 低 | 后续合规需要 |

---

## 下一个 Session 的任务清单

按顺序执行：

### Step 3：认证重构（Phase 3b）🔜

- [ ] 添加 `moka` 依赖
- [ ] 实现 `src/config.rs`（`GatewayConfig` + `gateway.toml` 解析）
- [ ] 实现 `src/pool.rs`（`ConnectionPool` wrapping moka cache）
- [ ] 重构 `src/backend/mod.rs`（单连接 → 连接池）
- [ ] 重构所有 handler（加 `Extension(Option<Credentials>)`）
- [ ] 清理冗余的 `AuthLayer`/`AuthService`

### Step 4：部署整理（Phase 4）

- [ ] 创建 `deploy/` 目录，迁移 Docker 文件
- [ ] 新增 `deploy/docker/docker-compose.dev.yml`（从 `docker-compose.yml` 改造）
- [ ] 新增 `deploy/docker/docker-compose.prod.yml`（仅 gateway 服务）
- [ ] 新增 `deploy/systemd/fluss-gateway.service`
- [ ] 新增 `deploy/config/gateway.toml.example`
- [ ] 实现配置文件解析（`toml` crate）
- [ ] 更新 `tests/common.rs` 中 `COMPOSE_FILE` 路径
