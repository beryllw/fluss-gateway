# Fluss Gateway Deployment Guide

## Directory Structure

```
deploy/
├── docker/
│   ├── Dockerfile                  # Minimal runtime image
│   ├── docker-compose.dev.yml      # Dev: Fluss cluster only
│   └── docker-compose.prod.yml     # Prod: Gateway only
├── systemd/
│   └── fluss-gateway.service       # systemd unit template
└── config/
    └── gateway.toml.example        # Example config
```

---

## 1. Development

```bash
# Start Fluss cluster
docker compose -f deploy/docker/docker-compose.dev.yml up -d

# Build & run gateway
cargo build --release
./target/release/fluss-gateway serve --fluss-coordinator=localhost:9123

# Or use the lifecycle script (background)
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123
```

Ports: `9123` (Coordinator), `9124` (Tablet Server), `2181` (ZooKeeper).

---

## 2. Docker Production

```bash
cargo build --release
docker build -f deploy/docker/Dockerfile . -t fluss-gateway:latest
FLUSS_COORDINATOR=fluss-prod:9123 docker compose -f deploy/docker/docker-compose.prod.yml up -d
```

| Env Var | Default | Description |
|---------|---------|-------------|
| `FLUSS_COORDINATOR` | (required) | Fluss coordinator address |
| `GATEWAY_PORT` | `8080` | HTTP listen port |
| `LOG_LEVEL` | `info` | Log level |

---

## 3. Bare Metal

```bash
sudo cp target/release/fluss-gateway /usr/local/bin/
sudo mkdir -p /etc/fluss-gateway
sudo cp deploy/config/gateway.toml.example /etc/fluss-gateway/gateway.toml
sudo cp deploy/systemd/fluss-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now fluss-gateway
```

---

## 4. Configuration

Priority: CLI args > config file > defaults.

```toml
[server]
host = "0.0.0.0"
port = 8080

[fluss]
coordinator = "localhost:9123"

[auth]
type = "none"           # "none" | "passthrough"
startup_username = ""
startup_password = ""

[pool]
max_connections = 500
idle_timeout_secs = 600

[log]
level = "info"
```

| Mode | Use case | Description |
|------|----------|-------------|
| `none` | Single-tenant, internal network | All requests share static startup credentials |
| `passthrough` | Multi-tenant, ACL required | Each request carries its own credentials via HTTP Basic Auth |

---

## 5. Lifecycle Management

```bash
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123  # Start in background
./bin/fluss-gateway.sh status                                         # Check status
./bin/fluss-gateway.sh stop                                           # Graceful stop
./bin/fluss-gateway.sh restart -- --fluss-coordinator=localhost:9123  # Restart
```

PID file: `/tmp/fluss-gateway.pid` (customizable via `--pid-file=PATH`).

### Graceful Shutdown

On SIGTERM or SIGINT:
1. Stop accepting new requests
2. Wait for in-flight requests to complete (up to 10s)
3. Close connection pool, clear cached Fluss connections
4. Exit process

---

## 6. Troubleshooting

```bash
# Check coordinator reachable
nc -zv fluss-prod 9123

# Check port in use
lsof -i :8080

# View gateway error log
cat /tmp/fluss-gateway.err

# systemd logs
journalctl -u fluss-gateway -f
```
