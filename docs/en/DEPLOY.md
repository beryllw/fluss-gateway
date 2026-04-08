# Fluss Gateway Deployment Guide

## Directory Structure

```
deploy/
├── docker/
│   ├── Dockerfile.standalone      # Self-contained build (includes Fluss cluster)
│   ├── Dockerfile.release          # Release publish image
│   ├── docker-compose.standalone.yml # Dev environment (with Fluss cluster)
│   └── docker-compose.prod.yml     # Production (Gateway only)
├── systemd/
│   └── fluss-gateway.service       # systemd unit template
├── config/
│   └── gateway.toml.example        # Example config
└── install.sh                      # One-click install script
```

---

## Method 1: Docker Compose (One-Click Full Environment)

For quick testing or when you don't have an existing Fluss cluster. Starts Fluss cluster + Gateway together.

```bash
# 1. Clone the repository
git clone <repo-url> fluss-gateway
cd fluss-gateway

# 2. Build the image (first time only, ~5-10 minutes)
docker build -t localhost/fluss-gateway:latest -f deploy/docker/Dockerfile.standalone .

# 3. Start full environment (ZooKeeper + Fluss + Gateway)
docker compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway up -d

# 4. Wait for readiness
sleep 30
curl http://localhost:8080/health
# Should return: {"status":"ok"}
```

**Cleanup**:
```bash
docker compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway down
```

---

## Method 2: Docker Compose (Gateway Only, Connect to Existing Fluss)

For when you already have a Fluss cluster and just need to add Gateway.

Create `docker-compose.yml`:

```yaml
version: "3"
services:
  gateway:
    image: localhost/fluss-gateway:latest
    command:
      - serve
      - --host=0.0.0.0
      - --port=8080
      - --fluss-coordinator=<YOUR_FLUSS_COORDINATOR>:9123
      - --auth-type=none
    ports:
      - "8080:8080"
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 5s
      timeout: 3s
      retries: 10
      start_period: 10s
```

```bash
docker compose up -d
curl http://localhost:8080/health
```

> Without the `command` override, the default CMD is `fluss-gateway serve --host 0.0.0.0 --port 8080`.
> You still need to specify `fluss.coordinator` via config file or environment.

---

## Method 3: GitHub Release Download (Recommended, No Build)

Download pre-built binaries from GitHub Releases — no local compilation needed.

### 1. Choose the Right Release Package

