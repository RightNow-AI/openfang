# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY agents ./agents
COPY packages ./packages
RUN cargo build --release --bin openfang

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/openfang /usr/local/bin/
COPY --from=builder /build/agents /opt/openfang/agents

# OCI labels — org.opencontainers.image.source links the GHCR package to this
# repository so the package can inherit the repo's public visibility.
LABEL org.opencontainers.image.source="https://github.com/RightNow-AI/openfang" \
      org.opencontainers.image.description="OpenFang — Open-Source Agent Operating System" \
      org.opencontainers.image.licenses="Apache-2.0 OR MIT"

EXPOSE 4200
VOLUME /data
ENV OPENFANG_HOME=/data
ENTRYPOINT ["openfang"]
CMD ["start"]
