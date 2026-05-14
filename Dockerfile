FROM rust:1.95-slim AS rust-builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p uncode-cli -p uncode-platform

FROM oven/bun:1 AS frontend-builder

WORKDIR /app
COPY apps/platform/package.json apps/platform/bun.lock ./
RUN bun install
COPY apps/platform/ ./
COPY biome.json /app/biome.json
RUN bun run build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/uncode /usr/local/bin/
COPY --from=rust-builder /app/target/release/uncode-platform /usr/local/bin/
COPY --from=frontend-builder /app/dist /usr/local/share/uncode/dist

ENV UNCODE_FRONTEND_DIR=/usr/local/share/uncode/dist
EXPOSE 3000
ENTRYPOINT ["uncode"]
