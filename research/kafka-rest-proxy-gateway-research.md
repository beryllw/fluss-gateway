# Kafka REST API Proxy / Kafka Gateway 调研报告

> 调研目标：为 Fluss Gateway 的开发提供技术方案参考，全面调研 Kafka 生态中 REST Proxy / Gateway 的主流实现方案，分析其架构设计、API 规范、实现细节、优缺点及适用场景。

---

## 一、背景与动机

### 1.1 为什么需要 REST Proxy / Gateway

Apache Kafka 原生使用自定义的二进制 TCP 协议通信，客户端需要引入特定语言的 Kafka SDK（Java、Go、Python 等）。这带来了以下限制：

- **语言生态受限**：不是所有语言都有成熟的 Kafka 客户端库（如前端 JavaScript、嵌入式设备、IoT 终端）
- **网络边界问题**：Kafka 协议不便穿越防火墙、WAF 等网络安全设备，难以向外部合作伙伴开放
- **运维复杂性**：每个客户端都需要管理 Kafka 连接参数、序列化配置、认证凭据等
- **治理缺失**：原生 Kafka 缺乏 API 级别的限流、审计、访问控制等治理能力

REST Proxy / Gateway 的核心价值在于通过 HTTP 协议桥接客户端与 Kafka Broker，降低接入门槛并增加治理层。

### 1.2 Fluss 简介

Apache Fluss（Incubating）是面向实时分析与 AI 场景的流存储系统，服务于 Lakehouse 架构的实时数据层。核心设计理念包括：

- **流与 Lakehouse 统一**：以 table 为统一抽象
- **计算存储分离**：计算引擎（Flink/Spark）处理计算，Fluss 管理状态和存储
- **列式流式处理**：基于 Apache Arrow 的列式数据格式
- **亚秒级数据新鲜度**：面向低延迟分析场景

Fluss 模块结构中包含 `fluss-rpc`（RPC 通信）、`fluss-client`（客户端）、`fluss-kafka`（Kafka 兼容层）等模块。开发 Fluss Gateway 可以参考 Kafka REST Proxy 的设计经验，为 Fluss 提供 HTTP/REST 接入层。

---

## 二、主流方案详细调研

### 2.1 Confluent Kafka REST Proxy

**项目地址**：https://github.com/confluentinc/kafka-rest

#### 2.1.1 架构设计

```
┌─────────────┐     HTTP/REST     ┌──────────────────┐    Kafka Protocol    ┌──────────────┐
│  HTTP Client ├──────────────────►│ Confluent REST   ├──────────────────────►│ Kafka Broker │
│  (any lang)  │                   │ Proxy (Jetty)    │                       │              │
└─────────────┘                   ├──────────────────┤                       ├──────────────┤
                                  │ Schema Registry  │◄─────────────────────►│ ZK/KRaft     │
                                  │ (可选集成)         │                       └──────────────┘
                                  └──────────────────┘
```

- **技术栈**：Java（99.6%），内嵌 Jetty HTTP 服务器
- **构建方式**：Maven 多模块项目，支持生成 standalone fat jar
- **关键依赖**：`common`、`rest-utils`、`schema-registry`
- **默认端口**：8082
- **部署形态**：
  - 独立节点部署（Standalone）
  - 嵌入 Confluent Server（Broker 内嵌 Admin REST API）
  - 多实例集群 + 负载均衡器

#### 2.1.2 API 设计（v2 + v3）

**v2 API（经典版本）**

自定义 Content-Type 标识数据格式：

| Content-Type | 说明 |
|---|---|
| `application/vnd.kafka.binary.v2+json` | 二进制（Base64 编码） |
| `application/vnd.kafka.json.v2+json` | JSON 格式 |
| `application/vnd.kafka.avro.v2+json` | Avro 格式 |
| `application/vnd.kafka.protobuf.v2+json` | Protobuf 格式 |
| `application/vnd.kafka.jsonschema.v2+json` | JSON Schema 格式 |
| `application/vnd.kafka.v2+json` | 元数据响应 |

**核心 Endpoints (v2)**：

