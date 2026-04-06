#!/usr/bin/env bash
# Fluss Gateway installer for Linux.
#
# Usage:
#   # From downloaded release tarball (recommended):
#   tar xzf fluss-gateway-x86_64-linux.tar.gz
#   sudo bash install.sh
#
#   # From source tree (requires cargo):
#   sudo bash install.sh
#
#   # With options:
#   sudo bash install.sh --coordinator=coordinator-server:9123 --port=8080
#
# This script:
#   1. Detects architecture (x86_64 / aarch64)
#   2. Uses pre-built binary if present, otherwise builds from source
#   3. Creates system user, config dir, log dir
#   4. Installs binary, config, systemd service
#   5. Enables and starts the service
#
# Requirements:
#   - Linux (x86_64 or aarch64)
#   - systemd
#   - curl (for health checks)
#   - cargo/rust toolchain (only if building from source)

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
COORDINATOR="localhost:9123"
PORT=8080
AUTH_TYPE="none"
LOG_LEVEL="info"
MAX_CONNECTIONS=500

INSTALL_BIN="/usr/local/bin/fluss-gateway"
INSTALL_CONFIG_DIR="/etc/fluss-gateway"
INSTALL_CONFIG="$INSTALL_CONFIG_DIR/gateway.toml"
INSTALL_SERVICE="/etc/systemd/system/fluss-gateway.service"
SERVICE_USER="fluss"
LOG_DIR="/var/log/fluss-gateway"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()     { echo -e "${GREEN}[fluss-gateway]${NC} $*"; }
warn()    { echo -e "${YELLOW}[fluss-gateway]${NC} WARN: $*"; }
error()   { echo -e "${RED}[fluss-gateway]${NC} ERROR: $*" >&2; }

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
for arg in "$@"; do
    case "$arg" in
        --coordinator=*) COORDINATOR="${arg#*=}" ;;
        --port=*)        PORT="${arg#*=}" ;;
        --auth-type=*)   AUTH_TYPE="${arg#*=}" ;;
        --log-level=*)   LOG_LEVEL="${arg#*=}" ;;
        --help|-h)
            echo "Usage: sudo bash install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --coordinator=ADDR   Fluss coordinator address (default: localhost:9123)"
            echo "  --port=N             HTTP port (default: 8080)"
            echo "  --auth-type=TYPE     Auth mode: none | passthrough (default: none)"
            echo "  --log-level=LEVEL    Log level: debug | info | warn | error (default: info)"
            echo "  --help, -h           Show this help"
            exit 0
            ;;
        *) error "Unknown option: $arg"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Check prerequisites
# ---------------------------------------------------------------------------
check_root() {
    if [[ $EUID -ne 0 ]]; then
        error "This script must be run as root (use sudo)."
        exit 1
    fi
}

check_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64|aarch64) log "Detected architecture: $arch" ;;
        *) error "Unsupported architecture: $arch (need x86_64 or aarch64)"; exit 1 ;;
    esac
}

check_systemd() {
    if ! command -v systemctl &>/dev/null; then
        error "systemd is required but not found."
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# Build binary if needed
# ---------------------------------------------------------------------------
build_binary() {
    # Check 1: pre-built binary in same directory as script (release tarball)
    if [[ -x "$SCRIPT_DIR/fluss-gateway" ]]; then
        log "Found pre-built binary: $SCRIPT_DIR/fluss-gateway"
        BINARY_SOURCE="release"
        return 0
    fi

    # Check 2: pre-built binary from cargo target (source tree)
    if [[ -x "$PROJECT_DIR/target/release/fluss-gateway" ]]; then
        log "Found pre-built binary: $PROJECT_DIR/target/release/fluss-gateway"
        BINARY_SOURCE="source"
        return 0
    fi

    # Check 3: build from source
    if ! command -v cargo &>/dev/null; then
        error "No pre-built binary found and cargo is not installed."
        error ""
        error "Options:"
        error "  1. Download a release tarball from: https://github.com/apache/fluss-gateway/releases"
        error "  2. Build first: cd $PROJECT_DIR && cargo build --release"
        error "  3. Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        error "  4. Use Docker: see deploy/docker/docker-compose.standalone.yml"
        exit 1
    fi

    log "Building fluss-gateway from source (this may take a few minutes)..."
    cd "$PROJECT_DIR"
    cargo build --release
    BINARY_SOURCE="source"
    log "Build complete."
}

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------
install_binary() {
    log "Installing binary to $INSTALL_BIN"
    case "$BINARY_SOURCE" in
        release)  cp "$SCRIPT_DIR/fluss-gateway" "$INSTALL_BIN" ;;
        source)   cp "$PROJECT_DIR/target/release/fluss-gateway" "$INSTALL_BIN" ;;
    esac
    chmod 755 "$INSTALL_BIN"
}

