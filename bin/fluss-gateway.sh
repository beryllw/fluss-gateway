#!/usr/bin/env bash
# Fluss Gateway lifecycle management script.
#
# Usage:
#   fluss-gateway.sh start   [--pid-file=PATH] [--port=N] [--fluss-coordinator=ADDR] [...]
#   fluss-gateway.sh stop    [--pid-file=PATH]
#   fluss-gateway.sh restart [--pid-file=PATH] [--port=N] [--fluss-coordinator=ADDR] [...]
#   fluss-gateway.sh status  [--pid-file=PATH]
#
# The script invokes the fluss-gateway binary (built or via cargo).

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

DEFAULT_PID_FILE="/tmp/fluss-gateway.pid"
DEFAULT_HEALTH_URL="http://localhost:8080/health"

# Resolve the gateway binary
# Prefer a pre-built binary from cargo target, fall back to running via cargo
if [[ -x "$PROJECT_DIR/target/release/fluss-gateway" ]]; then
    BINARY="$PROJECT_DIR/target/release/fluss-gateway"
    RUN_CMD=("$BINARY")
elif [[ -x "$PROJECT_DIR/target/debug/fluss-gateway" ]]; then
    BINARY="$PROJECT_DIR/target/debug/fluss-gateway"
    RUN_CMD=("$BINARY")
else
    RUN_CMD=(cargo run --release --)
fi

PID_FILE="$DEFAULT_PID_FILE"
HEALTH_URL="$DEFAULT_HEALTH_URL"
HEALTH_TIMEOUT=30
HEALTH_INTERVAL=1

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }
warn() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] WARN: $*" >&2; }
die()  { echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" >&2; exit 1; }

# Parse a --key=value or --key value pair from args and return the value.
# Returns empty string if not found.
get_opt() {
    local key="$1"
    shift
    for arg in "$@"; do
        case "$arg" in
            --pid-file=*) echo "${arg#--pid-file=}"; return ;;
        esac
    done
    echo ""
}

# Check if a PID is alive.
is_alive() { kill -0 "$1" 2>/dev/null; }

# Wait for the health endpoint to respond.
wait_for_health() {
    local elapsed=0
    while [[ $elapsed -lt $HEALTH_TIMEOUT ]]; do
        if curl -sf "$HEALTH_URL" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$HEALTH_INTERVAL"
        elapsed=$((elapsed + HEALTH_INTERVAL))
    done
    return 1
}

# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------
do_start() {
    local args=("$@")

    # Strip leading '--' separator if present (user convention: start -- --flags)
    if [[ "${args[0]:-}" == "--" ]]; then
        args=("${args[@]:1}")
    fi

    # Extract port for health check
    local port
    port=$(get_opt --port "${args[@]}")
    port="${port:-8080}"
    HEALTH_URL="http://localhost:${port}/health"

    # Parse PID file
    local pf
    pf=$(get_opt --pid-file "${args[@]}")
    if [[ -n "$pf" ]]; then PID_FILE="$pf"; fi

    # Remove --pid-file from args before passing to the binary
    local serve_args=()
    for arg in "${args[@]}"; do
        case "$arg" in
            --pid-file=*) ;;
            *) serve_args+=("$arg") ;;
        esac
    done

    # Check if already running
    if [[ -f "$PID_FILE" ]]; then
        local old_pid
        old_pid=$(cat "$PID_FILE")
        if is_alive "$old_pid"; then
            die "Gateway is already running (PID: $old_pid). Stop it first."
        fi
        rm -f "$PID_FILE"
    fi

    log "Starting fluss-gateway..."
    nohup "${RUN_CMD[@]}" serve "${serve_args[@]}" >/tmp/fluss-gateway.out 2>/tmp/fluss-gateway.err &
    local pid=$!

    # Write PID file
    echo "$pid" > "$PID_FILE"
    log "PID file written: $PID_FILE (PID: $pid)"

    # Wait for health
    log "Waiting for health endpoint ($HEALTH_URL)..."
    if wait_for_health; then
        log "Gateway started successfully (PID: $pid)"
    else
        warn "Gateway PID $pid created but health check timed out after ${HEALTH_TIMEOUT}s"
        log "Check /tmp/fluss-gateway.out and /tmp/fluss-gateway.err for details"
    fi
}

