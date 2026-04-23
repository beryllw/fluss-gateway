# 版本管理与发布指南

本文档说明 Fluss Gateway 的版本管理策略和发布流程。

[English version](../en/RELEASE.md)

## 版本号规范

项目采用 [语义化版本（SemVer）](https://semver.org/lang/zh-CN/)，格式为 `MAJOR.MINOR.PATCH`：

| 类型 | 何时递增 | 示例 |
|------|---------|------|
| MAJOR | 不兼容的 API 变更 | `1.0.0` → `2.0.0` |
| MINOR | 向后兼容的功能新增 | `0.1.0` → `0.2.0` |
| PATCH | 向后兼容的问题修复 | `0.1.0` → `0.1.1` |

Pre-release 版本使用 `-` 后缀，如 `1.0.0-beta.1`。

## 版本号位置

`Cargo.toml` 中的 `version` 字段是项目版本的**唯一来源（Single Source of Truth）**。以下位置自动从中读取版本号，无需手动维护：

| 位置 | 获取方式 | 说明 |
|------|---------|------|
| `Cargo.toml` | 直接定义 | 唯一需要手动修改的地方 |
| CLI `--version` | `clap` 自动读取 `CARGO_PKG_VERSION` | 运行 `fluss-gateway --version` 显示版本 |
| OpenAPI 文档 | `env!("CARGO_PKG_VERSION")` | Swagger UI 显示的 API 版本 |
| Git tag | cargo-release 自动创建 | 格式为 `v{version}`，如 `v0.2.0` |

## 发布流程

### 前置准备（一次性）

```bash
# 安装 cargo-release
cargo install cargo-release
```

### 标准发布

```bash
# 1. 确保在 main 分支，代码是最新的
git checkout main
git pull

# 2. 确保 CI 通过
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --bin fluss-gateway

# 3. 执行发布（先 dry run 检查）
cargo release patch           # dry run：查看将执行的操作
cargo release patch --execute # 实际执行
```

`cargo release` 会自动完成以下步骤：

1. 修改 `Cargo.toml` 中的 `version`（如 `0.1.0` → `0.1.1`）
2. 提交变更：`chore: release v0.1.1`
3. 创建 Git tag：`v0.1.1`
4. 推送 commit 和 tag 到远程仓库

推送后，GitHub Actions 的 Release workflow 自动触发，完成：
- 多平台二进制构建（Linux x86_64/aarch64, macOS aarch64）
- Docker 多架构镜像构建并推送到 GHCR
- 创建 GitHub Release（draft 状态，需手动确认发布）

### 版本类型选择

```bash
cargo release patch            # 0.1.0 → 0.1.1  (bug 修复)
cargo release minor            # 0.1.0 → 0.2.0  (新功能)
cargo release major            # 0.1.0 → 1.0.0  (破坏性变更)
cargo release 0.3.0            # 指定具体版本号
cargo release 1.0.0-beta.1     # pre-release 版本
```

### 手动发布（备用）

如果不使用 cargo-release，也可以手动操作：

```bash
# 1. 修改 Cargo.toml 中的 version
#    version = "0.1.0" → version = "0.2.0"

# 2. 更新 Cargo.lock
cargo check

# 3. 提交
git add Cargo.toml Cargo.lock
git commit -m "chore: release v0.2.0"

# 4. 打 tag
git tag v0.2.0

# 5. 推送
git push && git push --tags
```

### 手动触发 CI 发布

Release workflow 也支持通过 GitHub Actions 手动触发（`workflow_dispatch`），适用于热修复或测试发布场景：

1. 进入 GitHub 仓库 → Actions → Release workflow
2. 点击 "Run workflow"
3. 输入版本号（如 `v0.2.0`），该字段为必填

## 分支策略

项目采用简单的 trunk-based 开发模式：

```
main (默认分支)
  ├── feature/xxx  →  PR → merge to main
  ├── fix/xxx      →  PR → merge to main
  └── v0.2.0 (tag) ← 从 main 打 tag 发布
```

- **main 分支**：始终保持可发布状态，所有功能和修复通过 PR 合入
- **不使用 release 分支**：直接从 main 打 tag 发布
- **Tag 即发布**：推送 `v*` 格式的 tag 后 CI 自动构建发布

## CI/CD 流水线

| Workflow | 触发条件 | 作用 |
|----------|---------|------|
| CI (`ci.yml`) | PR 到 main | 格式检查、Clippy、单元测试、集成测试、构建检查 |
| Release (`release.yml`) | 推送 `v*` tag 或手动触发 | 多平台构建、Docker 镜像、创建 GitHub Release |

## 相关文件

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | 版本号定义（唯一来源） |
| `release.toml` | cargo-release 配置 |
| `.github/workflows/release.yml` | Release CI workflow |
| `.github/workflows/ci.yml` | PR CI workflow |
