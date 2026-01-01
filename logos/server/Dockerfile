FROM rust:latest AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY common/Cargo.toml common/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY client/Cargo.toml client/Cargo.toml

RUN mkdir -p common/src server/src client/src && \
    echo "fn main() {}" > server/src/main.rs && \
    echo "fn main() {}" > client/src/main.rs && \
    touch common/src/lib.rs

RUN cargo build --release --bin server

COPY common common
COPY server server

RUN touch common/src/lib.rs && touch server/src/main.rs && cargo build --release --bin server

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/server /app/server
CMD ["/app/server"]