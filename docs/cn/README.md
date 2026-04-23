# Fluss Gateway

[Apache Fluss](https://github.com/apache/fluss) 的 REST API 网关。将 Fluss 协议转换为 HTTP/JSON，使任何 HTTP 客户端都能与 Fluss 交互，无需原生协议支持。

[English documentation](../README.md)

## 功能特性

- **元数据 API** — 列出数据库、表，获取表结构信息
- **KV 操作** — 按主键的点查和批量查询
- **日志扫描** — 扫描日志表（仅追加），支持超时/限制
- **写入 API** — 追加到日志表，对 PK 表执行 upsert/delete
- **身份穿透** — HTTP Basic Auth → SASL/PLAIN → Fluss ACL 强制 enforcement
- **每用户连接池** — 基于 moka 的缓存，空闲淘汰（默认 500 连接）
- **优雅关闭** — 监听 SIGTERM/SIGINT，排空请求，清理连接

## 安装方式

### 1. 源码编译

需要 Rust 工具链。适合开发和测试。

```bash
git clone https://github.com/apache/fluss-gateway.git
cd fluss-gateway
cargo build --release
./target/release/fluss-gateway serve --fluss-coordinator=localhost:9123
```

### 2. 二进制部署

从 [GitHub Releases](https://github.com/apache/fluss-gateway/releases) 下载预编译二进制。支持 Linux x86_64/aarch64。

```bash
# 下载并解压
tar xzf fluss-gateway-x86_64-linux.tar.gz

# 一键安装（自动创建 systemd 服务、配置文件、系统用户）
sudo bash install.sh --coordinator=fluss-server:9123

# 或手动安装
sudo cp fluss-gateway /usr/local/bin/
sudo cp gateway.toml.example /etc/fluss-gateway/gateway.toml
# 编辑 gateway.toml 配置你的参数
sudo systemctl enable --now fluss-gateway
```

### 3. Docker 部署

GHCR 提供多架构镜像（`ghcr.io/apache/fluss-gateway`），支持 linux/amd64 和 linux/arm64。

```bash
# 开发环境（包含 Fluss 集群）
docker compose -f deploy/docker/docker-compose.dev.yml up -d

# 生产环境（仅 Gateway，连接外部 Fluss）
FLUSS_COORDINATOR=fluss-prod:9123 docker compose -f deploy/docker/docker-compose.prod.yml up -d
```

## REST API

基础 URL: `http://localhost:8080`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/v1/_databases` | 列出数据库 |
| GET | `/v1/{db}/_tables` | 列出数据库中的表 |
| GET | `/v1/{db}/{table}/_info` | 获取表结构信息 |
| GET | `/v1/{db}/{table}?pk.col=val` | 按主键点查 |
| POST | `/v1/{db}/{table}/batch` | 按主键批量查询 |
| POST | `/v1/{db}/{table}/scan` | 扫描日志表 |
| POST | `/v1/{db}/{table}/rows` | 写入行（自动路由到 append/upsert/delete） |

完整 API 文档：[English](../en/API.md) | [Chinese](./API.md)

## 配置

CLI 参数 > 配置文件 > 默认值。

```bash
./target/release/fluss-gateway serve --host=0.0.0.0 --port=8080 --fluss-coordinator=localhost:9123
# 或
./target/release/fluss-gateway serve --config=/etc/fluss-gateway/gateway.toml
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--host` | `0.0.0.0` | HTTP 绑定地址 |
| `--port` | `8080` | HTTP 监听端口 |
| `--fluss-coordinator` | `localhost:9123` | Fluss coordinator 地址 |
| `--auth-type` | `none` | `none` 或 `passthrough` |
| `--config` | `gateway.toml` | 配置文件路径 |
| `--pool-max-connections` | `500` | 连接池最大连接数 |
| `--log-level` | `info` | 日志级别 |

## 生命周期管理

```bash
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123  # 后台启动
./bin/fluss-gateway.sh status                                         # 查看状态
./bin/fluss-gateway.sh stop                                           # 优雅停止
./bin/fluss-gateway.sh restart -- --fluss-coordinator=localhost:9123  # 重启
```

## 架构

```
HTTP 客户端                Fluss Gateway                        Fluss 集群
+-----------+              +------------------+                 +------------------+
|  curl /   |  HTTP REST   |  协议层           |  SASL/PLAIN     |  Coordinator     |
|  JS SDK   | ---------->  |  (Axum handlers) | ------------>   |  Tablet Servers  |
|  ...      |  <---------- |  认证中间件        | <------------  |  ZooKeeper       |
+-----------+              |                  |                 +------------------+
                           |  后端层           |
                           |  (FlussBackend)  |
                           |                  |
                           |  连接池           |
                           |  (moka cache)    |
                           +------------------+
```

完整架构文档：[English](../en/ARCHITECTURE.md) | [Chinese](./ARCHITECTURE.md)

## 测试

```bash
# 单元测试
cargo test

# 集成测试
./bin/fluss-gateway.sh start -- --host=127.0.0.1 --port=8080 --fluss-coordinator=localhost:9123
cargo test --test integration
./bin/fluss-gateway.sh stop
```

## 部署文件

| 文件 | 用途 |
|------|------|
| `deploy/docker/Dockerfile` | 最小运行时镜像（需先本地编译二进制） |
| `deploy/docker/docker-compose.dev.yml` | 开发：仅 Fluss 集群，Gateway 本地运行 |
| `deploy/docker/docker-compose.prod.yml` | 生产：仅 Gateway，连接外部 Fluss |
| `deploy/systemd/fluss-gateway.service` | systemd 单元文件模板 |
| `deploy/config/gateway.toml.example` | 配置文件示例 |
| `deploy/install.sh` | Linux 一键安装脚本（二进制 + systemd + 配置） |

完整部署指南：[English](../en/DEPLOY.md) | [Chinese](./DEPLOY.md)

## 版本发布

项目采用语义化版本（SemVer），使用 `cargo-release` 一条命令完成版本发布。

发布指南：[English](../en/RELEASE.md) | [Chinese](./RELEASE.md)

## License

Apache License 2.0
