# --- BUILDER ---
FROM rust:1.90-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.toml

COPY src ./src
RUN cargo build --release

# --- RUNTIME ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libicu-dev \
    libcurl4 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rcon-console /usr/local/bin/rcon-console
COPY entrypoint.sh /entrypoint.sh
COPY scripts /scripts
RUN chmod +x /entrypoint.sh /usr/local/bin/rcon-console /scripts/*.sh

ENTRYPOINT ["/bin/bash", "/entrypoint.sh"]

