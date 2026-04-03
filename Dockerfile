# syntax=docker/dockerfile:1
FROM rust:1.85-slim AS builder

WORKDIR /app

# Build dependencies first for layer caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Copy actual source and rebuild
COPY src/ src/
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/fluss-gateway /usr/local/bin/

EXPOSE 8080

ENTRYPOINT ["fluss-gateway"]
CMD ["--host", "0.0.0.0", "--port", "8080"]
