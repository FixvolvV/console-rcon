# --- BUILDER ---
FROM rust:latest AS builder

WORKDIR /app
COPY Cargo.toml Cargo.toml

COPY src ./src
RUN cargo build --release

# --- RUNTIME ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/console-rcon /usr/local/bin/console-rcon
COPY entrypoint.sh /entrypoint.sh
COPY scripts /scripts
RUN chmod +x /entrypoint.sh /usr/local/bin/console-rcon /scripts/*.sh

WORKDIR /root/game

ENTRYPOINT ["/entrypoint.sh"]

