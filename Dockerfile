# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev tini && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY agents ./agents
COPY packages ./packages
COPY scripts ./scripts
COPY third_party ./third_party
# Optional build args for dev environments to speed up compilation
# Example: docker build --build-arg LTO=false --build-arg CODEGEN_UNITS=16 .
ARG LTO=true
ARG CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_LTO=${LTO} \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CODEGEN_UNITS}
RUN cargo build --release --bin openfang

FROM node:20-bookworm-slim AS node_runtime

# Keep runtime network-free: Python provides the healthcheck interpreter,
# and node/npm are copied from a dedicated stage for JS-based helpers.
FROM python:3.12-slim-bookworm
RUN useradd --system --create-home --home-dir /home/openfang --shell /usr/sbin/nologin openfang \
    && install -d -o openfang -g openfang /data /opt/openfang /app /app/data /app/.runtime/models /var/lib/telegram-bot-api

# Copy tini from builder stage (installed via apt in builder)
COPY --from=builder /usr/bin/tini /usr/bin/tini
COPY --from=node_runtime /usr/local/bin/node /usr/local/bin/node
COPY --from=node_runtime /usr/local/bin/npm /usr/local/bin/npm
COPY --from=node_runtime /usr/local/bin/npx /usr/local/bin/npx
COPY --from=node_runtime /usr/local/bin/corepack /usr/local/bin/corepack
COPY --from=node_runtime /usr/local/lib/node_modules /usr/local/lib/node_modules

COPY --from=builder --chown=openfang:openfang /build/target/release/openfang /usr/local/bin/openfang
COPY --from=builder --chown=openfang:openfang /build/scripts/healthcheck-openfang.py /usr/local/bin/healthcheck-openfang.py
COPY --from=builder --chown=openfang:openfang /build/agents /opt/openfang/agents
COPY --chown=openfang:openfang projects/shipinbot/pyproject.toml /app/pyproject.toml
COPY --chown=openfang:openfang projects/shipinbot/app /app/app
COPY --chown=openfang:openfang projects/shipinbot/config /app/config
COPY --chown=openfang:openfang projects/shipinbot/scripts /app/scripts
COPY --chown=openfang:openfang projects/shipinbot/openfang-hand /app/openfang-hand
COPY --chown=openfang:openfang projects/shipinbot/.runtime/lock.yaml /app/.runtime/lock.yaml
COPY --chown=openfang:openfang projects/shipinbot/.runtime/models/manifest.yaml /app/.runtime/models/manifest.yaml
COPY --chown=openfang:openfang scripts/docker/start-openfang-container.sh /usr/local/bin/start-openfang-container.sh
COPY --chown=openfang:openfang scripts/docker/bootstrap-shipinfabu-hand.py /usr/local/bin/bootstrap-shipinfabu-hand.py
RUN printf '# shipinbot runtime\n' > /app/README.md \
    && pip install --no-cache-dir /app \
    && chmod 0755 /usr/local/bin/start-openfang-container.sh /usr/local/bin/bootstrap-shipinfabu-hand.py
EXPOSE 4200
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD python3 /usr/local/bin/healthcheck-openfang.py
STOPSIGNAL SIGTERM
ENV HOME=/home/openfang \
    OPENFANG_HOME=/data \
    SHIPINBOT_DEPLOY_ROOT=/app
WORKDIR /data
USER openfang
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/start-openfang-container.sh"]
CMD ["start"]
