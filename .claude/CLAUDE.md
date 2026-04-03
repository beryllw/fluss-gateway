# Fluss Gateway - Agent 约束规则

> 本文档定义了 agent 每次执行任务时**必须加载**的约束规则。

## 项目基本信息

- **项目名称**: Fluss Gateway
- **技术栈**: Rust + fluss-rust + DataFusion + Axum
- **定位**: 基于 FIP-32 扩展的 Fluss REST API 网关，补充写入能力、流式消费和治理能力
- **项目根目录**: `/Users/boyu/VscodeProjects/fluss-gateway`
- **文档目录**: `docs/`
- **调研目录**: `research/`

## 认证方案 - 身份穿透

Gateway 作为协议桥接器，不维护独立认证体系：
- 客户端通过 HTTP Basic Auth 传递用户名/密码
- Gateway 用该凭据通过 SASL/PLAIN 认证连接 Fluss
- 权限控制完全在 Fluss 端（ZooKeeper ACL）
- 配置：`[auth] type = "none" | "http_basic"`

## 必须遵循的规则

### 1. 代码规范

- 使用 Rust 2021 edition
- 遵循 Rust 官方编码规范（rustfmt + clippy）
- 所有公开 API 必须有文档注释（`///`）
- 错误处理统一使用 `thiserror` 定义自定义错误类型
- 异步代码使用 `tokio` 运行时
- HTTP 框架使用 `axum`

### 2. 架构约束

- 严格遵循四层架构：协议前端层 -> 查询引擎层 -> Service 层 -> Backend 层
- 新增代码必须放在正确的层级模块中
- `FlussBackend` trait 是后端抽象核心，所有 Fluss 操作通过它进行
- Service 层封装业务逻辑，Controller/REST 层只做路由和序列化

### 3. 设计原则

- **无状态消费**: 避免 Kafka REST 的有状态消费者陷阱
- **连接池化**: 所有 Fluss 连接必须通过连接池管理
- **全异步**: 所有 I/O 操作使用 `async/await`，返回 `CompletableFuture` 等效的 Rust Future
- **错误码规范**: 遵循 `HTTP状态码 + 2位后缀` 模式（如 40401, 42901）

### 4. 安全与治理

- 认证采用身份穿透方案（HTTP Basic Auth -> SASL/PLAIN -> Fluss ACL）
- 所有端点默认经过限流中间件
- 写入路径必须经过四维限流检查

### 5. 测试要求

- 使用 `MockFlussBackend` 进行单元测试
- 集成测试放在 `tests/` 目录
- 新代码必须有对应的测试

### 6. 依赖管理

- 优先复用 FIP-32 已有组件（FlussBackend trait、DataFusion 集成、类型映射等）
- 新增依赖需评估必要性和许可证

## 关键参考路径

### 代码库
| 资源 | 路径 |
|------|------|
| Fluss (Java) | `~/IdeaProjects/fluss-community/` |
| fluss-rust | `~/VscodeProjects/fluss-rust/` |
| DataFusion | `~/VscodeProjects/datafusion/` |

### Fluss 认证/鉴权
| 资源 | 路径 |
|------|------|
| RPC 协议 | `~/IdeaProjects/fluss-community/fluss-rpc/src/main/proto/FlussApi.proto` |
| 认证插件 | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/auth/` |
| SASL/PLAIN | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/auth/sasl/authenticator/` |
| ACL 鉴权 | `~/IdeaProjects/fluss-community/fluss-server/src/main/java/.../server/authorizer/` |
| FlussPrincipal | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/acl/FlussPrincipal.java` |

### fluss-rust 客户端
| 资源 | 路径 |
|------|------|
| 客户端入口 | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/` |
| 连接管理 | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/connection.rs` |
| 写入路径 | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/write/` |
| 表扫描 | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/table/scanner.rs` |

### 调研文档
| 资源 | 路径 |
|------|------|
| FIP 设计文档 | `~/AiWorkSpace/fluss-fip/FIP-REST-API/fluss-gateway-design.md` |
| Kafka REST 调研 | `research/kafka-rest-proxy-gateway-research.md` |
| Kafka REST 源码 | `research/kafka-rest-source-code-analysis.md` |

## 按需加载文档

以下文档在**规划和方案设计阶段**按需加载：

- `docs/ARCHITECTURE.md` - 架构设计详情
- `docs/API.md` - API 设计详情
- `~/AiWorkSpace/fluss-fip/FIP-REST-API/fluss-gateway-design.md` - FIP 完整设计