| 分类 | 方法 | 路径 | 说明 |
|---|---|---|---|
| **Topics** | GET | `/topics` | 列出所有主题 |
| | GET | `/topics/{name}` | 获取主题元数据 |
| | POST | `/topics/{name}` | 生产消息到主题 |
| **Partitions** | GET | `/topics/{name}/partitions` | 列出分区 |
| | GET | `/topics/{name}/partitions/{id}` | 获取分区元数据 |
| | GET | `/topics/{name}/partitions/{id}/offsets` | 获取偏移量摘要 |
| | POST | `/topics/{name}/partitions/{id}` | 生产消息到指定分区 |
| **Consumers** | POST | `/consumers/{group}` | 创建消费者实例 |
| | DELETE | `/consumers/{group}/instances/{id}` | 销毁消费者 |
| | POST | `.../subscription` | 订阅主题 |
| | GET | `.../subscription` | 获取订阅列表 |
| | DELETE | `.../subscription` | 取消订阅 |
| | POST | `.../assignments` | 手动分配分区 |
| | GET | `.../assignments` | 获取分配信息 |
| | POST | `.../records` | 拉取消费消息 |
| | POST | `.../offsets` | 提交偏移量 |
| | GET | `.../offsets` | 获取已提交偏移量 |

**v3 API（新版本）**

- 统一使用 `application/json` Content-Type
- 路径模式：`/v3/clusters/{cluster_id}/{resource}`
- 新增批量生产 API：`POST .../records:batch`
- 支持多集群 ID 路径参数

**错误码体系**：

| 错误码 | 说明 |
|---|---|
| 40401 | 主题未找到 |
| 40402 | 分区未找到 |
| 40403 | 消费者实例未找到 |
| 40902 | 消费者已存在 |
| 42201/42202 | 缺少 schema |
| 50002 | Kafka 内部错误 |

#### 2.1.3 核心实现细节

**生产者模型**：
- 使用**共享生产者池**（Shared Producer Pool）
- 配置在全局层面设定，**不支持按请求设置生产者参数**
- Key 和 Value 的序列化器必须相同
- 不支持多主题批量生产

**消费者模型**：
- **有状态设计**：消费者实例绑定到特定的 REST Proxy 实例
- 每个消费者实例**限制为单线程**（`Currently limited to one thread per consumer`）
- 消费者需要通过 REST API 显式创建、订阅、拉取、提交、销毁
- 多实例部署时**必须使用 Sticky Session 负载均衡**

**Schema Registry 集成**：
- 与 Confluent Schema Registry 深度集成
- 支持 Avro、Protobuf、JSON Schema 的自动序列化/反序列化
- 通过 Content-Type header 决定序列化方式

#### 2.1.4 优缺点分析

**优点**：
- 功能最全面的 Kafka REST 实现，API 设计成熟
- 与 Schema Registry 深度集成，支持丰富的序列化格式
- 社区活跃，文档完善
- 支持多实例集群部署
- v3 API 支持多集群管理

**缺点**：
- **消费者有状态**：这是最大的架构痛点。消费者实例绑定到特定 Proxy 节点，水平扩展困难，需要 Sticky Session
- **单线程消费者**：消费吞吐量受限
- **全局生产者配置**：无法按请求定制生产者行为
- **HTTP 轮询模型**：消费端需要客户端轮询，无推送机制，延迟较高
- **安全插件闭源**：REST Proxy Security Plugin 需要 Confluent 企业版
- **无 UI 管理界面**：缺乏图形化配置和监控
- **无内置治理能力**：缺少限流、消息过滤、动态路由等高级功能

---

### 2.2 Strimzi Kafka Bridge

**项目地址**：https://github.com/strimzi/strimzi-kafka-bridge

#### 2.2.1 架构设计

```
┌─────────────┐     HTTP/1.1     ┌──────────────────┐    Kafka Protocol    ┌──────────────┐
│  HTTP Client ├─────────────────►│ Strimzi Kafka    ├──────────────────────►│ Kafka Broker │
│              │                   │ Bridge (Vert.x)  │                       │              │
└─────────────┘                   ├──────────────────┤                       └──────────────┘
                                  │ OpenTelemetry    │
                                  │ Tracing          │
                                  └──────────────────┘
```

- **技术栈**：Java，基于 Eclipse Vert.x 异步框架
- **API 规范**：提供 OpenAPI v2 规范文件
- **Kubernetes 原生**：通过 Strimzi Operator 以 CRD (`KafkaBridge`) 形式部署

#### 2.2.2 API 设计

与 Confluent v2 API 高度相似，但做了简化：

