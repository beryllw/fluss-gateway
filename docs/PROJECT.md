# Fluss Gateway 项目文档

## 项目概述

Fluss Gateway 是一个基于 Rust 的 REST API 网关，为 Apache Fluss 流存储系统提供 HTTP/REST 接入层。项目基于 FIP-32（多协议查询网关）扩展，补充写入能力、流式消费和治理能力。

**技术栈**: Rust + fluss-rust + DataFusion + Axum

**定位**: FIP-32 的补充而非替代 —— FIP-32 解决只读查询的多协议访问，本方案补充写入、流式消费和治理。

## 目录结构

```
fluss-gateway/
├── docs/                          # 项目文档（本目录）
│   ├── PROJECT.md                 # 项目概览（本文件）
│   ├── ARCHITECTURE.md            # 架构设计文档
│   └── API.md                     # API 设计文档
├── research/                      # 调研文档
│   ├── kafka-rest-proxy-gateway-research.md   # Kafka REST 方案调研
│   └── kafka-rest-source-code-analysis.md     # Kafka REST 源码分析
├── .claude/                       # Agent 约束与配置
│   ├── CLAUDE.md                  # 每次执行必须加载的约束
│   └── settings.local.json        # 权限配置
└── src/                           # 源代码（待创建）
```

## 核心文档索引

| 文档 | 内容 |
|------|------|
| [架构设计](./ARCHITECTURE.md) | 四层架构、项目结构、核心组件、实施计划 |
| [API 设计](./API.md) | REST API 端点定义、请求/响应格式、错误码体系 |

## 参考资料

| 来源 | 说明 |
|------|------|
| FIP-32 | 多协议查询网关提案（只读） |
| [FIP Gateway 设计文档](~/AiWorkSpace/fluss-fip/FIP-REST-API/fluss-gateway-design.md) | 本方案完整设计 |
| [Kafka REST 调研](../research/kafka-rest-proxy-gateway-research.md) | Kafka REST Proxy 方案对比 |
| [Kafka REST 源码分析](../research/kafka-rest-source-code-analysis.md) | Confluent kafka-rest 深度解读 |

## 关键参考文件

| 文件 | 用途 |
|------|------|
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/connection.rs` | FlussConnection 入口点 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/admin.rs` | FlussAdmin 元数据操作 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/table/scanner.rs` | LogScanner 消费原语 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/write/` | WriterClient 写入路径 |
| `~/IdeaProjects/fluss-community/fluss-rpc/src/main/proto/FlussApi.proto` | RPC 协议定义 |
