# Fluss Gateway

REST API Gateway for [Apache Fluss](https://github.com/apache/fluss). Translates the Fluss protocol into HTTP/JSON, enabling any HTTP client to interact with Fluss without native protocol support.

[Chinese documentation (中文文档)](docs/cn/README.md)

## Features

- **Metadata API** — list databases, tables, get table schema info
- **KV Operations** — point lookup and batch lookup by primary key
- **Log Scanning** — scan log tables (append-only) with timeout/limit
- **Write API** — append to log tables, upsert/delete on PK tables
- **Identity Passthrough** — HTTP Basic Auth → SASL/PLAIN → Fluss ACL enforcement
- **Per-user Connection Pool** — moka-based cache with idle eviction (500 connections default)
- **Graceful Shutdown** — listens for SIGTERM/SIGINT, drains requests, cleans up connections

## Installation

### 1. Build from Source

Requires Rust toolchain. Best for development and testing.

```bash
git clone https://github.com/apache/fluss-gateway.git
cd fluss-gateway
cargo build --release
./target/release/fluss-gateway serve --fluss-coordinator=localhost:9123
```

### 2. Binary Deployment

Download pre-built binaries from [GitHub Releases](https://github.com/apache/fluss-gateway/releases). Supports Linux x86_64/aarch64.

```bash
# Download and extract
tar xzf fluss-gateway-x86_64-linux.tar.gz

# One-click install (creates systemd service, config, system user)
sudo bash install.sh --coordinator=fluss-server:9123

# Or install manually
sudo cp fluss-gateway /usr/local/bin/
sudo cp gateway.toml.example /etc/fluss-gateway/gateway.toml
# Edit gateway.toml with your settings
sudo systemctl enable --now fluss-gateway
```

### 3. Docker Deployment

Multi-arch images available on GHCR (`ghcr.io/apache/fluss-gateway`). Supports linux/amd64 and linux/arm64.

```bash
# Development (includes Fluss cluster)
docker compose -f deploy/docker/docker-compose.dev.yml up -d

# Production (Gateway only, connects to external Fluss)
FLUSS_COORDINATOR=fluss-prod:9123 docker compose -f deploy/docker/docker-compose.prod.yml up -d
```

## REST API

Base URL: `http://localhost:8080`

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/_databases` | List databases |
| GET | `/v1/{db}/_tables` | List tables in a database |
| GET | `/v1/{db}/{table}/_info` | Get table schema info |
| GET | `/v1/{db}/{table}?pk.col=val` | Point lookup by primary key |
| POST | `/v1/{db}/{table}/batch` | Batch lookup by primary keys |
| POST | `/v1/{db}/{table}/scan` | Scan log table |
| POST | `/v1/{db}/{table}/rows` | Write rows (auto-routes append/upsert/delete) |

Full API docs: [English](docs/en/API.md) | [Chinese](docs/cn/API.md)

## Configuration

CLI args > config file > defaults.

```bash
./target/release/fluss-gateway serve --host=0.0.0.0 --port=8080 --fluss-coordinator=localhost:9123
# or
./target/release/fluss-gateway serve --config=/etc/fluss-gateway/gateway.toml
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `0.0.0.0` | HTTP bind address |
| `--port` | `8080` | HTTP listen port |
| `--fluss-coordinator` | `localhost:9123` | Fluss coordinator address |
| `--auth-type` | `none` | `none` or `passthrough` |
| `--config` | `gateway.toml` | Config file path |
| `--pool-max-connections` | `500` | Max connections in pool |
| `--log-level` | `info` | Log level |

## Lifecycle Management

```bash
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123  # Start in background
./bin/fluss-gateway.sh status                                         # Check status
./bin/fluss-gateway.sh stop                                           # Graceful stop
./bin/fluss-gateway.sh restart -- --fluss-coordinator=localhost:9123  # Restart
```

## Architecture

```
HTTP Client                 Fluss Gateway                        Fluss Cluster
+-----------+               +------------------+                 +------------------+
|  curl /   |  HTTP REST    |  Protocol Layer  |  SASL/PLAIN     |  Coordinator     |
|  JS SDK   | ----------->  |  (Axum handlers) | ------------>   |  Tablet Servers  |
|  ...      |  <----------- |  Auth Middleware |  <------------  |  ZooKeeper       |
+-----------+               |                  |                 +------------------+
                            |  Backend Layer   |
                            |  (FlussBackend)  |
                            |                  |
                            |  Connection Pool |
                            |  (moka cache)    |
                            +------------------+
```

Full architecture docs: [English](docs/en/ARCHITECTURE.md) | [Chinese](docs/cn/ARCHITECTURE.md)

## Testing

```bash
# Unit tests
cargo test

# Integration tests
./bin/fluss-gateway.sh start -- --host=127.0.0.1 --port=8080 --fluss-coordinator=localhost:9123
cargo test --test integration
./bin/fluss-gateway.sh stop
```

## Deployment Files

| File | Purpose |
|------|---------|
| `deploy/docker/Dockerfile` | Minimal runtime image (build binary locally first) |
| `deploy/docker/docker-compose.dev.yml` | Dev: Fluss cluster only, gateway runs locally |
| `deploy/docker/docker-compose.prod.yml` | Prod: gateway only, connects to external Fluss |
| `deploy/systemd/fluss-gateway.service` | systemd unit file template |
| `deploy/config/gateway.toml.example` | Example configuration file |
| `deploy/install.sh` | One-click installer for Linux (binary + systemd + config) |

Full deployment guide: [English](docs/en/DEPLOY.md) | [Chinese](docs/cn/DEPLOY.md)

## Releasing

The project follows Semantic Versioning (SemVer) and uses `cargo-release` for one-command releases.

Release guide: [English](docs/en/RELEASE.md) | [Chinese](docs/cn/RELEASE.md)

## License

Apache License 2.0