| 分类 | 方法 | 路径 | 说明 |
|---|---|---|---|
| **健康检查** | GET | `/healthy` | 健康检查 |
| | GET | `/ready` | 就绪检查 |
| | GET | `/openapi` | 获取 OpenAPI 规范 |
| **Topics** | GET | `/topics` | 列出所有主题 |
| | GET | `/topics/{name}` | 获取主题元数据 |
| | POST | `/topics/{name}` | 生产消息到主题 |
| | POST | `/topics/{name}/partitions/{id}` | 生产消息到指定分区 |
| **Consumers** | POST | `/consumers/{group}` | 创建消费者实例 |
| | DELETE | `.../instances/{name}` | 删除消费者 |
| | POST | `.../subscription` | 订阅主题（支持正则） |
| | GET | `.../records` | 拉取消息 |
| | POST | `.../offsets` | 手动提交偏移量 |
| | POST | `.../positions` | Seek 操作（指定 offset/开头/结尾） |

**数据格式**：支持 JSON 和 Binary（Base64 编码），使用与 Confluent 相同的 Content-Type 约定。

#### 2.2.3 核心实现细节

**安全模型**：
- Kafka 端：支持 TLS 加密 + SASL 认证
- HTTP 端：**Bridge 本身不支持 HTTPS 或 HTTP 认证**，需依赖外部反向代理、API Gateway 或防火墙
- 支持 CORS 跨域配置

**可观测性**：
- 支持 OpenTelemetry 分布式追踪
- 支持基于 OpenAPI operation ID 细粒度配置日志级别
- 提供指标暴露（Prometheus 格式）

**配置前缀**：
- `kafka.`：通用 Kafka 配置
- `kafka.consumer.` / `kafka.producer.`：消费者/生产者专用配置
- `http.`：HTTP 监听及 CORS
- `bridge.tracing`：追踪系统开关

#### 2.2.4 优缺点分析

**优点**：
- **Kubernetes 原生**：Strimzi 生态一等公民，CRD 方式管理
- **轻量级**：不依赖 Schema Registry，部署简单
- **异步架构**：基于 Vert.x，IO 模型高效
- **OpenAPI 规范**：提供标准 API 文档
- **开源免费**：Apache 2.0 协议
- **可观测性好**：原生支持 OpenTelemetry

**缺点**：
- **无 Schema 支持**：仅支持 JSON 和 Binary，不支持 Avro/Protobuf
- **HTTP 端无安全**：安全性完全依赖外部组件
- **消费者同样有状态**：与 Confluent 共享相同的有状态消费者设计缺陷
- **功能较少**：无 Admin API（不支持创建/删除 Topic 等管理操作）
- **HTTP 轮询**：与 Confluent 相同的消费模型限制

---

### 2.3 Karapace（Aiven 开源）

**项目地址**：https://github.com/Aiven-Open/karapace

#### 2.3.1 架构设计

- **技术栈**：Python，异步架构基于 aiohttp，Schema Registry 基于 FastAPI
- **Kafka 客户端**：基于 aiokafka（底层使用 rdkafka）
- **HA 模型**：Leader/Replica 架构，支持高可用和负载均衡

#### 2.3.2 核心特性

- **Confluent API 兼容**：REST Proxy 和 Schema Registry 均兼容 Confluent API（Schema Registry 兼容 6.1.1 API 级别）
- **Schema 支持**：Avro、JSON Schema、Protobuf
- **可观测性**：Metrics + OpenTelemetry
- **OAuth2/OIDC 支持**：支持 Kafka REST Proxy 的 OAuth2/OIDC 认证

#### 2.3.3 优缺点分析

**优点**：
- **完全开源**：可替代 Confluent 企业版的 REST Proxy 和 Schema Registry
- **Drop-in 替换**：客户端和服务端均可直接替换 Confluent 实现
- **异步架构**：Python aiohttp 提供较好的 IO 性能
- **OAuth2 支持**：原生支持 OAuth2/OIDC

**缺点**：
- **Python 实现**：在高吞吐量场景下，Python 的 GIL 限制可能成为瓶颈
- **社区规模小**：相比 Confluent 和 Strimzi 社区活跃度较低
- **与 Confluent API 耦合**：设计目标是兼容而非创新
- **依赖 rdkafka**：底层仍然依赖 C 库，跨平台部署可能有问题

---

### 2.4 Gravitee Kafka Gateway

**项目地址**：https://documentation.gravitee.io/apim/4.8/kafka-gateway

#### 2.4.1 架构设计

Gravitee 提供两种 Kafka 接入模式：

**模式一：协议介导（Protocol Mediation）**
```
┌─────────┐  HTTP/WS/SSE  ┌──────────────┐  Kafka Protocol  ┌──────────┐
│ Client  ├───────────────►│  Gravitee    ├──────────────────►│  Kafka   │
│         │                │  Gateway     │                   │  Broker  │
└─────────┘               └──────────────┘                   └──────────┘
```