Visit [GitHub Releases](https://github.com/<owner>/fluss-gateway/releases) and pick the tarball for your platform:

| Filename | Platform | Architecture |
|----------|---------|-------------|
| `fluss-gateway-x86_64-linux.tar.gz` | Linux | x86_64 (amd64) |
| `fluss-gateway-aarch64-linux.tar.gz` | Linux | ARM64 |
| `fluss-gateway-aarch64-macos.tar.gz` | macOS | Apple Silicon (M1/M2/M3) |

Each tarball contains:
- `fluss-gateway` — pre-built binary
- `gateway.toml.example` — example config
- `install.sh` — one-click install script (Linux only)

### 2. Download and Install

**Automated install** (recommended, Linux only):

```bash
# Download the tarball for your architecture
tar xzf fluss-gateway-x86_64-linux.tar.gz
cd fluss-gateway-x86_64-linux

# Run the install script (requires sudo)
sudo bash install.sh \
  --coordinator=coordinator-server:9123 \
  --port=8080 \
  --auth-type=none
```

The `install.sh` script automatically:
- Installs the binary to `/usr/local/bin/fluss-gateway`
- Creates the `fluss` system user
- Generates config at `/etc/fluss-gateway/gateway.toml`
- Installs and starts the systemd service
- Runs a health check to confirm the service is ready

**Manual install** (macOS or when you don't want to use the script):

```bash
# Extract
tar xzf fluss-gateway-aarch64-macos.tar.gz
cd fluss-gateway-aarch64-macos

# Install binary
sudo cp fluss-gateway /usr/local/bin/
sudo chmod 755 /usr/local/bin/fluss-gateway

# Create config directory
sudo mkdir -p /etc/fluss-gateway
sudo cp gateway.toml.example /etc/fluss-gateway/gateway.toml

# Edit config (at minimum, change fluss.coordinator)
sudo vi /etc/fluss-gateway/gateway.toml

# Run directly
fluss-gateway serve --config=/etc/fluss-gateway/gateway.toml
```

### 3. One-Line Install via curl

You can also download and install in a single pipeline (Linux x86_64):

```bash
# Set variables (replace with actual version)
VERSION="v0.1.0"
ARCH="x86_64"  # or "aarch64"

# Download and install
curl -fsSL "https://github.com/<owner>/fluss-gateway/releases/download/${VERSION}/fluss-gateway-${ARCH}-linux.tar.gz" \
  | tar xz
cd "fluss-gateway-${ARCH}-linux"
sudo bash install.sh --coordinator=localhost:9123
```

---

## Method 4: Build from Source + systemd Deployment

No Docker, suitable for production or custom development.

### 1. Build the Binary

```bash
git clone <repo-url> fluss-gateway
cd fluss-gateway
cargo build --release
sudo cp target/release/fluss-gateway /usr/local/bin/
sudo chmod 755 /usr/local/bin/fluss-gateway
```

### 2. Install the Service

```bash
# Create user and config directories
sudo useradd --system --no-create-home fluss
sudo mkdir -p /etc/fluss-gateway
sudo mkdir -p /var/log/fluss-gateway
sudo chown fluss:fluss /var/log/fluss-gateway

# Install config file
sudo cp deploy/config/gateway.toml.example /etc/fluss-gateway/gateway.toml
sudo vi /etc/fluss-gateway/gateway.toml
# At minimum, change fluss.coordinator to your Fluss address

# Install systemd service
sudo cp deploy/systemd/fluss-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable fluss-gateway
sudo systemctl start fluss-gateway

# Check status
sudo systemctl status fluss-gateway

# View logs
sudo journalctl -u fluss-gateway -f
```

### 3. Configuration File Reference

`/etc/fluss-gateway/gateway.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8080

[fluss]
coordinator = "localhost:9123"   # Change to your Fluss Coordinator address

[auth]
type = "none"  # "none" | "passthrough"

[pool]
max_connections = 500
idle_timeout_secs = 600

[log]
level = "info"  # "debug" | "info" | "warn" | "error"
```

### 4. CLI Options

Priority: CLI args > config file > defaults.

```bash
fluss-gateway serve [OPTIONS]

Options:
      --host <HOST>                        Host to bind to
      --port <PORT>                        Port to listen on
      --fluss-coordinator <ADDR>           Fluss coordinator address
      --auth-type <TYPE>                   Auth type: none | passthrough
      --sasl-username <USER>               SASL username (fallback for "none" mode)
      --sasl-password <PASS>               SASL password (fallback for "none" mode)
      --config <PATH>                      Config file path
      --pool-max-connections <N>           Max connections in the pool
      --pool-idle-timeout-secs <N>         Idle timeout in seconds
      --log-level <LEVEL>                  Log level: debug | info | warn | error
  -h, --help                               Print help
```

---

## Method 5: Lifecycle Management

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

## Verifying the Deployment

```bash
# Health check
curl http://localhost:8080/health

# Query databases
curl http://localhost:8080/v1/_databases

# Create a database
curl -X POST http://localhost:8080/v1/_databases \
  -H 'Content-Type: application/json' \
  -d '{"database_name":"my_db","ignore_if_exists":true}'

# Create a log table
curl -X POST http://localhost:8080/v1/my_db/_tables \
  -H 'Content-Type: application/json' \
  -d '{
    "table_name":"my_log",
    "schema":[
      {"name":"id","data_type":"int"},
      {"name":"name","data_type":"string"},
      {"name":"value","data_type":"bigint"}
    ],
    "ignore_if_exists":true
  }'

# Write data
curl -X POST http://localhost:8080/v1/my_db/my_log/rows \
  -H 'Content-Type: application/json' \
  -d '{"rows":[{"values":[1,"Alice",100]},{"values":[2,"Bob",200]}]}'

# Scan data
curl -X POST http://localhost:8080/v1/my_db/my_log/scan \
  -H 'Content-Type: application/json' \
  -d '{"timeout_ms":5000}'
```

---

## Common Operations

### systemd

```bash
sudo systemctl restart fluss-gateway   # Restart
sudo systemctl stop fluss-gateway      # Stop
sudo systemctl disable fluss-gateway   # Disable auto-start
sudo journalctl -u fluss-gateway -n 50 # View last 50 log lines
```

### Docker

```bash
docker compose up -d --force-recreate  # Recreate
docker compose logs -f gateway         # View logs
docker compose down                    # Stop and clean up
```

### Debug Mode

systemd: Edit `/etc/fluss-gateway/gateway.toml`, change `log.level` to `"debug"`, then `sudo systemctl restart fluss-gateway`.

Docker: Add environment variable or mount a debug config file.

---

## Troubleshooting

| Issue | Cause | Solution |
|-------|-------|----------|
| `Connection refused` on 9123 | Fluss Coordinator not running or not exposed | Check Fluss cluster status |
| `Exec format error` | Binary architecture mismatch (macOS binary on Linux) | Recompile on Linux or use Dockerfile.standalone |
| Gateway exits immediately after start | Cannot connect to Fluss Coordinator | Verify `coordinator` address in config |
| Health check returns ok but API fails | Fluss `advertised.listeners` misconfigured | Ensure the container can resolve the coordinator address |
| `unknown subcommand 'server'` | Typo in command | Use `fluss-gateway serve` (not `server`) |
