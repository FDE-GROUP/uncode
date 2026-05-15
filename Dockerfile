# ── Stage 1: Build Rust backend ─────────────────────────────────
FROM rust:1.85-slim AS rust-builder

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/uncode-core/Cargo.toml crates/uncode-core/Cargo.toml
COPY crates/uncode-macros/Cargo.toml crates/uncode-macros/Cargo.toml
COPY crates/uncode-llm/Cargo.toml crates/uncode-llm/Cargo.toml
COPY crates/uncode-session/Cargo.toml crates/uncode-session/Cargo.toml
COPY crates/uncode-tools/Cargo.toml crates/uncode-tools/Cargo.toml
COPY crates/uncode-extensions/Cargo.toml crates/uncode-extensions/Cargo.toml
COPY crates/uncode-agent/Cargo.toml crates/uncode-agent/Cargo.toml
COPY crates/uncode-tui/Cargo.toml crates/uncode-tui/Cargo.toml
COPY crates/uncode-platform/Cargo.toml crates/uncode-platform/Cargo.toml
COPY crates/uncode-cli/Cargo.toml crates/uncode-cli/Cargo.toml
COPY crates/uncode-rpc/Cargo.toml crates/uncode-rpc/Cargo.toml

# Create dummy source files for dependency caching
RUN for d in crates/uncode-*/; do \
      mkdir -p "$d/src" && \
      echo "fn main() {}" > "$d/src/main.rs" 2>/dev/null; \
      echo "" > "$d/src/lib.rs" 2>/dev/null; \
    done

RUN cargo build --release -p uncode-cli -p uncode-platform 2>/dev/null || true

# Copy real source and rebuild
COPY crates/ crates/
RUN cargo build --release -p uncode-cli -p uncode-platform

# ── Stage 2: Build frontend ─────────────────────────────────────
FROM oven/bun:1 AS frontend-builder

WORKDIR /app/apps/platform
COPY apps/platform/package.json apps/platform/bun.lock ./
RUN bun install --frozen-lockfile

COPY apps/platform/ ./
RUN bun run build

# ── Stage 3: Runtime ─────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates git && \
    rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/uncode /usr/local/bin/
COPY --from=rust-builder /app/target/release/uncode-platform /usr/local/bin/
COPY --from=frontend-builder /app/apps/platform/dist /usr/local/share/uncode/dist

ENV UNCODE_FRONTEND_DIR=/usr/local/share/uncode/dist
EXPOSE 3000

ENTRYPOINT ["uncode"]