**模式二：原生 Kafka 代理（Native Kafka Gateway）**
```
┌─────────┐  Kafka Protocol  ┌──────────────┐  Kafka Protocol  ┌──────────┐
│ Kafka   ├──────────────────►│  Gravitee    ├──────────────────►│  Kafka   │
│ Client  │                   │  Gateway     │                   │  Broker  │
└─────────┘                  └──────────────┘                   └──────────┘
```

#### 2.4.2 差异化特性

与 Confluent REST Proxy 的关键差异：

| 维度 | Confluent REST Proxy | Gravitee Kafka Gateway |
|---|---|---|
| 配置方式 | 全局静态配置 | **按 API 动态配置** |
| 协议支持 | 仅 HTTP | **HTTP、WebSocket、SSE、Webhooks、原生 Kafka** |
| 消息消费 | v3 Cloud 版不支持 | **完全支持** |
| 动态路由 | 不支持 | **支持** |
| 消息过滤 | 不支持 | **支持** |
| CloudEvents | 不支持 | **支持标准化** |
| QoS | 基础 | **支持重放等高级 QoS** |
| UI 管理 | 无 | **提供完整 UI** |
| 安全模型 | 需企业插件 | **灵活的 Entrypoint/Endpoint 安全映射** |

#### 2.4.3 优缺点分析

**优点**：
- **协议多样性**：唯一同时支持 HTTP/WS/SSE/Webhooks 和原生 Kafka 协议的方案
- **完整治理**：限流、审计、访问控制、开发者门户一体化
- **原生 Kafka Gateway**：保留 Kafka 原生协议的同时添加治理层
- **动态配置**：无需重启即可修改 API 策略

**缺点**：
- **商业产品**：核心功能需要付费许可
- **架构复杂度**：引入完整 API 管理平台，部署和运维成本高
- **性能开销**：协议介导必然引入额外延迟
- **过度设计风险**：如果只需简单的 REST Proxy，Gravitee 方案过重

---

### 2.5 API Gateway 集成方案（APISIX / Kong）

#### 2.5.1 Apache APISIX

APISIX 提供两种 Kafka 集成方式：

**1. kafka-proxy 插件**
- 功能有限，仅支持 SASL/PLAIN 认证代理
- 配置 `sasl.username` 和 `sasl.password` 注入到上游 Kafka 请求
- 密码在 etcd 中加密存储

**2. PubSub 框架**
- 通过 WebSocket 连接实现 Kafka 消费
- 提供 `CmdKafkaFetch` 和 `CmdKafkaListOffset` 命令
- **不支持消费者组**，offset 需要手动管理
- 适用于简单的实时消费场景

#### 2.5.2 Kong

- **kafka-upstream 插件**：将 HTTP 请求转发到 Kafka Topic（仅生产方向）
- 与 AWS Lambda 组合桥接 Kafka 消费到 REST API
- 不提供完整的 Kafka REST Proxy 功能

#### 2.5.3 通用 API Gateway 集成模式

| 模式 | 说明 | 适用场景 |
|---|---|---|
| 推送模型 | Gateway 作为 Producer，转发 HTTP 请求到 Kafka | 事件采集、日志收集 |
| 拉取模型 | Gateway 作为 Consumer，暴露 REST endpoint | 简单的数据查询 |
| 代理桥接 | Gateway 代理 REST Proxy 的流量 | 已有 REST Proxy 需要添加治理 |

---

## 三、核心架构问题深入分析

### 3.1 有状态消费者问题（最关键的架构挑战）

**问题描述**：

Confluent REST Proxy、Strimzi Bridge、Karapace 都面临相同的设计问题——**消费者是有状态的**。

```
                          ┌── REST Proxy Node A ──┐
                          │  Consumer Instance X  │ ← 绑定在此节点
┌────────┐   Round Robin  │  Consumer Instance Y  │
│  LB    ├────────────────├── REST Proxy Node B ──┤
└────────┘                │  Consumer Instance Z  │
                          └───────────────────────┘

问题：Client 创建消费者在 Node A，后续请求被路由到 Node B → 404 Not Found
```

**解决方案对比**：