create_user() {
    if id "$SERVICE_USER" &>/dev/null; then
        log "User '$SERVICE_USER' already exists, skipping."
    else
        log "Creating system user '$SERVICE_USER'..."
        useradd --system --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

install_config() {
    if [[ -f "$INSTALL_CONFIG" ]]; then
        warn "Config file already exists at $INSTALL_CONFIG, skipping."
        log "To reinstall, remove it first: sudo rm $INSTALL_CONFIG"
        return 0
    fi

    log "Creating config directory: $INSTALL_CONFIG_DIR"
    mkdir -p "$INSTALL_CONFIG_DIR"

    log "Installing config to $INSTALL_CONFIG"
    cat > "$INSTALL_CONFIG" <<TOML
[server]
host = "0.0.0.0"
port = $PORT

[fluss]
coordinator = "$COORDINATOR"

[auth]
type = "$AUTH_TYPE"

[pool]
max_connections = $MAX_CONNECTIONS
idle_timeout_secs = 600

[log]
level = "$LOG_LEVEL"
TOML

    chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL_CONFIG_DIR"
}

install_service() {
    log "Installing systemd service to $INSTALL_SERVICE"

    # Create log directory
    mkdir -p "$LOG_DIR"
    chown "$SERVICE_USER:$SERVICE_USER" "$LOG_DIR"

    cat > "$INSTALL_SERVICE" <<'SERVICE'
[Unit]
Description=Fluss Gateway - REST API Gateway for Apache Fluss
Documentation=https://github.com/apache/fluss
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=fluss
Group=fluss

ExecStart=/usr/local/bin/fluss-gateway serve --config=/etc/fluss-gateway/gateway.toml
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/fluss-gateway

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=fluss-gateway

[Install]
WantedBy=multi-user.target
SERVICE

    log "Reloading systemd..."
    systemctl daemon-reload
    log "Enabling fluss-gateway service..."
    systemctl enable fluss-gateway
    log "Starting fluss-gateway..."
    systemctl start fluss-gateway
}

# ---------------------------------------------------------------------------
# Post-install check
# ---------------------------------------------------------------------------
post_check() {
    local elapsed=0
    local timeout=30
    local health_url="http://localhost:${PORT}/health"

    log "Waiting for gateway to become ready..."
    while [[ $elapsed -lt $timeout ]]; do
        if curl -sf "$health_url" >/dev/null 2>&1; then
            log "Gateway is ready!"
            log ""
            log "  Health: $(curl -sf "$health_url")"
            log "  Config: $INSTALL_CONFIG"
            log "  Logs:   journalctl -u fluss-gateway -f"
            log ""
            log "Quick test:"
            log "  curl $health_url"
            log "  curl http://localhost:${PORT}/v1/_databases"
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done

    warn "Gateway did not become ready within ${timeout}s."
    warn "Check logs: sudo journalctl -u fluss-gateway -n 50"
    return 1
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    log "Fluss Gateway Installer"
    log "====================="
    log "Coordinator: $COORDINATOR"
    log "Port:        $PORT"
    log "Auth:        $AUTH_TYPE"
    log ""

    check_root
    check_arch
    check_systemd

    build_binary
    install_binary
    create_user
    install_config
    install_service
    post_check
}

main
