# Fluss Gateway 项目文档

## 项目概述

Fluss Gateway 是一个基于 Rust 的 REST API 网关,为 Apache Fluss 流存储系统提供 HTTP/REST 接入层。项目基于 FIP-32(多协议查询网关)扩展,补充写入能力、流式消费和治理能力。

**技术栈**: Rust + fluss-rust + DataFusion + Axum

**定位**: FIP-32 的补充而非替代 —— FIP-32 解决只读查询的多协议访问,本方案补充写入、流式消费和治理。

## 文档说明

本文档为中文版。英文版位于 [docs/en/](../en/) 目录。

| 语言 | 文档路径 |
|------|---------|
| 英文(默认) | [docs/en/PROJECT.md](../en/PROJECT.md) |
| 中文 | [docs/cn/PROJECT.md](./PROJECT.md) |

## 核心文档索引

| 文档 | 内容 |
|------|------|
| [架构设计](./ARCHITECTURE.md) | 四层架构、项目结构、核心组件、实施计划 |
| [API 设计](./API.md) | REST API 端点定义、请求/响应格式、错误码体系 |
| [部署指南](./DEPLOY.md) | Docker、systemd、物理机部署 |
| [项目进度](./PROGRESS.md) | 实现状态、功能清单、技术债 |

## 关键参考文件

| 文件 | 用途 |
|------|------|
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/connection.rs` | FlussConnection 入口点 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/admin.rs` | FlussAdmin 元数据操作 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/table/scanner.rs` | LogScanner 消费原语 |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/write/` | WriterClient 写入路径 |
| `~/IdeaProjects/fluss-community/fluss-rpc/src/main/proto/FlussApi.proto` | RPC 协议定义 |