| 方案 | 描述 | 缺点 |
|---|---|---|
| Sticky Session | 负载均衡器基于 Cookie/IP 绑定会话 | 节点故障时消费者丢失，负载不均 |
| 返回具体节点地址 | 创建消费者时返回实际节点 URL | 暴露内部拓扑，客户端需处理重定向 |
| 分布式状态存储 | 消费者状态存储在外部存储中 | 实现复杂，增加依赖 |
| 无状态设计 | 每次请求自包含消费位置信息 | 需要重新设计 API，客户端更复杂 |

**对 Fluss Gateway 的启示**：
- 如果选择有状态消费者模型，必须考虑 Sticky Session 和故障转移
- 建议探索无状态设计：客户端在请求中携带 offset 信息，服务端无需维护消费者实例
- 或采用 WebSocket/SSE 长连接模型，避免 HTTP 轮询的有状态性问题

### 3.2 协议选择：HTTP REST vs WebSocket vs SSE

| 协议 | 生产 | 消费 | 延迟 | 连接开销 | 适用场景 |
|---|---|---|---|---|---|
| HTTP REST | 适合 | 需轮询，不理想 | 较高（轮询间隔） | 低（短连接） | 低频生产、管理操作 |
| WebSocket | 适合 | **非常适合** | 低（推送） | 中（长连接） | 实时双向通信 |
| SSE | 不支持 | **非常适合** | 低（推送） | 低（单向长连接） | 服务端推送消费 |
| gRPC | 适合 | 适合（流式） | 最低 | 中 | 内部服务间通信 |

**建议**：
- 生产端：HTTP REST 即可满足需求
- 消费端：WebSocket 或 SSE 更适合流式数据消费
- 管理端：HTTP REST 最合适

### 3.3 序列化格式设计

| 格式 | Confluent | Strimzi | Karapace | 说明 |
|---|---|---|---|---|
| JSON | 支持 | 支持 | 支持 | 通用性最强 |
| Binary (Base64) | 支持 | 支持 | 支持 | 传递原始二进制 |
| Avro | 支持 | 不支持 | 支持 | 需要 Schema Registry |
| Protobuf | 支持 | 不支持 | 支持 | 需要 Schema Registry |
| JSON Schema | 支持 | 不支持 | 支持 | 需要 Schema Registry |
| Arrow (列式) | 不支持 | 不支持 | 不支持 | Fluss 原生格式 |

**对 Fluss Gateway 的启示**：
- Fluss 基于 Apache Arrow 列式格式，Gateway 可以考虑支持 Arrow IPC 或 Arrow Flight 协议
- 最低限度应支持 JSON 和 Binary，确保通用客户端兼容性
- 如果 Fluss 有 Schema 管理需求，可考虑类似 Schema Registry 的集成

### 3.4 安全模型设计

| 层面 | Confluent | Strimzi | Gravitee | 建议 |
|---|---|---|---|---|
| HTTP 认证 | 企业版插件 | 不支持（依赖外部） | 原生支持 | 应内置基础认证 |
| TLS/HTTPS | 支持 | 依赖反向代理 | 原生支持 | 应原生支持 |
| 后端认证 | SASL | SASL/TLS | 灵活映射 | 按需 |
| 多租户 | 企业版 | 不支持 | 支持 | 可作为后续特性 |
| RBAC | 有限 | 不支持 | 完整 | 基础 ACL 即可 |

---

## 四、方案对比总结

### 4.1 功能矩阵

| 能力 | Confluent REST Proxy | Strimzi Bridge | Karapace | Gravitee | APISIX/Kong |
|---|---|---|---|---|---|
| **开源** | 部分 | 完全 | 完全 | 部分 | 完全 |
| **生产消息** | 完整 | 完整 | 完整 | 完整 | 基础 |
| **消费消息** | 完整 | 完整 | 完整 | 完整 | 有限 |
| **Topic 管理** | 完整(v3) | 只读 | 兼容 | 完整 | 无 |
| **Schema 集成** | 深度 | 无 | 深度 | 有限 | 无 |
| **多协议** | HTTP | HTTP | HTTP | HTTP/WS/SSE/Kafka | WS(有限) |
| **API 治理** | 无 | 无 | 无 | 完整 | 完整 |
| **K8s 原生** | 否 | 是 | 否 | 是 | 是 |
| **可观测性** | 基础 | 好(OTel) | 好(OTel) | 完整 | 完整 |
| **成熟度** | 高 | 中 | 中 | 中 | 高 |

### 4.2 适用场景矩阵

