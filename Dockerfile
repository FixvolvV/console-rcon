FROM rust:1.83-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.toml

RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/scpsl_wrapper*

COPY src ./src

RUN cargo build --release

RUN ./target/release/scpsl-wrapper --help


FROM debian:trixie-slim AS runtime

s
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    libc6 \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /root/game /root/.config/SCP\ Secret\ Laboratory

COPY --from=builder /app/target/release/scpsl-wrapper /usr/local/bin/scpsl-wrapper

RUN chmod +x /usr/local/bin/scpsl-wrapper

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /root/game

ENTRYPOINT ["/entrypoint.sh"]

