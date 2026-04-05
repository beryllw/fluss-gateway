# Fluss Gateway

REST API Gateway for [Apache Fluss](https://github.com/apache/fluss). Translates the Fluss protocol into HTTP/JSON, enabling any HTTP client to interact with Fluss without native protocol support.

[Chinese documentation (中文文档)](docs/cn/)

## Features

- **Metadata API** — list databases, tables, get table schema info
- **KV Operations** — point lookup and batch lookup by primary key
- **Log Scanning** — scan log tables (append-only) with timeout/limit
- **Write API** — append to log tables, upsert/delete on PK tables
- **Identity Passthrough** — HTTP Basic Auth → SASL/PLAIN → Fluss ACL enforcement
- **Per-user Connection Pool** — moka-based cache with idle eviction (500 connections default)
- **Graceful Shutdown** — listens for SIGTERM/SIGINT, drains requests, cleans up connections

## Quickstart

```bash
# 1. Start Fluss cluster
docker compose -f deploy/docker/docker-compose.dev.yml up -d

# 2. Build & run
cargo build --release
./target/release/fluss-gateway serve --host=0.0.0.0 --port=8080 --fluss-coordinator=localhost:9123

# Or use the lifecycle script (background)
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123
```

## Lifecycle Management

```bash
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123  # Start in background
./bin/fluss-gateway.sh status                                         # Check status
./bin/fluss-gateway.sh stop                                           # Graceful stop
./bin/fluss-gateway.sh restart -- --fluss-coordinator=localhost:9123  # Restart
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

## Deployment

| File | Purpose |
|------|---------|
| `deploy/docker/Dockerfile` | Minimal runtime image (build binary locally first) |
| `deploy/docker/docker-compose.dev.yml` | Dev: Fluss cluster only, gateway runs locally |
| `deploy/docker/docker-compose.prod.yml` | Prod: gateway only, connects to external Fluss |
| `deploy/systemd/fluss-gateway.service` | systemd unit file template |
| `deploy/config/gateway.toml.example` | Example configuration file |

Full deployment guide: [English](docs/en/DEPLOY.md) | [Chinese](docs/cn/DEPLOY.md)

### Docker Production

```bash
cargo build --release
docker build -f deploy/docker/Dockerfile .
FLUSS_COORDINATOR=fluss-prod:9123 docker compose -f deploy/docker/docker-compose.prod.yml up -d
```

### Bare Metal / systemd

```bash
sudo cp target/release/fluss-gateway /usr/local/bin/
sudo cp deploy/config/gateway.toml.example /etc/fluss-gateway/gateway.toml
sudo cp deploy/systemd/fluss-gateway.service /etc/systemd/system/
sudo systemctl enable --now fluss-gateway
```

## Architecture

```
HTTP Client                    Fluss Gateway                           Fluss Cluster
+-----------+                  +------------------+                    +------------------+
|  curl /    |  HTTP REST     |  Protocol Layer  |  SASL/PLAIN       |  Coordinator     |
|  JS SDK    | -------------> |  (Axum handlers) | ----------------> |  Tablet Servers  |
|  ...       |  <------------- |  Auth Middleware |  <---------------- |  ZooKeeper       |
+-----------+                  |                  |                    +------------------+
                               |  Backend Layer   |
                               |  (FlussBackend)  |
                               |                  |
                               |  Connection Pool |
                               |  (moka cache)    |
                               +------------------+
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests (12/12 passing)
./bin/fluss-gateway.sh start -- --host=127.0.0.1 --port=8080 --fluss-coordinator=localhost:9123
cargo test --test integration
./bin/fluss-gateway.sh stop
```

## License

Apache License 2.0