| 场景 | 推荐方案 | 原因 |
|---|---|---|
| 快速接入、功能全面 | Confluent REST Proxy | API 最完善，Schema 集成深度 |
| Kubernetes 环境 | Strimzi Bridge | CRD 原生管理，轻量 |
| 替代 Confluent 企业版 | Karapace | 完全兼容 + 开源 |
| 需要多协议 + 治理 | Gravitee | 唯一完整的协议介导 + 治理方案 |
| 已有 API Gateway | APISIX/Kong + REST Proxy | 复用已有基础设施 |
| 外部开放 + 安全管控 | Gravitee / Kafka Gateway | 原生协议 + 治理层 |
| 内部简单集成 | Strimzi Bridge 或自研 | 轻量即可 |

---

## 五、对 Fluss Gateway 开发的建议

### 5.1 需要回答的关键设计问题

1. **目标用户是谁？**
   - 内部 Flink/Spark 作业 → 可能不需要 REST Gateway
   - 外部应用接入 → 需要完整 REST API + 安全层
   - 前端/移动端 → 需要 WebSocket/SSE 支持

2. **Fluss 与 Kafka 的兼容层如何处理？**
   - Fluss 已有 `fluss-kafka` 模块，Gateway 是否基于此模块做 REST 桥接？
   - 还是直接基于 `fluss-client` / `fluss-rpc` 做 REST 映射？

3. **列式格式（Arrow）如何通过 HTTP 暴露？**
   - Arrow IPC 格式直接通过 HTTP body 传输
   - Arrow Flight RPC（基于 gRPC）
   - 转换为 JSON（牺牲列式优势）

### 5.2 推荐的架构方向

基于调研结果，建议 Fluss Gateway 采用分层设计：

```
┌─────────────────────────────────────────────────────────────────┐
│                        Fluss Gateway                            │
├─────────────┬──────────────┬──────────────┬─────────────────────┤
│  HTTP REST  │  WebSocket   │     SSE      │   Arrow Flight      │
│  (管理+生产) │  (实时消费)   │  (推送消费)   │   (高性能通道)       │
├─────────────┴──────────────┴──────────────┴─────────────────────┤
│                     中间层 (Middleware)                          │
│  认证/授权 │ 限流 │ 序列化转换 │ 路由 │ 监控/追踪               │
├─────────────────────────────────────────────────────────────────┤
│                     Fluss Client / RPC                          │
│                  (连接 Fluss Server)                             │
└─────────────────────────────────────────────────────────────────┘
```

### 5.3 可借鉴的设计元素

| 来源 | 可借鉴点 |
|---|---|
| Confluent REST Proxy | v3 API 的 RESTful 资源模型设计、错误码体系、Schema 集成模式 |
| Strimzi Bridge | OpenAPI 规范优先、Vert.x 异步框架选型、健康检查端点设计 |
| Karapace | Python 异步架构的 Leader/Replica HA 模型 |
| Gravitee | 多协议入口设计（HTTP/WS/SSE）、按 API 动态配置、CloudEvents 标准化 |
| APISIX | 插件化架构、动态路由、热更新配置 |

### 5.4 应避免的设计陷阱

1. **避免有状态消费者模型**：这是现有所有 Kafka REST Proxy 的最大痛点，严重影响水平扩展
2. **避免仅支持 HTTP 轮询消费**：对于流式数据场景延迟不可接受
3. **避免全局生产者配置**：限制了多租户和灵活性
4. **避免安全性外包**：至少应内置基础的认证和 TLS 支持
5. **避免忽略列式数据优势**：Fluss 的 Arrow 列式格式是差异化优势，Gateway 应充分利用

---

## 六、参考资料

- Confluent Kafka REST Proxy: https://docs.confluent.io/platform/current/kafka-rest/index.html
- Confluent REST Proxy API Reference: https://docs.confluent.io/platform/current/kafka-rest/api.html
- Confluent REST Proxy 源码: https://github.com/confluentinc/kafka-rest
- Strimzi Kafka Bridge: https://strimzi.io/docs/bridge/0.23.1/
- Karapace: https://github.com/Aiven-Open/karapace
- Gravitee Kafka Gateway: https://documentation.gravitee.io/apim/4.8/kafka-gateway
- Apache APISIX Kafka 集成: https://apisix.apache.org/docs/apisix/plugins/kafka-proxy/
- Kafka 暴露机制对比: https://www.gravitee.io/blog/comparing-kafka-exposure-mechanisms
- Confluent vs Gravitee 对比: https://www.gravitee.io/blog/confluent-rest-proxy-vs-gravitee-kafka-support
- Apache Fluss: https://github.com/apache/fluss
