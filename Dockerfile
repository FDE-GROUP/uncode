FROM rust:1.95-slim AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p uncode-cli -p uncode-platform

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/uncode /usr/local/bin/
COPY --from=builder /app/target/release/uncode-platform /usr/local/bin/

EXPOSE 3000
ENTRYPOINT ["uncode"]
