# Stage 1a: Build React frontend with Vite
FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# Stage 1b: Build Rust server binary
FROM rust:1.87-bookworm AS builder

# Build dependencies (SQLite for sqlx compile-time checks)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy cargo config for rsproxy mirror (needed for China network)
COPY .cargo/config.toml /usr/local/cargo/config.toml

WORKDIR /app
COPY . .
RUN cargo build --release --features headless-browser -p pt-reseeder-server

# Stage 2: Runtime — full rustls chain, no libssl needed
FROM debian:bookworm-slim

ARG PT_RESEEDER_UID=10001
ARG PT_RESEEDER_GID=10001

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libsqlite3-0 chromium \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid "${PT_RESEEDER_GID}" pt-reseeder \
    && useradd --uid "${PT_RESEEDER_UID}" --gid "${PT_RESEEDER_GID}" \
        --create-home --home-dir /home/pt-reseeder --shell /usr/sbin/nologin pt-reseeder \
    && install -d -o pt-reseeder -g pt-reseeder \
        /data \
        /home/pt-reseeder/.cache \
        /home/pt-reseeder/.cache/chromium \
        /home/pt-reseeder/.config \
        /home/pt-reseeder/.config/chromium

COPY --from=frontend-builder /app/web/dist /opt/pt-reseeder/site
COPY --from=builder /app/target/release/pt-reseeder-server /usr/local/bin/
COPY --from=builder /app/migrations /opt/pt-reseeder/migrations

ENV SITE_ROOT=/opt/pt-reseeder/site \
    SERVER_BIND=0.0.0.0:3000 \
    DATABASE_URL=sqlite:///data/pt-reseeder.db \
    DATA_DIR=/data \
    LOG_DIR=/data/logs \
    CHROME_PATH=/usr/bin/chromium \
    HOME=/home/pt-reseeder \
    XDG_CACHE_HOME=/home/pt-reseeder/.cache \
    XDG_CONFIG_HOME=/home/pt-reseeder/.config \
    PT_RESEEDER_CHROME_NO_SANDBOX=false

EXPOSE 3000
VOLUME ["/data"]

# slim image has no curl — self-check via TCP connect
HEALTHCHECK --interval=30s --timeout=5s CMD ["pt-reseeder-server", "--healthcheck"]
USER pt-reseeder:pt-reseeder
ENTRYPOINT ["pt-reseeder-server"]
