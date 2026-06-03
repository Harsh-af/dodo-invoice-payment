FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY invoice-service/Cargo.toml invoice-service/
COPY mock-psp/Cargo.toml mock-psp/
RUN mkdir -p invoice-service/src mock-psp/src && \
    echo "fn main() {}" > invoice-service/src/main.rs && \
    echo "fn main() {}" > mock-psp/src/main.rs && \
    cargo build --release -p invoice-service 2>/dev/null || true
COPY invoice-service ./invoice-service
COPY mock-psp ./mock-psp
COPY migrations ./migrations
RUN touch invoice-service/src/main.rs mock-psp/src/main.rs && \
    cargo build --release -p invoice-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/invoice-service /usr/local/bin/
COPY migrations /migrations
ENV RUST_LOG=invoice_service=info
EXPOSE 8080
CMD ["invoice-service"]
