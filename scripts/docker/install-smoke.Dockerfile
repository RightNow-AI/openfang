# Smoke test for install.sh
# Verifies the installer works in a clean environment.
#
# Usage (CI):
#   docker build -f scripts/docker/install-smoke.Dockerfile .
#
# Usage (full E2E — requires a published release):
#   docker build -f scripts/docker/install-smoke.Dockerfile \
#     --build-arg OPENFANG_SMOKE_FULL=1 .

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    bash \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user (simulates real user install)
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

# Copy the install script from the build context
COPY scripts/install.sh /tmp/install.sh

ARG OPENFANG_SMOKE_FULL=0
RUN if [ "$OPENFANG_SMOKE_FULL" = "1" ]; then \
        bash /tmp/install.sh; \
    else \
        # 1. Syntax check
        bash -n /tmp/install.sh && \
        echo "PASS: install.sh syntax is valid" && \
        # 2. Verify detect_platform works by extracting the function
        bash -c ' \
            eval "$(sed -n "/^detect_platform/,/^}/p" /tmp/install.sh)" && \
            detect_platform && \
            echo "PASS: platform detected as $PLATFORM" \
        ' && \
        # 3. Verify target matches release naming (must contain -unknown-linux-musl)
        bash -c ' \
            eval "$(sed -n "/^detect_platform/,/^}/p" /tmp/install.sh)" && \
            detect_platform && \
            echo "$PLATFORM" | grep -q "linux-musl" && \
            echo "PASS: target is musl (matches release.yml)" \
        '; \
    fi

# If full install succeeded, verify the binary works
RUN if [ "$OPENFANG_SMOKE_FULL" = "1" ] && [ -f "$HOME/.openfang/bin/openfang" ]; then \
        $HOME/.openfang/bin/openfang --version && \
        echo "PASS: openfang binary works"; \
    else \
        echo "SKIP: binary verification (no full install)"; \
    fi
