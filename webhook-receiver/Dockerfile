FROM rust:bookworm AS builder
WORKDIR /app
COPY webhook-receiver/Cargo.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release 2>/dev/null || true
COPY webhook-receiver/src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/webhook-receiver /usr/local/bin/
EXPOSE 8090
CMD ["webhook-receiver"]
