# syntax=docker/dockerfile:1
#
# NOTE: fluss-rust is a local path dependency, so this Dockerfile expects the
# binary to be built locally first. The build step happens on the host machine,
# and this Dockerfile only packages the resulting binary into a minimal image.
#
# Usage:
#   1. Build the binary: cargo build --release
#   2. Build the image:  docker build -f deploy/docker/Dockerfile .
#   3. Or use docker compose: docker compose -f deploy/docker/docker-compose.prod.yml up

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY target/release/fluss-gateway /usr/local/bin/

EXPOSE 8080

ENTRYPOINT ["fluss-gateway"]
CMD ["--host", "0.0.0.0", "--port", "8080"]
