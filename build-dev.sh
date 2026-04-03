#!/usr/bin/env bash
# Fast dev build: compile on host (incremental), package into image.
set -e
cd "$(dirname "$0")"
cargo build --release --bin openfang
docker build -t openfang-gw:latest -f Dockerfile.dev .
