# Contributing to Fluss Gateway

Thank you for your interest in contributing to Fluss Gateway! This guide will help you get started with development and understand the contribution process.

感谢您对 Fluss Gateway 的贡献感兴趣！本指南将帮助您开始开发并了解贡献流程。

---

## Table of Contents / 目录

- [Welcome / 欢迎](#welcome--欢迎)
- [Getting Started / 入门](#getting-started--入门)
  - [Prerequisites / 前置要求](#prerequisites--前置要求)
  - [Setup / 环境设置](#setup--环境设置)
  - [Build / 构建](#build--构建)
- [Running Tests / 运行测试](#running-tests--运行测试)
  - [Unit Tests / 单元测试](#unit-tests--单元测试)
  - [Integration Tests / 集成测试](#integration-tests--集成测试)
  - [Code Coverage / 代码覆盖率](#code-coverage--代码覆盖率)
- [Code Style / 代码风格](#code-style--代码风格)
- [Project Structure / 项目结构](#project-structure--项目结构)
- [Making Changes / 进行修改](#making-changes--进行修改)
- [Submitting a PR / 提交 PR](#submitting-a-pr--提交-pr)
  - [Title Format / 标题格式](#title-format--标题格式)
  - [Description Requirements / 描述要求](#description-requirements--描述要求)
  - [Checklist / 检查清单](#checklist--检查清单)
- [Reporting Bugs / 报告 Bug](#reporting-bugs--报告-bug)
- [Code of Conduct / 行为准则](#code-of-conduct--行为准则)
- [License / 许可证](#license--许可证)

---

## Welcome / 欢迎

We welcome contributions from everyone. Whether you are fixing a typo, reporting a bug, proposing a new feature, or writing code, your help is greatly appreciated!

我们欢迎所有人的贡献。无论您是修复拼写错误、报告 bug、提出新功能，还是编写代码，您的帮助都备受感激！

Types of contributions we accept: / 我们接受的贡献类型：

- Bug fixes / Bug 修复
- Feature enhancements / 功能增强
- Documentation improvements / 文档改进
- Performance optimizations / 性能优化
- Test coverage improvements / 测试覆盖率提升
- Translation contributions / 翻译贡献

---

## Getting Started / 入门

### Prerequisites / 前置要求

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.70+ (stable) | Language runtime / 语言运行时 |
| Cargo | Bundled with Rust | Package manager / 包管理器 |
| protoc | Any recent version | Protocol Buffers compiler / 协议编译器 |
| Docker + Compose | Latest | Run Fluss cluster locally / 本地运行 Fluss 集群 |

**Install Rust and Cargo / 安装 Rust 和 Cargo:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

**Install protoc / 安装 protoc:**

```bash
# macOS
brew install protobuf

# Ubuntu / Debian
sudo apt-get update && sudo apt-get install -y protobuf-compiler

# Windows (via Chocolatey)
choco install protoc
```

### Setup / 环境设置

1. **Fork and clone the repository / Fork 并克隆仓库:**

```bash
git clone https://github.com/<your-username>/fluss-gateway.git
cd fluss-gateway
git remote add upstream https://github.com/<org>/fluss-gateway.git
```

2. **Start a local Fluss cluster / 启动本地 Fluss 集群:**

```bash
docker compose -f deploy/docker/docker-compose.dev.yml up -d
```

3. **Verify the cluster is running / 验证集群运行状态:**

```bash
docker compose -f deploy/docker/docker-compose.dev.yml ps
```

### Build / 构建

```bash
# Debug build (faster compilation / 更快的编译)
cargo build

# Release build (optimized / 优化版本)
cargo build --release

# Run the gateway / 运行网关
./target/release/fluss-gateway serve --fluss-coordinator=localhost:9123

# Or use the lifecycle script / 或使用生命周期脚本
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123
```

---

## Running Tests / 运行测试

### Unit Tests / 单元测试

```bash
# Run all unit tests / 运行所有单元测试
cargo test

# Run unit tests for a specific binary / 运行特定 binary 的单元测试
cargo test --bin fluss-gateway

# Run a single test by name / 按名称运行单个测试
cargo test test_name_here
```

### Integration Tests / 集成测试

Integration tests require a running Fluss cluster. They use `setup.rs` to prepare the test environment and `teardown.rs` to clean up.

集成测试需要运行中的 Fluss 集群。它们使用 `setup.rs` 准备测试环境，使用 `teardown.rs` 清理。

```bash
# Method 1: Full integration test flow (recommended) / 方法 1：完整集成测试流程（推荐）

# Start the gateway
./bin/fluss-gateway.sh start -- --host=127.0.0.1 --port=8080 --fluss-coordinator=localhost:9123

# Run integration tests
cargo test --test integration

# Stop the gateway
./bin/fluss-gateway.sh stop

# Method 2: Use the setup/teardown test harness / 方法 2：使用 setup/teardown 测试工具
cargo test --test setup
cargo test --test integration
cargo test --test teardown
```

### Code Coverage / 代码覆盖率

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Run tests with coverage
cargo llvm-cov --all-targets

# Generate LCOV report
cargo llvm-cov --lcov --output-path lcov.info
```

---

## Code Style / 代码风格

All code must pass formatting and linting checks before being submitted.

所有代码在提交前必须通过格式化和 linting 检查。

### Format / 格式化

We use `rustfmt` for code formatting. Run before every commit:

我们使用 `rustfmt` 进行代码格式化。每次提交前运行：

```bash
cargo fmt --all
```

The CI checks formatting with: / CI 使用以下命令检查格式化：

```bash
cargo fmt --all -- --check
```

### Clippy / 静态分析

We use `clippy` for static analysis. All clippy warnings must be resolved:

我们使用 `clippy` 进行静态分析。必须解决所有 clippy 警告：

```bash
cargo clippy --all-targets -- -D warnings
```

### Pre-commit Checklist / 提交前检查清单

Before committing code, run the following commands:

提交代码前，请运行以下命令：

```bash
# 1. Format code / 格式化代码
cargo fmt --all

# 2. Run clippy / 运行静态分析
cargo clippy --all-targets -- -D warnings

# 3. Run unit tests / 运行单元测试
cargo test --bin fluss-gateway

# 4. Verify build / 验证构建
cargo build --release
```

---

## Project Structure / 项目结构

```
fluss-gateway/
├── src/
│   ├── main.rs              # Entry point and CLI parsing / 入口点和 CLI 解析
│   ├── config.rs            # Configuration management / 配置管理
│   ├── pool.rs              # Connection pool (moka cache) / 连接池
│   ├── backend/             # Backend layer - FlussBackend trait / 后端层
│   ├── server/              # Protocol frontend - Axum handlers / 协议前端层
│   │   ├── mod.rs           # Server startup / 服务器启动
│   │   ├── rest/            # REST API route handlers / REST API 路由处理
│   │   └── middleware/      # Auth and rate-limit middleware / 认证和限流中间件
│   └── types/               # Type definitions and conversions / 类型定义和转换
├── tests/
│   ├── common.rs            # Shared test utilities / 共享测试工具
│   ├── integration.rs       # Integration test cases / 集成测试用例
│   ├── setup.rs             # Test environment setup / 测试环境设置
│   └── teardown.rs          # Test environment cleanup / 测试环境清理
├── deploy/
│   ├── docker/              # Dockerfile and compose files / Docker 文件
│   ├── systemd/             # systemd unit template / systemd 单元模板
│   └── config/              # Example configuration / 示例配置
├── docs/
│   ├── en/                  # English documentation / 英文文档
│   └── cn/                  # Chinese documentation / 中文文档
├── bin/
│   └── fluss-gateway.sh     # Lifecycle management script / 生命周期管理脚本
├── Cargo.toml               # Project manifest / 项目清单
└── README.md                # Project overview / 项目概览
```

### Architecture Layers / 架构分层

The project follows a four-layer architecture:

项目采用四层架构：

1. **Protocol Frontend (server/)** - HTTP layer: Axum handlers, routing, middleware / HTTP 层：Axum 处理程序、路由、中间件
2. **Service Layer** - Business logic orchestration / 业务逻辑编排
3. **Backend Layer (backend/)** - FlussBackend trait, Fluss protocol abstraction / FlussBackend trait，Fluss 协议抽象
4. **Connection Pool (pool.rs)** - Per-user connection caching with moka / 每用户连接缓存

---

## Making Changes / 进行修改

1. **Create a branch from main / 从 main 创建分支:**

```bash
git checkout main
git pull upstream main
git checkout -b feat/your-feature-name   # or fix/bug-name
```

2. **Make your changes and commit / 进行修改并提交:**

```bash
git add -A
git commit -m "feat: add batch lookup support"
```

3. **Push to your fork / 推送到你的 fork:**

```bash
git push origin feat/your-feature-name
```

### Branch Naming Convention / 分支命名规范

| Prefix | Usage / 用途 | Example |
|--------|-------------|---------|
| `feat/` | New features / 新功能 | `feat/add-streaming-scan` |
| `fix/` | Bug fixes / Bug 修复 | `fix/connection-pool-leak` |
| `docs/` | Documentation / 文档 | `docs/update-api-reference` |
| `refactor/` | Code refactoring / 代码重构 | `refactor/extract-auth-middleware` |
| `test/` | Test improvements / 测试改进 | `test/add-edge-cases` |
| `chore/` | Build/config changes / 构建配置更改 | `chore/update-dependencies` |

---

## Submitting a PR / 提交 PR

### Title Format / 标题格式

Use [Conventional Commits](https://www.conventionalcommits.org/) style for PR titles:

PR 标题请使用 [Conventional Commits](https://www.conventionalcommits.org/) 格式：

```
<type>: <short description>
```

Types: / 类型：

| Type | When to use / 使用场景 |
|------|----------------------|
| `feat` | New feature / 新功能 |
| `fix` | Bug fix / Bug 修复 |
| `docs` | Documentation changes / 文档更改 |
| `refactor` | Code refactoring (no behavior change) / 代码重构（无行为变更） |
| `test` | Adding or fixing tests / 添加或修复测试 |
| `chore` | Build, CI, dependency changes / 构建、CI、依赖更改 |
| `perf` | Performance improvement / 性能改进 |
| `ci` | CI/CD configuration changes / CI/CD 配置更改 |

**Examples / 示例:**

```
feat: add batch lookup API for PK tables
fix: resolve connection pool deadlock under high load
docs: add Chinese deployment guide
refactor: extract auth middleware into separate module
```

### Description Requirements / 描述要求

Every PR description should include:

每个 PR 描述应包含：

1. **Summary / 概述** - What does this PR do? / 这个 PR 做了什么？
2. **Motivation / 动机** - Why is this change needed? / 为什么需要这个更改？
3. **Testing / 测试** - How was this tested? / 如何测试的？
4. **Breaking Changes / 破坏性变更** - Are there any API or behavior changes? / 是否有 API 或行为变更？

### Checklist / 检查清单

Before submitting a PR, please confirm:

提交 PR 前，请确认：

- [ ] Code is formatted with `cargo fmt --all`
- [ ] No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] Unit tests pass (`cargo test --bin fluss-gateway`)
- [ ] Integration tests pass (if applicable)
- [ ] New code has corresponding tests / 新代码有对应的测试
- [ ] Public APIs have documentation comments (`///`) / 公共 API 有文档注释
- [ ] PR title follows Conventional Commits format / PR 标题遵循 Conventional Commits 格式
- [ ] PR description is clear and complete / PR 描述清晰完整

---

## Reporting Bugs / 报告 Bug

When reporting a bug, please include:

报告 bug 时，请包含以下信息：

1. **Environment / 环境信息:**
   - Fluss Gateway version / Fluss Gateway 版本
   - Rust version (`rustc --version`)
   - OS and architecture / 操作系统和架构
   - Fluss cluster version / Fluss 集群版本

2. **Steps to Reproduce / 复现步骤:**
   - Detailed, step-by-step instructions / 详细的分步说明
   - Include configuration used / 包含使用的配置
   - Provide sample requests/responses if applicable / 如适用，提供示例请求/响应

3. **Expected vs Actual Behavior / 预期与实际行为:**
   - What you expected to happen / 您期望发生什么
   - What actually happened / 实际发生了什么

4. **Logs / 日志:**
   - Gateway logs with `--log-level=debug` / 使用 `--log-level=debug` 的网关日志
   - Any stack traces or error messages / 任何堆栈跟踪或错误消息

Open an issue on GitHub with the "bug" label to report.

请在 GitHub 上使用 "bug" 标签提交 issue 来报告。

---

## Code of Conduct / 行为准则

This project follows the [Apache Software Foundation Code of Conduct](https://www.apache.org/foundation/policies/conduct.html).

本项目遵循 [Apache 软件基金会行为准则](https://www.apache.org/foundation/policies/conduct.html)。

By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

参与本项目即表示您承诺遵守此行为准则。请向项目维护者报告不可接受的行为。

---

## License / 许可证

By contributing to Fluss Gateway, you agree that your contributions will be licensed under the Apache License 2.0.

通过向 Fluss Gateway 贡献代码，您同意您的贡献将根据 Apache License 2.0 进行授权。
