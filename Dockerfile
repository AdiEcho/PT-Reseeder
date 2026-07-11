# Stage 1: cargo-leptos produces hydrate-WASM + SSR binary
FROM rust:1.87-bookworm AS builder

# Build dependencies (SQLite for sqlx compile-time checks)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy cargo config for rsproxy mirror (needed for China network)
COPY .cargo/config.toml /usr/local/cargo/config.toml

RUN rustup target add wasm32-unknown-unknown
RUN cargo install cargo-leptos --locked

WORKDIR /app
COPY . .
RUN cargo leptos build --release

# Stage 2: Runtime — full rustls chain, no libssl needed
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pt-reseeder-server /usr/local/bin/
COPY --from=builder /app/target/site /opt/pt-reseeder/site
COPY --from=builder /app/crates/frontend/index.html /opt/pt-reseeder/site/index.html
COPY --from=builder /app/migrations /opt/pt-reseeder/migrations

ENV LEPTOS_SITE_ROOT=/opt/pt-reseeder/site \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    DATABASE_URL=sqlite:///data/pt-reseeder.db \
    PT_RESEEDER_DATA_DIR=/data

EXPOSE 3000
VOLUME ["/data"]

# slim image has no curl — self-check via TCP connect
HEALTHCHECK --interval=30s --timeout=5s CMD ["pt-reseeder-server", "--healthcheck"]
ENTRYPOINT ["pt-reseeder-server"]
