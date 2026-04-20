# Fluss Gateway Project Documentation

## Project Overview

Fluss Gateway is a REST API gateway built in Rust that provides HTTP/REST access to the Apache Fluss streaming storage system. The project extends FIP-32 (Multi-Protocol Query Gateway) by adding write capabilities, streaming consumption, and governance features.

**Tech Stack**: Rust + fluss-rust + DataFusion + Axum

**Positioning**: A complement to FIP-32, not a replacement — FIP-32 solves multi-protocol read-only access, while this solution adds write, streaming consumption, and governance.

## Directory Structure

```
fluss-gateway/
├── docs/                          # Project documentation
│   ├── en/                        # English documents (default)
│   │   ├── PROJECT.md             # Project overview (this file)
│   │   ├── ARCHITECTURE.md        # Architecture design
│   │   ├── API.md                 # API design
│   │   ├── DEPLOY.md              # Deployment guide
│   │   └── PROGRESS.md            # Project progress
│   └── cn/                        # Chinese documents
│       ├── PROJECT.md             # Project overview
│       ├── ARCHITECTURE.md        # Architecture design
│       ├── API.md                 # API design
│       ├── DEPLOY.md              # Deployment guide
│       └── PROGRESS.md            # Project progress
├── .claude/                       # Agent constraints and configuration
│   ├── CLAUDE.md                  # Constraints loaded on every run
│   └── settings.local.json        # Permission configuration
└── src/                           # Source code
```

## Core Documentation Index

| Document | Content |
|----------|---------|
| [Architecture Design](./en/ARCHITECTURE.md) | Four-layer architecture, project structure, core components, implementation plan |
| [API Design](./en/API.md) | REST API endpoint definitions, request/response format, error code system |
| [Deployment Guide](./en/DEPLOY.md) | Docker, systemd, bare-metal deployment |
| [Project Progress](./en/PROGRESS.md) | Implementation status, feature checklist, technical debt |

## Chinese Documentation

| Document | Content |
|----------|---------|
| [Architecture Design](./cn/ARCHITECTURE.md) | Four-layer architecture, project structure, core components, implementation plan |
| [API Design](./cn/API.md) | REST API endpoint definitions, request/response format, error code system |
| [Deployment Guide](./cn/DEPLOY.md) | Docker, systemd, bare-metal deployment |
| [Project Progress](./cn/PROGRESS.md) | Implementation status, feature checklist, technical debt |

## Key Reference Files

| File | Purpose |
|------|---------|
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/connection.rs` | FlussConnection entry point |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/admin.rs` | FlussAdmin metadata operations |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/table/scanner.rs` | LogScanner consumption primitives |
| `~/VscodeProjects/fluss-rust/crates/fluss/src/client/write/` | WriterClient write path |
| `~/IdeaProjects/fluss-community/fluss-rpc/src/main/proto/FlussApi.proto` | RPC protocol definition |