do_stop() {
    local pf
    pf=$(get_opt --pid-file "$@")
    if [[ -n "$pf" ]]; then PID_FILE="$pf"; fi

    if [[ ! -f "$PID_FILE" ]]; then
        die "PID file not found: $PID_FILE. Is the gateway running?"
    fi

    local pid
    pid=$(cat "$PID_FILE")

    if ! is_alive "$pid"; then
        log "Process $pid is not running. Cleaning up stale PID file."
        rm -f "$PID_FILE"
        return 0
    fi

    log "Stopping fluss-gateway (PID: $pid)..."
    kill -TERM "$pid"

    # Wait for process to exit (up to 15 seconds)
    local elapsed=0
    while is_alive "$pid" && [[ $elapsed -lt 15 ]]; do
        sleep 1
        elapsed=$((elapsed + 1))
    done

    if is_alive "$pid"; then
        warn "Process did not exit gracefully, sending SIGKILL..."
        kill -9 "$pid" 2>/dev/null || true
        sleep 1
    fi

    rm -f "$PID_FILE"
    log "Gateway stopped."
}

do_restart() {
    local args=("$@")
    do_stop "${args[@]}" 2>/dev/null || true
    sleep 1
    # Strip leading '--' from start args in restart
    local start_args=("${args[@]}")
    if [[ "${start_args[0]:-}" == "--" ]]; then
        start_args=("${start_args[@]:1}")
    fi
    do_start "${start_args[@]}"
}

do_status() {
    local pf
    pf=$(get_opt --pid-file "$@")
    if [[ -n "$pf" ]]; then PID_FILE="$pf"; fi

    local port
    port=$(get_opt --port "$@")
    port="${port:-8080}"
    HEALTH_URL="http://localhost:${port}/health"

    if [[ ! -f "$PID_FILE" ]]; then
        echo "Gateway is not running."
        echo "PID file: $PID_FILE"
        return 1
    fi

    local pid
    pid=$(cat "$PID_FILE")

    if is_alive "$pid"; then
        echo "Gateway is running (PID: $pid)"
        if curl -sf "$HEALTH_URL" >/dev/null 2>&1; then
            local resp
            resp=$(curl -sf "$HEALTH_URL")
            echo "Health: $resp"
        else
            echo "Health: UNREACHABLE"
        fi
    else
        echo "Gateway PID file exists but process $pid is not running (stale)."
        echo "PID file: $PID_FILE"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
case "${1:-}" in
    start)
        shift
        do_start "$@"
        ;;
    stop)
        shift
        do_stop "$@"
        ;;
    restart)
        shift
        do_restart "$@"
        ;;
    status)
        shift
        do_status "$@"
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status} [OPTIONS]"
        echo ""
        echo "Commands:"
        echo "  start    Start the gateway in the background"
        echo "  stop     Stop the running gateway"
        echo "  restart  Stop and start the gateway"
        echo "  status   Show the current gateway status"
        echo ""
        echo "Options:"
        echo "  --pid-file=PATH           PID file path (default: /tmp/fluss-gateway.pid)"
        echo "  --port=N                  HTTP port for health checks (default: 8080)"
        echo "  --fluss-coordinator=ADDR  Fluss coordinator address"
        echo "  --host=ADDR               Bind address"
        echo "  --auth-type=TYPE          Auth mode (none|passthrough)"
        echo "  --config=PATH             Config file path"
        echo "  (other 'fluss-gateway serve' options also supported)"
        exit 1
        ;;
esac
