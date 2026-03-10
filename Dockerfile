# syntax=docker/dockerfile:1
# ─────────────────────────────────────────────────────────────────────────────
# Stage 1: Dependency cache layer
# Pre-fetch the crate registry so that subsequent builds only recompile changed
# crates, not all dependencies.
# ─────────────────────────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS deps
WORKDIR /build

# Install build-time system dependencies.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests only — allows Docker layer caching of compiled deps.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask

# Pre-fetch the dependency graph (cached as long as Cargo.lock doesn't change).
RUN cargo fetch

# ─────────────────────────────────────────────────────────────────────────────
# Stage 2: Full release build
# ─────────────────────────────────────────────────────────────────────────────
FROM deps AS builder

# Copy remaining source files.
COPY agents ./agents
COPY packages ./packages

# Build the release binary with all optimisations.
RUN cargo build --release --bin openfang

# Strip debug symbols to reduce image size (~60% smaller binary).
RUN strip /build/target/release/openfang

# ─────────────────────────────────────────────────────────────────────────────
# Stage 3: Minimal production runtime image
# ─────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Install only the runtime libraries needed by the binary.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for security.
RUN groupadd --gid 1001 openfang \
    && useradd --uid 1001 --gid openfang --shell /bin/sh --create-home openfang

# Copy the binary and bundled agents from the builder stage.
COPY --from=builder /build/target/release/openfang /usr/local/bin/openfang
COPY --from=builder /build/agents /opt/openfang/agents

# Ensure data directory exists and is owned by the non-root user.
RUN mkdir -p /data && chown -R openfang:openfang /data

# Switch to non-root user.
USER openfang

# Expose the API port.
EXPOSE 4200

# Persist data (SurrealDB files, config, agent state) outside the container.
VOLUME /data

# Environment defaults.
ENV OPENFANG_HOME=/data

# Liveness probe: checks that the HTTP server is responding.
# Readiness probe is handled by /api/ready (checked by docker-compose / k8s).
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://localhost:4200/api/health | grep -q '"status"' || exit 1

ENTRYPOINT ["openfang"]
CMD ["start"]
