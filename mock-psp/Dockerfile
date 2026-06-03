FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY invoice-service/Cargo.toml invoice-service/
COPY mock-psp/Cargo.toml mock-psp/
RUN mkdir -p invoice-service/src mock-psp/src && \
    echo "fn main() {}" > invoice-service/src/main.rs && \
    echo "fn main() {}" > mock-psp/src/main.rs && \
    cargo build --release -p mock-psp 2>/dev/null || true
COPY mock-psp ./mock-psp
RUN touch mock-psp/src/main.rs && cargo build --release -p mock-psp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/mock-psp /usr/local/bin/
EXPOSE 8081
CMD ["mock-psp"]
