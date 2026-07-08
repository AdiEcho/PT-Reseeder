# Stage 1: cargo-leptos produces hydrate-WASM + ssr binary
FROM rust:1.88-bookworm AS builder
# Copy cargo config for rsproxy mirror (needed for China network)
COPY .cargo/config.toml /usr/local/cargo/config.toml
RUN rustup target add wasm32-unknown-unknown
RUN cargo install cargo-leptos --locked
WORKDIR /app
COPY . .
RUN cargo leptos build --release

# Stage 2: Runtime — full rustls chain, no libssl needed
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pt-reseeder-server /usr/local/bin/
COPY --from=builder /app/target/site /opt/pt-reseeder/site
ENV LEPTOS_SITE_ROOT=/opt/pt-reseeder/site \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    DATABASE_URL=sqlite:///data/pt-reseeder.db
EXPOSE 3000
VOLUME ["/data"]
# slim image has no curl → self-check subcommand hits /api/health
HEALTHCHECK --interval=30s --timeout=5s CMD ["pt-reseeder-server", "--healthcheck"]
CMD ["pt-reseeder-server", "--headless"]
